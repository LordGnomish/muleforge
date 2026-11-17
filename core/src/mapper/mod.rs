//! Mapper: transforms Mule AST -> Camel IR using YAML-defined rules and
//! optionally an LLM fallback for semantics rules cannot express.

use std::path::Path;

use crate::ast::camel_ir::*;
use crate::ast::mule_ast::{MuleElement, MuleProject};
use crate::llm::LlmProvider;
use crate::report::{DecisionStatus, MappingDecision};
use crate::Result;

pub struct Rule {
    pub id: String,
    pub mule_ns: String,
    pub mule_name: String,
    pub camel_uri_template: Option<String>,
    /// Camel endpoint options appended to the URI as a query string, in
    /// declaration order: `(option name, value template)`. A template may
    /// reference `{attr}` (an attribute of the Mule element) and
    /// `{config.attr}` (an attribute of the element's `config-ref` target).
    /// An option whose template cannot be fully resolved is left out.
    pub camel_params: Vec<(String, String)>,
    pub camel_component: Option<String>,
    pub maven_deps: Vec<MavenDependency>,
    pub notes: String,
}

/// Endpoint options of the built-in Kafka mappings (same as the
/// `camel_params` of the rules in `mappings/kafka.yaml`): brokers from the
/// `config-ref` target, consumer group from the listener itself.
const KAFKA_CONSUMER_PARAMS: &[(&str, &str)] = &[
    ("brokers", "{config.bootstrapServers}"),
    ("groupId", "{groupId}"),
];
const KAFKA_PRODUCER_PARAMS: &[(&str, &str)] = &[("brokers", "{config.bootstrapServers}")];

pub fn load_rules(mappings_dir: &Path) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();
    if !mappings_dir.is_dir() {
        return Ok(rules);
    }
    for entry in std::fs::read_dir(mappings_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml")
            || path.extension().and_then(|e| e.to_str()) == Some("yml")
        {
            let content = std::fs::read_to_string(&path)?;
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                if let Some(mappings) = yaml.get("mappings").and_then(|m| m.as_sequence()) {
                    for m in mappings {
                        let rule = Rule {
                            id: m
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            mule_ns: m
                                .get("mule_ns")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            mule_name: m
                                .get("mule_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            camel_uri_template: m
                                .get("camel_uri")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            camel_params: parse_camel_params(m.get("camel_params")),
                            camel_component: m
                                .get("camel_component")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            maven_deps: parse_maven_deps(m.get("maven_deps")),
                            notes: m
                                .get("notes")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        };
                        rules.push(rule);
                    }
                }
            }
        }
    }
    Ok(rules)
}

fn parse_camel_params(val: Option<&serde_yaml::Value>) -> Vec<(String, String)> {
    let Some(map) = val.and_then(|v| v.as_mapping()) else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
        .collect()
}

fn parse_maven_deps(val: Option<&serde_yaml::Value>) -> Vec<MavenDependency> {
    let mut deps = Vec::new();
    if let Some(seq) = val.and_then(|v| v.as_sequence()) {
        for d in seq {
            deps.push(MavenDependency {
                group_id: d
                    .get("group_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                artifact_id: d
                    .get("artifact_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                version: d
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }
    deps
}

pub async fn map_project(
    project: &MuleProject,
    rules: &[Rule],
    _llm: Option<&dyn LlmProvider>,
) -> Result<(CamelProject, Vec<MappingDecision>)> {
    let mut camel = CamelProject {
        name: project.name.clone(),
        ..Default::default()
    };
    let mut decisions = Vec::new();
    let mut all_deps: Vec<MavenDependency> = Vec::new();

    // Always add Quarkus + Camel BOM
    all_deps.push(MavenDependency {
        group_id: "org.apache.camel.quarkus".into(),
        artifact_id: "camel-quarkus-core".into(),
        version: None,
    });
    all_deps.push(MavenDependency {
        group_id: "org.apache.camel.quarkus".into(),
        artifact_id: "camel-quarkus-platform-http".into(),
        version: None,
    });

    for flow in &project.flows {
        if flow.is_sub_flow {
            // Sub-flows map to direct: endpoints
            let route = map_sub_flow(flow, &project.flows, rules, &mut decisions, &mut all_deps);
            camel.routes.push(route);
        } else {
            let route = map_flow(flow, &project.flows, rules, &mut decisions, &mut all_deps);
            camel.routes.push(route);
        }
    }

    // Sub-flows are `from("direct:...")` routes and flow-refs call them with
    // `.to("direct:...")`. Without the direct component the application does
    // not start ("No endpoint could be found for: direct://...").
    if camel.routes.iter().any(route_uses_direct) {
        all_deps.push(MavenDependency {
            group_id: "org.apache.camel.quarkus".into(),
            artifact_id: "camel-quarkus-direct".into(),
            version: None,
        });
    }

    // Deduplicate deps
    let mut seen = std::collections::HashSet::new();
    all_deps.retain(|d| seen.insert(format!("{}:{}", d.group_id, d.artifact_id)));
    camel.maven_dependencies = all_deps;

    // Convert Mule properties to Quarkus properties
    for (k, v) in &project.properties {
        camel.quarkus_properties.insert(k.clone(), v.clone());
    }

    Ok((camel, decisions))
}

/// Whether the route consumes from or sends to a `direct:` endpoint.
fn route_uses_direct(route: &CamelRoute) -> bool {
    let from_direct = match &route.source {
        RouteEndpoint::Direct(_) => true,
        RouteEndpoint::Uri(uri) => uri.starts_with("direct:"),
    };
    from_direct
        || steps_use_direct(&route.steps)
        || route
            .on_exceptions
            .iter()
            .any(|handler| steps_use_direct(&handler.steps))
}

fn steps_use_direct(steps: &[RouteStep]) -> bool {
    steps.iter().any(|step| match step {
        RouteStep::ToUri(uri) => uri.starts_with("direct:"),
        RouteStep::Choice { whens, otherwise } => {
            whens.iter().any(|(_, steps)| steps_use_direct(steps))
                || otherwise.as_deref().is_some_and(steps_use_direct)
        }
        RouteStep::Split { steps, .. } => steps_use_direct(steps),
        _ => false,
    })
}

fn map_flow(
    flow: &crate::ast::mule_ast::MuleFlow,
    flows: &[crate::ast::mule_ast::MuleFlow],
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> CamelRoute {
    let mut steps = Vec::new();
    let mut source = RouteEndpoint::Uri("direct:unknown".into());

    for (i, proc) in flow.processors.iter().enumerate() {
        if i == 0 {
            // First processor is typically the source (listener, scheduler, etc.)
            source = map_source(proc, rules, decisions, deps);
            continue;
        }
        let step = map_processor(proc, flows, rules, decisions, deps);
        steps.extend(step);
    }

    let on_exceptions = flow
        .error_handlers
        .iter()
        .flat_map(|eh| map_error_handler(eh, flows, rules, decisions, deps))
        .collect();

    CamelRoute {
        id: flow.name.clone(),
        source,
        steps,
        on_exceptions,
    }
}

/// The route of a `<sub-flow>`: `from("direct:<name>")`, called by the
/// flow-refs that name it.
///
/// A Mule sub-flow has no error handling of its own. It runs as part of the
/// flow that references it, and what fails in it is handled by that flow's
/// error handler. A Camel route always has an error handler (the default
/// one, unless told otherwise), and once that has dealt with a failure the
/// `onException` clauses of the calling route do not run for it. So this
/// route, `RouteEndpoint::Direct` without exception handlers, is the one the
/// emitter writes with `.errorHandler(noErrorHandler())`: its failures go
/// back to the calling route and to that route's clauses.
fn map_sub_flow(
    flow: &crate::ast::mule_ast::MuleFlow,
    flows: &[crate::ast::mule_ast::MuleFlow],
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> CamelRoute {
    let steps: Vec<RouteStep> = flow
        .processors
        .iter()
        .flat_map(|p| map_processor(p, flows, rules, decisions, deps))
        .collect();

    CamelRoute {
        id: flow.name.clone(),
        source: RouteEndpoint::Direct(flow.name.clone()),
        steps,
        on_exceptions: vec![],
    }
}

fn map_source(
    elem: &MuleElement,
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> RouteEndpoint {
    let qname = elem.qualified_name();

    // Try rule match first
    if let Some(rule) = find_rule(rules, &elem.namespace, &elem.name) {
        record_decision(
            decisions,
            elem,
            DecisionStatus::Done,
            Some(&rule.id),
            &rule.notes,
        );
        deps.extend(rule.maven_deps.iter().cloned());
        if let Some(ref tmpl) = rule.camel_uri_template {
            return RouteEndpoint::Uri(rule_endpoint_uri(rule, tmpl, elem));
        }
    }

    // Built-in mappings for common sources
    match (elem.namespace.as_str(), elem.name.as_str()) {
        ("http", "listener") => {
            let path = elem.attr("path").unwrap_or("/");
            let method = elem.attr("allowedMethods").unwrap_or("GET");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "HTTP listener -> platform-http endpoint",
            );
            RouteEndpoint::Uri(format!(
                "platform-http:{}?httpMethodRestrict={}",
                path, method
            ))
        }
        ("", "scheduler") | ("scheduler", "scheduler") => {
            let freq = elem.attr("frequency").unwrap_or("60000");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "Scheduler -> timer endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-timer".into(),
                version: None,
            });
            RouteEndpoint::Uri(format!("timer:scheduler?period={}", freq))
        }
        // Kafka connector 4.x has no topic on <kafka:message-listener>: the
        // subscription (topic patterns, group ID, bootstrap servers) sits on
        // the connection inside its consumer-config, which is not read here.
        // Without a topic there is no endpoint to emit, so flag the listener
        // instead of inventing a topic.
        ("kafka", "message-listener") if !has_attr_value(elem, "topic") => {
            record_decision(
                decisions,
                elem,
                DecisionStatus::ManualReview,
                None,
                "kafka:message-listener has no topic attribute - take the topic, group ID \
                 and brokers from the connection of its consumer-config and set the kafka: \
                 source manually",
            );
            RouteEndpoint::Uri(format!("direct:{}", qname.replace(':', "-")))
        }
        ("kafka", "consumer") | ("kafka", "listener") | ("kafka", "message-listener") => {
            let topic = elem.attr("topic").unwrap_or("default-topic");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "Kafka consumer -> kafka endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-kafka".into(),
                version: None,
            });
            RouteEndpoint::Uri(append_uri_params(
                format!("kafka:{}", topic),
                KAFKA_CONSUMER_PARAMS.iter().copied(),
                elem,
            ))
        }
        ("jms", "listener") => {
            let dest = elem.attr("destination").unwrap_or("default-queue");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "JMS listener -> jms endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-jms".into(),
                version: None,
            });
            RouteEndpoint::Uri(format!("jms:queue:{}", dest))
        }
        ("file", "listener") | ("file", "read") => {
            let dir = elem.attr("directory").unwrap_or("/tmp/input");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "File listener -> file endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-file".into(),
                version: None,
            });
            RouteEndpoint::Uri(format!("file:{}", dir))
        }
        _ => {
            record_decision(
                decisions,
                elem,
                DecisionStatus::ManualReview,
                None,
                &format!("Unknown source: {} - needs manual mapping", qname),
            );
            RouteEndpoint::Uri(format!("direct:{}", qname.replace(':', "-")))
        }
    }
}

/// Map one message processor to route steps.
///
/// `flows` are the flows and sub-flows of the project, as they are mapped
/// to routes: what a `flow-ref` can point at.
fn map_processor(
    elem: &MuleElement,
    flows: &[crate::ast::mule_ast::MuleFlow],
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> Vec<RouteStep> {
    let qname = elem.qualified_name();

    // Try rule match first. A rule with a URI template is the whole
    // mapping. A rule without one (`set-payload`, `choice-router`, ...)
    // only names the Camel construct: the built-in mapping below does the
    // work and records the element's one decision, under the rule's id. The
    // rule must not report the element as done on its own, the built-in
    // mapping may find that it is not.
    let mut rule_id = None;
    if let Some(rule) = find_rule(rules, &elem.namespace, &elem.name) {
        deps.extend(rule.maven_deps.iter().cloned());
        if let Some(ref uri) = rule.camel_uri_template {
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                Some(&rule.id),
                &rule.notes,
            );
            return vec![RouteStep::ToUri(rule_endpoint_uri(rule, uri, elem))];
        }
        rule_id = Some(rule.id.as_str());
    }

    // Built-in processor mappings
    match (elem.namespace.as_str(), elem.name.as_str()) {
        // The value is a string, or an expression that is converted only
        // if it means the same in Camel whatever the payload is (see
        // `mel::convert_value`). An unconverted value stays in the step as
        // `Expression::Unconverted`; the emitter makes the route fail there.
        ("", "set-payload") => {
            let expression = crate::mel::convert_value(elem.attr("value").unwrap_or(""));
            record_value_decision(
                decisions,
                elem,
                rule_id,
                &expression,
                "set-payload -> setBody",
            );
            vec![RouteStep::SetBody { expression }]
        }
        ("", "set-variable") => {
            let name = elem.attr("variableName").unwrap_or("var").to_string();
            let expression = crate::mel::convert_value(elem.attr("value").unwrap_or(""));
            record_value_decision(
                decisions,
                elem,
                rule_id,
                &expression,
                "set-variable -> setProperty",
            );
            vec![RouteStep::SetProperty { name, expression }]
        }
        ("", "logger") | ("", "log") => {
            let raw = elem.attr("message").unwrap_or("");
            let level = match elem.attr("level").unwrap_or("INFO") {
                "DEBUG" => LogLevel::Debug,
                "WARN" | "WARNING" => LogLevel::Warn,
                "ERROR" => LogLevel::Error,
                "TRACE" => LogLevel::Trace,
                _ => LogLevel::Info,
            };
            // Embedded #[...] expressions become Camel Simple ${...}; if any
            // cannot be converted, keep the original text and flag it.
            let msg = if let Some(converted) = crate::mel::interpolate_mel(raw) {
                record_decision(
                    decisions,
                    elem,
                    DecisionStatus::Done,
                    rule_id,
                    "logger -> log",
                );
                converted
            } else {
                record_decision(
                    decisions,
                    elem,
                    DecisionStatus::ManualReview,
                    rule_id,
                    "logger -> log (message has an unconverted MEL expression)",
                );
                raw.to_string()
            };
            vec![RouteStep::Log {
                level,
                message: msg,
            }]
        }
        // Mule takes the first `when` whose expression is true, else the
        // `otherwise`; so does Camel's choice. Every branch is kept, in
        // order. A condition is converted only if it means the same in
        // Camel whatever the payload is (see `mel::convert_condition`); one
        // that is not stays as `Expression::Unconverted`, which the emitter
        // writes as a predicate that fails when it is evaluated. A `when`
        // without expression is not converted either: nothing says what it
        // should match.
        ("", "choice") => {
            let mut whens = Vec::new();
            let mut otherwise = None;
            let mut unconverted = Vec::new();
            for child in &elem.children {
                if child.name == "when" {
                    let condition =
                        crate::mel::convert_condition(child.attr("expression").unwrap_or(""));
                    if let Expression::Unconverted { original, reason } = &condition {
                        unconverted.push(format!("{original} ({reason})"));
                    }
                    let steps: Vec<RouteStep> = child
                        .children
                        .iter()
                        .flat_map(|c| map_processor(c, flows, rules, decisions, deps))
                        .collect();
                    whens.push((condition, steps));
                } else if child.name == "otherwise" {
                    let steps: Vec<RouteStep> = child
                        .children
                        .iter()
                        .flat_map(|c| map_processor(c, flows, rules, decisions, deps))
                        .collect();
                    otherwise = Some(steps);
                }
            }
            if unconverted.is_empty() {
                record_decision(
                    decisions,
                    elem,
                    DecisionStatus::Done,
                    rule_id,
                    "choice -> choice",
                );
            } else {
                record_decision(
                    decisions,
                    elem,
                    DecisionStatus::ManualReview,
                    rule_id,
                    &format!(
                        "choice -> choice; review: {} of {} conditions are not converted. Each \
                         is emitted as a predicate that fails when it is evaluated, so a \
                         message that gets as far as that condition fails instead of taking a \
                         branch by guess: {}",
                        unconverted.len(),
                        whens.len(),
                        unconverted.join("; ")
                    ),
                );
            }
            vec![RouteStep::Choice { whens, otherwise }]
        }
        ("", "scatter-gather") | ("", "split") => {
            let expr = elem.attr("expression").unwrap_or("#[payload]").to_string();
            let parallel = elem.attr("parallel").map(|v| v == "true").unwrap_or(false);
            let steps: Vec<RouteStep> = elem
                .children
                .iter()
                .flat_map(|c| map_processor(c, flows, rules, decisions, deps))
                .collect();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                "scatter-gather/split -> split",
            );
            vec![RouteStep::Split {
                expression: Expression::Simple(expr),
                parallel,
                aggregation_strategy: None,
                steps,
            }]
        }
        ("", "foreach") => {
            let expr = elem.attr("collection").unwrap_or("#[payload]").to_string();
            let steps: Vec<RouteStep> = elem
                .children
                .iter()
                .flat_map(|c| map_processor(c, flows, rules, decisions, deps))
                .collect();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                "foreach -> split",
            );
            vec![RouteStep::Split {
                expression: Expression::Simple(expr),
                parallel: false,
                aggregation_strategy: None,
                steps,
            }]
        }
        // A flow-ref runs its target as part of the calling flow, and what
        // the target does not handle itself goes to the caller's error
        // handler. Only the route of a sub-flow behaves like that (see
        // `map_sub_flow`), so only a flow-ref to a sub-flow is done.
        ("", "flow-ref") => {
            let name = elem.attr("name").unwrap_or("unknown").to_string();
            match flows.iter().find(|f| f.name == name) {
                Some(target) if target.is_sub_flow => record_decision(
                    decisions,
                    elem,
                    DecisionStatus::Done,
                    rule_id,
                    "flow-ref -> direct endpoint",
                ),
                // The target's route starts at the flow's own source, and it
                // keeps its error handler: it needs it for the flow's on-error
                // scopes, and with it the caller's onException clauses see
                // none of the flow's errors. Camel cannot have both on one
                // route, so this is left to a person.
                Some(_) => record_decision(
                    decisions,
                    elem,
                    DecisionStatus::ManualReview,
                    rule_id,
                    &format!(
                        "flow-ref -> direct:{name}; review: {name} is a <flow>, not a \
                         <sub-flow>. Its route does not consume from direct:{name} - add that \
                         consumer. It also keeps its own error handling: an error the flow does \
                         not handle itself (on-error-propagate, or no on-error scope that \
                         matches) fails the exchange without running the onException clauses of \
                         the calling route, while Mule hands it to the caller's error handler"
                    ),
                ),
                None => record_decision(
                    decisions,
                    elem,
                    DecisionStatus::ManualReview,
                    rule_id,
                    &format!(
                        "flow-ref -> direct:{name}; review: no sub-flow route named {name} is \
                         generated, so nothing consumes from direct:{name} and it is unknown \
                         whether errors of the target reach the onException clauses of the \
                         calling route"
                    ),
                ),
            }
            vec![RouteStep::ToUri(format!("direct:{}", name))]
        }
        ("", "transform") | ("ee", "transform") => {
            // DataWeave transform -> bean reference (LLM handles actual conversion)
            let dw_ref = elem
                .children
                .iter()
                .find(|c| c.name == "message" || c.name == "set-payload")
                .and_then(|c| c.text.as_ref())
                .cloned()
                .unwrap_or_else(|| "// TODO: manual DataWeave conversion".into());
            record_decision(
                decisions,
                elem,
                DecisionStatus::ManualReview,
                rule_id,
                "DataWeave transform requires manual or LLM-assisted conversion",
            );
            vec![RouteStep::RawDsl(format!(
                "// DataWeave: {}",
                dw_ref.lines().next().unwrap_or("")
            ))]
        }
        ("http", "request") => {
            let method = elem.attr("method").unwrap_or("GET");
            let path = elem.attr("path").unwrap_or("/");
            let url: Option<String> = elem.attr("url").map(|s| s.to_string()).or_else(|| {
                elem.resolved_config.as_ref().and_then(|c| {
                    c.children
                        .iter()
                        .find(|ch| ch.name == "request-connection")
                        .map(|conn| {
                            let host = conn.attr("host").unwrap_or("localhost");
                            let port = conn.attr("port").unwrap_or("80");
                            let proto = conn.attr("protocol").unwrap_or("HTTP");
                            format!("{}://{}:{}", proto.to_lowercase(), host, port)
                        })
                })
            });
            let base_url = url.unwrap_or_else(|| "http://localhost".to_string());
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                "HTTP request -> http endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-http".into(),
                version: None,
            });
            vec![RouteStep::ToUri(format!(
                "{}{}?httpMethod={}",
                base_url, path, method
            ))]
        }
        ("db", "select") | ("db", "insert") | ("db", "update") | ("db", "delete") => {
            let sql = elem.text.as_deref().unwrap_or("SELECT 1");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                &format!("db:{} -> sql endpoint", elem.name),
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-sql".into(),
                version: None,
            });
            vec![RouteStep::ToUri(format!(
                "sql:{}?dataSource=default",
                sql.trim()
            ))]
        }
        ("kafka", "publish") | ("kafka", "producer") => {
            let topic = elem.attr("topic").unwrap_or("default-topic");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                "Kafka publish -> kafka endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-kafka".into(),
                version: None,
            });
            vec![RouteStep::ToUri(append_uri_params(
                format!("kafka:{}", topic),
                KAFKA_PRODUCER_PARAMS.iter().copied(),
                elem,
            ))]
        }
        ("jms", "publish") | ("jms", "producer") => {
            let dest = elem.attr("destination").unwrap_or("default-queue");
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                "JMS publish -> jms endpoint",
            );
            deps.push(MavenDependency {
                group_id: "org.apache.camel.quarkus".into(),
                artifact_id: "camel-quarkus-jms".into(),
                version: None,
            });
            vec![RouteStep::ToUri(format!("jms:queue:{}", dest))]
        }
        ("file", "write") | ("sftp", "write") => {
            let dir = elem
                .attr("path")
                .or(elem.attr("directory"))
                .unwrap_or("/tmp/output");
            let proto = if elem.namespace == "sftp" {
                "sftp"
            } else {
                "file"
            };
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                rule_id,
                &format!("{} write -> {} endpoint", elem.namespace, proto),
            );
            if proto == "sftp" {
                deps.push(MavenDependency {
                    group_id: "org.apache.camel.quarkus".into(),
                    artifact_id: "camel-quarkus-ftp".into(),
                    version: None,
                });
            }
            vec![RouteStep::ToUri(format!("{}:{}", proto, dir))]
        }
        _ => {
            record_decision(
                decisions,
                elem,
                DecisionStatus::ManualReview,
                rule_id,
                &format!("No mapping for {} - needs manual review", qname),
            );
            vec![RouteStep::RawDsl(format!(
                "// TODO: manual mapping for {}",
                qname
            ))]
        }
    }
}

/// Exception class of a handler for every error: Mule's `ANY`, or an
/// on-error scope without `type`.
const ANY_EXCEPTION: &str = "java.lang.Exception";

/// Map a flow's `<error-handler>` to Camel exception clauses, one per
/// on-error scope.
///
/// * `on-error-continue` -> `onException(...).handled(true)`: the flow stops
///   at the failed processor, the scope's processors run and their result is
///   the flow's result, as if it had succeeded.
/// * `on-error-propagate` -> `onException(...).handled(false)`: the scope's
///   processors run and the error still reaches the caller.
/// * `type` is a comma-separated list of Mule error types. `ANY`, or no
///   `type`, is every error: `java.lang.Exception`, an exact mapping. Any
///   other known type becomes its nearest exception class (see
///   [`map_mule_error_type`]), which is an approximation and therefore
///   flagged. A type without known class contributes none: it is never
///   widened to `java.lang.Exception`, which would make the handler take
///   every error of the route.
/// * Mule takes the first scope that matches, in document order; Camel keeps
///   one clause per exception class and prefers the closest class. So a
///   class goes to the first scope that names it, and nothing goes to a
///   scope behind one that takes every error.
/// * `when` conditions are not converted; such a scope loses its classes,
///   because without the condition it would handle more than it did.
///
/// A handler left without exception class is emitted commented out. Only an
/// exact mapping is reported as done; everything else is flagged for manual
/// review, in the report and with a TODO in the generated clause.
fn map_error_handler(
    eh: &MuleElement,
    flows: &[crate::ast::mule_ast::MuleFlow],
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> Vec<ExceptionHandler> {
    if let Some(reference) = eh.attr("ref") {
        record_decision(
            decisions,
            eh,
            DecisionStatus::ManualReview,
            None,
            &format!(
                "error-handler ref=\"{reference}\" points to a global error handler, which is \
                 not migrated - add its on-error scopes to this route as onException clauses"
            ),
        );
    }

    let mut handlers = Vec::new();
    // Exception classes that already went to an earlier scope.
    let mut claimed: Vec<String> = Vec::new();
    for child in &eh.children {
        if child.name != "on-error-propagate" && child.name != "on-error-continue" {
            continue;
        }
        let handled = child.name == "on-error-continue";
        let mut classes: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();

        let types: Vec<&str> = child
            .attr("type")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
        if types.is_empty() || types.iter().any(|t| matches!(*t, "ANY" | "MULE:ANY")) {
            classes.push(ANY_EXCEPTION.to_string());
        } else {
            for mule_type in &types {
                let Some(mapping) = map_mule_error_type(mule_type) else {
                    notes.push(format!(
                        "{mule_type} has no known Camel exception class: add the exception \
                         the migrated route throws for it"
                    ));
                    continue;
                };
                notes.push(format!(
                    "{mule_type} is approximated by {}: {}",
                    mapping.class, mapping.caveat
                ));
                if !classes.iter().any(|c| c == mapping.class) {
                    classes.push(mapping.class.to_string());
                }
                if let Some(artifact_id) = mapping.extension {
                    deps.push(MavenDependency {
                        group_id: "org.apache.camel.quarkus".into(),
                        artifact_id: artifact_id.into(),
                        version: None,
                    });
                }
            }
        }

        if claimed.iter().any(|c| c == ANY_EXCEPTION) {
            if !classes.is_empty() {
                notes.push(
                    "an earlier on-error scope of this error handler takes every error, so \
                     Mule never runs this one"
                        .to_string(),
                );
            }
            classes.clear();
        } else {
            classes.retain(|class| {
                let free = !claimed.contains(class);
                if !free {
                    notes.push(format!(
                        "{class} already goes to an earlier on-error scope of this error \
                         handler: Mule takes the first scope that matches, Camel keeps one \
                         clause per exception class"
                    ));
                }
                free
            });
        }

        match child.attr("when").filter(|w| !w.trim().is_empty()) {
            Some(when) => {
                notes.push(format!(
                    "when=\"{}\" is not converted: the clause stays disabled until the \
                     condition is added as .onWhen(...) to {}",
                    when.trim(),
                    on_exception_signature(&classes)
                ));
                classes.clear();
            }
            None => claimed.extend(classes.iter().cloned()),
        }

        let mut steps: Vec<RouteStep> = Vec::new();
        if !notes.is_empty() {
            let mut todo = format!("TODO: review this handler ({})", describe_scope(child));
            for note in &notes {
                todo.push_str("\n- ");
                todo.push_str(note);
            }
            steps.push(RouteStep::RawDsl(todo));
        }
        steps.extend(
            child
                .children
                .iter()
                .flat_map(|c| map_processor(c, flows, rules, decisions, deps)),
        );

        let clause = if classes.is_empty() {
            "onException clause emitted commented out".to_string()
        } else {
            format!("{}.handled({handled})", on_exception_signature(&classes))
        };
        if notes.is_empty() {
            record_decision(
                decisions,
                child,
                DecisionStatus::Done,
                None,
                &format!("{} -> {clause}", child.name),
            );
        } else {
            record_decision(
                decisions,
                child,
                DecisionStatus::ManualReview,
                None,
                &format!("{} -> {clause}; review: {}", child.name, notes.join("; ")),
            );
        }
        handlers.push(ExceptionHandler {
            exception_classes: classes,
            handled,
            steps,
        });
    }
    handlers
}

/// `onException(a.B.class, c.D.class)` for the given exception classes.
fn on_exception_signature(classes: &[String]) -> String {
    let literals: Vec<String> = classes.iter().map(|c| format!("{c}.class")).collect();
    format!("onException({})", literals.join(", "))
}

/// An on-error scope as written in the Mule XML, without its processors:
/// `on-error-continue type="HTTP:CONNECTIVITY"`.
fn describe_scope(scope: &MuleElement) -> String {
    let mut out = scope.name.clone();
    for key in ["type", "when"] {
        if let Some(value) = scope.attr(key) {
            out.push_str(&format!(" {key}=\"{}\"", value.trim()));
        }
    }
    out
}

/// The Java exception class nearest to a Mule error type.
struct ErrorTypeMapping {
    /// Fully qualified class name.
    class: &'static str,
    /// How the class differs from the Mule error type. There always is a
    /// difference: an exception class is not tied to one connector, and
    /// none of these classes covers exactly the failures of its error type.
    caveat: &'static str,
    /// The Camel Quarkus extension that brings the class, unless it is part
    /// of the JDK or of Camel's core API.
    extension: Option<&'static str>,
}

/// Nearest exception class for a namespaced Mule error type, as raised by
/// the Camel component its connector is mapped to. `None` for a type
/// without known counterpart (`ANY` is handled by the caller).
fn map_mule_error_type(mule_type: &str) -> Option<ErrorTypeMapping> {
    let (class, caveat, extension) = match mule_type {
        "HTTP:CONNECTIVITY" => (
            "java.net.ConnectException",
            "it covers a refused connection only; other connection failures (unknown host, \
             TLS, reset) are other java.io.IOException types",
            None,
        ),
        // Connect and response timeouts of Camel's HTTP producer.
        "HTTP:TIMEOUT" => (
            "java.net.SocketTimeoutException",
            "it also matches a socket timeout of any other endpoint of the route",
            None,
        ),
        "HTTP:UNAUTHORIZED" => (
            "org.apache.camel.http.base.HttpOperationFailedException",
            "it matches every failed HTTP status: narrow it with .onWhen(...) on \
             getStatusCode() == 401",
            Some("camel-quarkus-http"),
        ),
        "HTTP:FORBIDDEN" => (
            "org.apache.camel.http.base.HttpOperationFailedException",
            "it matches every failed HTTP status: narrow it with .onWhen(...) on \
             getStatusCode() == 403",
            Some("camel-quarkus-http"),
        ),
        "DB:CONNECTIVITY" | "DB:BAD_SQL_SYNTAX" => (
            "java.sql.SQLException",
            "it matches every SQL failure, not only this error type",
            None,
        ),
        "VALIDATION:INVALID_PAYLOAD" => (
            "org.apache.camel.ValidationException",
            "it is what Camel's validate() and validator endpoints throw: check how the \
             migrated route validates the payload",
            None,
        ),
        _ => return None,
    };
    Some(ErrorTypeMapping {
        class,
        caveat,
        extension,
    })
}

fn find_rule<'a>(rules: &'a [Rule], ns: &str, name: &str) -> Option<&'a Rule> {
    rules
        .iter()
        .find(|r| r.mule_ns == ns && r.mule_name == name)
}

/// Whether `elem` sets attribute `key` to something other than blank.
fn has_attr_value(elem: &MuleElement, key: &str) -> bool {
    elem.attr(key).is_some_and(|v| !v.trim().is_empty())
}

fn expand_uri_template(template: &str, elem: &MuleElement) -> String {
    let mut result = template.to_string();
    for (k, v) in &elem.attributes {
        result = result.replace(&format!("{{{}}}", k), v);
    }
    result
}

/// The endpoint URI a rule produces for `elem`: its URI template plus the
/// rule's `camel_params`.
fn rule_endpoint_uri(rule: &Rule, template: &str, elem: &MuleElement) -> String {
    append_uri_params(
        expand_uri_template(template, elem),
        rule.camel_params
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
        elem,
    )
}

/// Append endpoint options to `uri` as a query string, skipping every option
/// whose value template cannot be resolved against `elem`.
fn append_uri_params<'a>(
    mut uri: String,
    params: impl IntoIterator<Item = (&'a str, &'a str)>,
    elem: &MuleElement,
) -> String {
    for (name, template) in params {
        if let Some(value) = expand_param_template(template, elem) {
            uri.push(if uri.contains('?') { '&' } else { '?' });
            uri.push_str(name);
            uri.push('=');
            uri.push_str(&value);
        }
    }
    uri
}

/// Expand `{attr}` and `{config.attr}` placeholders in an endpoint option
/// template. Returns `None` when a referenced attribute is missing (or the
/// value comes out empty), so optional options simply drop out of the URI.
/// Text in double braces (`{{key}}`) is a Camel property placeholder and is
/// kept as written.
fn expand_param_template(template: &str, elem: &MuleElement) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        if rest.starts_with("{{") {
            let end = rest.find("}}")? + 2;
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        let end = rest.find('}')?;
        let key = &rest[1..end];
        let value = match key.strip_prefix("config.") {
            Some(config_key) => elem.resolved_config.as_ref()?.attr(config_key)?,
            None => elem.attr(key)?,
        };
        out.push_str(&mule_placeholders_to_camel(value));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    (!out.is_empty()).then_some(out)
}

/// Rewrite Mule `${key}` property placeholders (the ones the normalizer could
/// not resolve at build time) as Camel `{{key}}` placeholders, which Camel
/// Quarkus resolves from `application.properties` / MicroProfile Config.
/// Camel does not interpret `${...}` in an endpoint URI.
fn mule_placeholders_to_camel(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        let key = &rest[start + 2..start + len];
        out.push_str(&rest[..start]);
        out.push_str("{{");
        out.push_str(key.strip_prefix("secure::").unwrap_or(key));
        out.push_str("}}");
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

/// The decision for a step that sets a value (`set-payload`,
/// `set-variable`): done if the value is converted, flagged if it is not.
fn record_value_decision(
    decisions: &mut Vec<MappingDecision>,
    elem: &MuleElement,
    rule_id: Option<&str>,
    value: &Expression,
    mapping: &str,
) {
    match value {
        Expression::Unconverted { original, reason } => record_decision(
            decisions,
            elem,
            DecisionStatus::ManualReview,
            rule_id,
            &format!(
                "{mapping}; review: the value {original} is not converted ({reason}). The \
                 route fails at this step until it is replaced"
            ),
        ),
        _ => record_decision(decisions, elem, DecisionStatus::Done, rule_id, mapping),
    }
}

fn record_decision(
    decisions: &mut Vec<MappingDecision>,
    elem: &MuleElement,
    status: DecisionStatus,
    rule_id: Option<&str>,
    rationale: &str,
) {
    decisions.push(MappingDecision {
        mule_element: elem.qualified_name(),
        source_file: String::new(),
        source_line: elem.line,
        flow_name: None,
        status,
        rule_id: rule_id.map(|s| s.to_string()),
        rationale: rationale.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::mule_ast::MuleFlow;

    fn elem(ns: &str, name: &str, attrs: &[(&str, &str)]) -> MuleElement {
        MuleElement {
            namespace: ns.into(),
            name: name.into(),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            children: vec![],
            text: None,
            line: None,
            resolved_config: None,
        }
    }

    fn with_config(mut e: MuleElement, config: MuleElement) -> MuleElement {
        e.resolved_config = Some(Box::new(config));
        e
    }

    fn rules_from_yaml(yaml: &str) -> Vec<Rule> {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("rules.yaml"), yaml).unwrap();
        load_rules(dir.path()).unwrap()
    }

    fn source_uri(e: &MuleElement, rules: &[Rule]) -> String {
        match map_source(e, rules, &mut Vec::new(), &mut Vec::new()) {
            RouteEndpoint::Uri(uri) => uri,
            other => panic!("expected a Uri source, got {:?}", other),
        }
    }

    fn to_uris(e: &MuleElement, rules: &[Rule]) -> Vec<String> {
        map_processor(e, &[], rules, &mut Vec::new(), &mut Vec::new())
            .into_iter()
            .map(|step| match step {
                RouteStep::ToUri(uri) => uri,
                other => panic!("expected a ToUri step, got {:?}", other),
            })
            .collect()
    }

    const PARAM_RULES: &str = r#"
mappings:
  - id: x-listener
    mule_ns: x
    mule_name: listener
    camel_uri: "x:{name}"
    camel_params:
      zeta: "{z}"
      alpha: "{config.a}"
  - id: x-send
    mule_ns: x
    mule_name: send
    camel_uri: "x:{name}?fixed=1"
    camel_params:
      zeta: "{z}"
"#;

    #[test]
    fn rule_camel_params_are_appended_in_declaration_order() {
        let rules = rules_from_yaml(PARAM_RULES);
        let e = with_config(
            elem("x", "listener", &[("name", "n"), ("z", "1")]),
            elem("x", "config", &[("a", "2")]),
        );
        assert_eq!(source_uri(&e, &rules), "x:n?zeta=1&alpha=2");
    }

    #[test]
    fn rule_camel_param_is_omitted_when_its_attribute_is_missing() {
        let rules = rules_from_yaml(PARAM_RULES);

        let bare = elem("x", "listener", &[("name", "n")]);
        assert_eq!(source_uri(&bare, &rules), "x:n");

        let only_config = with_config(bare, elem("x", "config", &[("a", "2")]));
        assert_eq!(source_uri(&only_config, &rules), "x:n?alpha=2");
    }

    #[test]
    fn rule_camel_params_extend_an_existing_query_string() {
        let rules = rules_from_yaml(PARAM_RULES);
        let e = elem("x", "send", &[("name", "n"), ("z", "1")]);
        assert_eq!(to_uris(&e, &rules), ["x:n?fixed=1&zeta=1"]);
    }

    #[test]
    fn rule_camel_param_turns_mule_placeholders_into_camel_placeholders() {
        let rules = rules_from_yaml(PARAM_RULES);
        let e = elem("x", "listener", &[("name", "n"), ("z", "${my.prop}")]);
        assert_eq!(source_uri(&e, &rules), "x:n?zeta={{my.prop}}");
    }

    #[test]
    fn rule_camel_param_keeps_a_literal_camel_placeholder() {
        let rules = rules_from_yaml(
            r#"
mappings:
  - id: y-send
    mule_ns: y
    mule_name: send
    camel_uri: "y:{name}"
    camel_params:
      brokers: "{{y.brokers}}"
"#,
        );
        let e = elem("y", "send", &[("name", "n")]);
        assert_eq!(to_uris(&e, &rules), ["y:n?brokers={{y.brokers}}"]);
    }

    #[test]
    fn mule_placeholders_become_camel_placeholders() {
        assert_eq!(
            mule_placeholders_to_camel("localhost:9092"),
            "localhost:9092"
        );
        assert_eq!(
            mule_placeholders_to_camel("${kafka.brokers}"),
            "{{kafka.brokers}}"
        );
        assert_eq!(
            mule_placeholders_to_camel("${host}:${port}/x"),
            "{{host}}:{{port}}/x"
        );
        // Mule's secure:: prefix has no Camel counterpart, and Camel would read
        // "{{secure::key}}" as key "secure" with default ":key".
        assert_eq!(mule_placeholders_to_camel("${secure::db.pw}"), "{{db.pw}}");
        // Not a placeholder: left alone.
        assert_eq!(
            mule_placeholders_to_camel("${unterminated"),
            "${unterminated"
        );
    }

    // ── direct: endpoints ───────────────────────────────────────────────

    fn flow(name: &str, is_sub_flow: bool, processors: Vec<MuleElement>) -> MuleFlow {
        MuleFlow {
            name: name.into(),
            is_sub_flow,
            source_file: "flows.xml".into(),
            processors,
            error_handlers: vec![],
        }
    }

    async fn dependencies_of(flows: Vec<MuleFlow>) -> Vec<String> {
        let project = MuleProject {
            name: "orders".into(),
            flows,
            configs: vec![],
            properties: Default::default(),
        };
        let (camel, _) = map_project(&project, &[], None).await.unwrap();
        camel
            .maven_dependencies
            .into_iter()
            .map(|d| d.artifact_id)
            .collect()
    }

    fn http_listener() -> MuleElement {
        elem(
            "http",
            "listener",
            &[("path", "/orders"), ("allowedMethods", "GET")],
        )
    }

    #[tokio::test]
    async fn sub_flow_route_brings_the_direct_component() {
        // A sub-flow becomes from("direct:<name>"). Without the extension
        // the application fails to start: "No endpoint could be found for:
        // direct://<name>".
        let deps = dependencies_of(vec![flow(
            "enrichPayload",
            true,
            vec![elem("", "logger", &[("message", "enriched")])],
        )])
        .await;

        assert!(deps.iter().any(|d| d == "camel-quarkus-direct"), "{deps:?}");
    }

    #[tokio::test]
    async fn flow_ref_brings_the_direct_component() {
        // Also when it is nested in a scope or in an error handler.
        let mut choice = elem("", "choice", &[]);
        let mut when = elem("", "when", &[("expression", "#[true]")]);
        when.children = vec![elem("", "flow-ref", &[("name", "audit")])];
        choice.children = vec![when];
        let nested = flow("ordersFlow", false, vec![http_listener(), choice]);

        let mut handled = flow("pricesFlow", false, vec![http_listener()]);
        let mut scope = elem("", "on-error-propagate", &[]);
        scope.children = vec![elem("", "flow-ref", &[("name", "audit")])];
        handled.error_handlers = vec![error_handler(vec![scope])];

        for flows in [vec![nested], vec![handled]] {
            let deps = dependencies_of(flows).await;
            assert!(deps.iter().any(|d| d == "camel-quarkus-direct"), "{deps:?}");
        }
    }

    #[tokio::test]
    async fn project_without_direct_endpoint_does_not_get_the_direct_component() {
        let deps = dependencies_of(vec![flow(
            "ordersFlow",
            false,
            vec![http_listener(), elem("", "set-payload", &[("value", "ok")])],
        )])
        .await;

        assert!(
            !deps.iter().any(|d| d == "camel-quarkus-direct"),
            "{deps:?}"
        );
    }

    // ── flow-ref and the error handler of the caller ────────────────────

    async fn map_flows(flows: Vec<MuleFlow>) -> (CamelProject, Vec<MappingDecision>) {
        let project = MuleProject {
            name: "orders".into(),
            flows,
            configs: vec![],
            properties: Default::default(),
        };
        map_project(&project, &[], None).await.unwrap()
    }

    /// A flow with an HTTP listener that calls `target` through a flow-ref
    /// and handles every error with an on-error-continue.
    fn caller_of(target: &str) -> MuleFlow {
        let mut caller = flow(
            "callerFlow",
            false,
            vec![http_listener(), elem("", "flow-ref", &[("name", target)])],
        );
        caller.error_handlers = vec![error_handler(vec![on_error("on-error-continue", &[])])];
        caller
    }

    fn decisions_for<'a>(
        decisions: &'a [MappingDecision],
        element: &str,
    ) -> Vec<&'a MappingDecision> {
        decisions
            .iter()
            .filter(|d| d.mule_element == element)
            .collect()
    }

    #[tokio::test]
    async fn flow_ref_to_a_sub_flow_is_done_and_its_route_has_no_handlers_of_its_own() {
        // The sub-flow's route is the one kind of route the emitter writes
        // without error handler (`RouteEndpoint::Direct`, no exception
        // handlers), so what fails in it reaches the caller's onException
        // clauses like it reaches the caller's error handler in Mule.
        let (camel, decisions) = map_flows(vec![
            caller_of("shared"),
            flow(
                "otherFlow",
                false,
                vec![http_listener(), elem("", "flow-ref", &[("name", "shared")])],
            ),
            flow(
                "shared",
                true,
                vec![elem("", "logger", &[("message", "in shared")])],
            ),
        ])
        .await;

        let shared = camel.routes.iter().find(|r| r.id == "shared").unwrap();
        assert!(
            matches!(&shared.source, RouteEndpoint::Direct(name) if name == "shared"),
            "{:?}",
            shared.source
        );
        assert!(shared.on_exceptions.is_empty());
        // Only a sub-flow becomes such a route.
        for route in camel.routes.iter().filter(|r| r.id != "shared") {
            assert!(
                matches!(route.source, RouteEndpoint::Uri(_)),
                "{}: {:?}",
                route.id,
                route.source
            );
        }

        let flow_refs = decisions_for(&decisions, "flow-ref");
        assert_eq!(flow_refs.len(), 2);
        for decision in flow_refs {
            assert_eq!(
                decision.status,
                DecisionStatus::Done,
                "{}",
                decision.rationale
            );
        }
        let scopes = decisions_for(&decisions, "on-error-continue");
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].status, DecisionStatus::Done);
    }

    #[tokio::test]
    async fn flow_ref_to_a_flow_is_flagged_for_review() {
        // A flow called through flow-ref handles its own errors first, and
        // Mule hands what it propagates on to the caller's error handler.
        // The flow's route keeps its error handler (it needs it for its own
        // on-error scopes), and what that propagates does not reach the
        // caller's onException clauses. Nor does the route consume from
        // direct:<name>. So the flow-ref is not done.
        let mut with_handler = flow(
            "auditFlow",
            false,
            vec![elem("", "logger", &[("message", "audit")])],
        );
        with_handler.error_handlers =
            vec![error_handler(vec![on_error("on-error-propagate", &[])])];
        let with_source = flow(
            "auditFlow",
            false,
            vec![http_listener(), elem("", "logger", &[("message", "audit")])],
        );

        for target in [with_handler, with_source] {
            let (camel, decisions) = map_flows(vec![caller_of("auditFlow"), target]).await;

            let flow_refs = decisions_for(&decisions, "flow-ref");
            assert_eq!(flow_refs.len(), 1);
            assert_eq!(flow_refs[0].status, DecisionStatus::ManualReview);
            let rationale = &flow_refs[0].rationale;
            assert!(
                rationale.contains("auditFlow")
                    && rationale.contains("direct:auditFlow")
                    && rationale.contains("<flow>")
                    && rationale.contains("onException"),
                "{rationale}"
            );
            // The step is still emitted, and the called flow's route is not
            // turned into a route without error handler.
            let caller = camel.routes.iter().find(|r| r.id == "callerFlow").unwrap();
            assert!(
                matches!(&caller.steps[..], [RouteStep::ToUri(uri)] if uri == "direct:auditFlow"),
                "{:?}",
                caller.steps
            );
            let audit = camel.routes.iter().find(|r| r.id == "auditFlow").unwrap();
            assert!(
                matches!(audit.source, RouteEndpoint::Uri(_)),
                "{:?}",
                audit.source
            );
        }
    }

    #[tokio::test]
    async fn flow_ref_without_a_sub_flow_route_is_flagged_for_review() {
        // Nothing consumes from direct:<name>, and nothing can be said
        // about where the errors of the target go.
        let (_, decisions) = map_flows(vec![caller_of("elsewhere")]).await;

        let flow_refs = decisions_for(&decisions, "flow-ref");
        assert_eq!(flow_refs.len(), 1);
        assert_eq!(flow_refs[0].status, DecisionStatus::ManualReview);
        assert!(
            flow_refs[0].rationale.contains("direct:elsewhere"),
            "{}",
            flow_refs[0].rationale
        );
    }

    #[tokio::test]
    async fn flow_ref_in_an_error_handler_or_a_scope_is_judged_the_same_way() {
        let mut scope = on_error("on-error-continue", &[]);
        scope.children = vec![
            elem("", "flow-ref", &[("name", "shared")]),
            elem("", "flow-ref", &[("name", "auditFlow")]),
        ];
        let mut handled = flow("pricesFlow", false, vec![http_listener()]);
        handled.error_handlers = vec![error_handler(vec![scope])];

        let mut when = elem("", "when", &[("expression", "#[true]")]);
        when.children = vec![
            elem("", "flow-ref", &[("name", "shared")]),
            elem("", "flow-ref", &[("name", "auditFlow")]),
        ];
        let mut choice = elem("", "choice", &[]);
        choice.children = vec![when];
        let nested = flow("ordersFlow", false, vec![http_listener(), choice]);

        for caller in [handled, nested] {
            let (_, decisions) = map_flows(vec![
                caller,
                flow(
                    "shared",
                    true,
                    vec![elem("", "logger", &[("message", "in shared")])],
                ),
                flow(
                    "auditFlow",
                    false,
                    vec![http_listener(), elem("", "logger", &[("message", "audit")])],
                ),
            ])
            .await;

            let statuses: Vec<DecisionStatus> = decisions_for(&decisions, "flow-ref")
                .iter()
                .map(|d| d.status)
                .collect();
            assert_eq!(
                statuses,
                [DecisionStatus::Done, DecisionStatus::ManualReview]
            );
        }
    }

    // ── Error handlers ──────────────────────────────────────────────────

    /// `<error-handler>` holding the given on-error scopes.
    fn error_handler(scopes: Vec<MuleElement>) -> MuleElement {
        let mut eh = elem("", "error-handler", &[]);
        eh.children = scopes;
        eh
    }

    /// An on-error scope whose only processor sets the payload to `"fallback"`.
    fn on_error(name: &str, attrs: &[(&str, &str)]) -> MuleElement {
        let mut scope = elem("", name, attrs);
        scope.children = vec![elem("", "set-payload", &[("value", "fallback")])];
        scope
    }

    /// Map an error handler without rules: the handlers and the decisions
    /// recorded for the on-error scopes themselves (not for their processors).
    fn map_scopes(scopes: Vec<MuleElement>) -> (Vec<ExceptionHandler>, Vec<MappingDecision>) {
        let mut decisions = Vec::new();
        let handlers = map_error_handler(
            &error_handler(scopes),
            &[],
            &[],
            &mut decisions,
            &mut Vec::new(),
        );
        decisions.retain(|d| d.mule_element.starts_with("on-error-"));
        (handlers, decisions)
    }

    /// The review notes the mapper left in the handler for the generated code.
    fn review_notes(handler: &ExceptionHandler) -> String {
        handler
            .steps
            .iter()
            .filter_map(|s| match s {
                RouteStep::RawDsl(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn on_error_continue_is_handled_and_on_error_propagate_is_not() {
        let (handlers, decisions) = map_scopes(vec![
            on_error("on-error-continue", &[("type", "HTTP:TIMEOUT")]),
            on_error("on-error-propagate", &[]),
        ]);

        assert_eq!(handlers.len(), 2);
        assert!(handlers[0].handled, "on-error-continue -> handled(true)");
        assert!(!handlers[1].handled, "on-error-propagate -> handled(false)");
        assert_eq!(decisions.len(), 2);
    }

    #[test]
    fn handler_for_any_error_catches_every_exception_and_is_done() {
        // No type, ANY, or a list that contains ANY: every Mule error.
        for attrs in [
            &[][..],
            &[("type", "ANY")],
            &[("type", "MULE:ANY")],
            &[("type", " ")],
            &[("type", "HTTP:TIMEOUT, ANY")],
        ] {
            let (handlers, decisions) = map_scopes(vec![on_error("on-error-propagate", attrs)]);

            assert_eq!(
                handlers[0].exception_classes,
                ["java.lang.Exception"],
                "{attrs:?}"
            );
            assert_eq!(decisions[0].status, DecisionStatus::Done, "{attrs:?}");
            assert_eq!(review_notes(&handlers[0]), "", "{attrs:?}");
            assert!(
                matches!(handlers[0].steps[..], [RouteStep::SetBody { .. }]),
                "{attrs:?}: {:?}",
                handlers[0].steps
            );
        }
    }

    #[test]
    fn http_timeout_is_a_socket_timeout_not_a_refused_connection() {
        // A response timeout of Camel's HTTP producer is a
        // java.net.SocketTimeoutException; it is not a ConnectException.
        let (handlers, _) = map_scopes(vec![on_error(
            "on-error-continue",
            &[("type", "HTTP:TIMEOUT")],
        )]);

        assert_eq!(
            handlers[0].exception_classes,
            ["java.net.SocketTimeoutException"]
        );
    }

    #[test]
    fn error_type_list_becomes_one_exception_class_per_type() {
        let (handlers, decisions) = map_scopes(vec![on_error(
            "on-error-continue",
            &[("type", "HTTP:CONNECTIVITY, HTTP:TIMEOUT ,HTTP:UNAUTHORIZED")],
        )]);

        assert_eq!(handlers.len(), 1);
        assert_eq!(
            handlers[0].exception_classes,
            [
                "java.net.ConnectException",
                "java.net.SocketTimeoutException",
                "org.apache.camel.http.base.HttpOperationFailedException",
            ]
        );
        assert!(handlers[0].handled);
        assert_eq!(decisions.len(), 1);
    }

    #[test]
    fn error_types_with_the_same_exception_class_name_it_once() {
        let (handlers, _) = map_scopes(vec![on_error(
            "on-error-propagate",
            &[("type", "DB:CONNECTIVITY, DB:BAD_SQL_SYNTAX")],
        )]);

        assert_eq!(handlers[0].exception_classes, ["java.sql.SQLException"]);
    }

    #[test]
    fn connector_error_type_is_an_approximation_and_flagged_for_review() {
        // A Java exception class is never exactly a Mule error type: it is
        // not scoped to one connector and covers other failures as well, or
        // fewer. The clause is emitted, and flagged in report and code.
        for (mule_type, class) in [
            ("HTTP:CONNECTIVITY", "java.net.ConnectException"),
            ("HTTP:TIMEOUT", "java.net.SocketTimeoutException"),
            (
                "HTTP:UNAUTHORIZED",
                "org.apache.camel.http.base.HttpOperationFailedException",
            ),
            (
                "HTTP:FORBIDDEN",
                "org.apache.camel.http.base.HttpOperationFailedException",
            ),
            ("DB:CONNECTIVITY", "java.sql.SQLException"),
            ("DB:BAD_SQL_SYNTAX", "java.sql.SQLException"),
            (
                "VALIDATION:INVALID_PAYLOAD",
                "org.apache.camel.ValidationException",
            ),
        ] {
            let (handlers, decisions) =
                map_scopes(vec![on_error("on-error-continue", &[("type", mule_type)])]);

            assert_eq!(handlers[0].exception_classes, [class], "{mule_type}");
            assert_eq!(
                decisions[0].status,
                DecisionStatus::ManualReview,
                "{mule_type}"
            );
            assert!(
                decisions[0].rationale.contains(mule_type)
                    && decisions[0].rationale.contains(class),
                "{mule_type}: {}",
                decisions[0].rationale
            );
            let notes = review_notes(&handlers[0]);
            assert!(
                notes.contains("TODO") && notes.contains(mule_type),
                "{mule_type}: {notes:?}"
            );
            // The note comes first, the handler's processors follow.
            assert!(
                matches!(
                    handlers[0].steps[..],
                    [RouteStep::RawDsl(_), RouteStep::SetBody { .. }]
                ),
                "{mule_type}: {:?}",
                handlers[0].steps
            );
        }
    }

    #[test]
    fn http_status_error_type_brings_the_dependency_of_its_exception_class() {
        // HttpOperationFailedException is not on the classpath of a project
        // without camel-quarkus-http; the generated route would not compile.
        let mut deps = Vec::new();
        map_error_handler(
            &error_handler(vec![on_error(
                "on-error-continue",
                &[("type", "HTTP:FORBIDDEN")],
            )]),
            &[],
            &[],
            &mut Vec::new(),
            &mut deps,
        );

        assert!(
            deps.iter().any(|d| d.artifact_id == "camel-quarkus-http"),
            "{deps:?}"
        );
    }

    #[test]
    fn unknown_error_type_is_never_widened_to_every_exception() {
        // Falling back to java.lang.Exception would turn a handler for one
        // error type into one that swallows every failure of the route.
        let (handlers, decisions) = map_scopes(vec![on_error(
            "on-error-continue",
            &[("type", "FILE:FILE_NOT_FOUND")],
        )]);

        assert_eq!(handlers.len(), 1);
        assert!(
            handlers[0].exception_classes.is_empty(),
            "{:?}",
            handlers[0].exception_classes
        );
        assert!(handlers[0].handled);
        assert_eq!(decisions[0].status, DecisionStatus::ManualReview);
        assert!(
            decisions[0].rationale.contains("FILE:FILE_NOT_FOUND"),
            "{}",
            decisions[0].rationale
        );
        let notes = review_notes(&handlers[0]);
        assert!(
            notes.contains("TODO") && notes.contains("FILE:FILE_NOT_FOUND"),
            "{notes:?}"
        );
    }

    #[test]
    fn unknown_error_type_next_to_known_ones_keeps_the_known_classes() {
        let (handlers, decisions) = map_scopes(vec![on_error(
            "on-error-propagate",
            &[("type", "HTTP:TIMEOUT, APP:ORDER_REJECTED")],
        )]);

        assert_eq!(
            handlers[0].exception_classes,
            ["java.net.SocketTimeoutException"]
        );
        assert_eq!(decisions[0].status, DecisionStatus::ManualReview);
        assert!(
            decisions[0].rationale.contains("APP:ORDER_REJECTED"),
            "{}",
            decisions[0].rationale
        );
        assert!(review_notes(&handlers[0]).contains("APP:ORDER_REJECTED"));
    }

    #[test]
    fn when_condition_disables_the_handler_instead_of_being_dropped() {
        // Without its condition the clause would fire for every error.
        let condition = "#[error.description contains 'quota']";
        let (handlers, decisions) = map_scopes(vec![
            on_error("on-error-continue", &[("when", condition)]),
            on_error("on-error-propagate", &[]),
        ]);

        assert!(
            handlers[0].exception_classes.is_empty(),
            "{:?}",
            handlers[0].exception_classes
        );
        assert_eq!(decisions[0].status, DecisionStatus::ManualReview);
        assert!(
            decisions[0].rationale.contains("when"),
            "{}",
            decisions[0].rationale
        );
        assert!(review_notes(&handlers[0]).contains(condition));
        // A conditional handler does not take anything away from the
        // handlers after it.
        assert_eq!(handlers[1].exception_classes, ["java.lang.Exception"]);
        assert_eq!(decisions[1].status, DecisionStatus::Done);
    }

    #[test]
    fn first_matching_handler_wins_like_in_mule() {
        // Mule takes the first on-error scope that matches, in document
        // order. Camel keeps a single clause per exception class, so the
        // class goes to the first scope only.
        let (handlers, decisions) = map_scopes(vec![
            on_error("on-error-continue", &[("type", "HTTP:UNAUTHORIZED")]),
            on_error(
                "on-error-propagate",
                &[("type", "HTTP:FORBIDDEN, HTTP:TIMEOUT")],
            ),
            on_error("on-error-propagate", &[("type", "HTTP:FORBIDDEN")]),
        ]);

        assert_eq!(
            handlers[0].exception_classes,
            ["org.apache.camel.http.base.HttpOperationFailedException"]
        );
        assert_eq!(
            handlers[1].exception_classes,
            ["java.net.SocketTimeoutException"]
        );
        assert!(
            handlers[2].exception_classes.is_empty(),
            "{:?}",
            handlers[2].exception_classes
        );
        for (i, decision) in decisions.iter().enumerate().skip(1) {
            assert_eq!(decision.status, DecisionStatus::ManualReview, "scope {i}");
            assert!(
                decision
                    .rationale
                    .contains("org.apache.camel.http.base.HttpOperationFailedException"),
                "scope {i}: {}",
                decision.rationale
            );
        }
    }

    #[test]
    fn handler_after_a_catch_all_is_unreachable_and_not_emitted_as_active() {
        // In Mule the ANY scope takes every error; in Camel the more
        // specific clause would win. Keep Mule's outcome.
        let (handlers, decisions) = map_scopes(vec![
            on_error("on-error-propagate", &[("type", "ANY")]),
            on_error("on-error-continue", &[("type", "HTTP:TIMEOUT")]),
        ]);

        assert_eq!(handlers[0].exception_classes, ["java.lang.Exception"]);
        assert_eq!(decisions[0].status, DecisionStatus::Done);
        assert!(
            handlers[1].exception_classes.is_empty(),
            "{:?}",
            handlers[1].exception_classes
        );
        assert_eq!(decisions[1].status, DecisionStatus::ManualReview);
    }

    #[test]
    fn referenced_error_handler_is_flagged_not_dropped() {
        // <error-handler ref="..."/> points at a global handler that is not
        // migrated; the flow must not look as if it had no error handling.
        let reference = elem("", "error-handler", &[("ref", "globalErrorHandler")]);
        let mut decisions = Vec::new();
        let handlers = map_error_handler(&reference, &[], &[], &mut decisions, &mut Vec::new());

        assert!(handlers.is_empty());
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].mule_element, "error-handler");
        assert_eq!(decisions[0].status, DecisionStatus::ManualReview);
        assert!(
            decisions[0].rationale.contains("globalErrorHandler"),
            "{}",
            decisions[0].rationale
        );
    }

    // ── Kafka ───────────────────────────────────────────────────────────

    /// The rules shipped in `mappings/`, and no rules at all (built-in
    /// fallback): Kafka endpoints must come out the same either way.
    fn shipped_and_builtin_rules() -> [Vec<Rule>; 2] {
        let shipped = load_rules(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../mappings"))
            .expect("load mappings/");
        assert!(shipped.iter().any(|r| r.mule_ns == "kafka"));
        [shipped, Vec::new()]
    }

    fn kafka_config(element: &str, name: &str, bootstrap_servers: &str) -> MuleElement {
        elem(
            "kafka",
            element,
            &[("name", name), ("bootstrapServers", bootstrap_servers)],
        )
    }

    #[test]
    fn kafka_message_listener_is_a_consumer_with_brokers_and_group_id() {
        let listener = with_config(
            elem(
                "kafka",
                "message-listener",
                &[
                    ("config-ref", "Kafka_Consumer_config"),
                    ("topic", "orders.raw"),
                    ("groupId", "order-ingest-group"),
                ],
            ),
            kafka_config(
                "consumer-config",
                "Kafka_Consumer_config",
                "${kafka.brokers}",
            ),
        );

        for rules in shipped_and_builtin_rules() {
            let mut decisions = Vec::new();
            let mut deps = Vec::new();
            let source = map_source(&listener, &rules, &mut decisions, &mut deps);

            match source {
                RouteEndpoint::Uri(uri) => assert_eq!(
                    uri,
                    "kafka:orders.raw?brokers={{kafka.brokers}}&groupId=order-ingest-group"
                ),
                other => panic!("expected a Uri source, got {:?}", other),
            }
            assert_eq!(decisions.len(), 1);
            assert_eq!(decisions[0].status, DecisionStatus::Done);
            assert!(deps.iter().any(|d| d.artifact_id == "camel-quarkus-kafka"));
        }
    }

    /// `<kafka:consumer-config>` the way Kafka connector 4.x writes it: topic
    /// patterns, group ID and bootstrap servers all on the connection child.
    fn kafka_4x_consumer_config(name: &str) -> MuleElement {
        let mut bootstrap_servers = elem("kafka", "bootstrap-servers", &[]);
        bootstrap_servers.children = vec![elem(
            "kafka",
            "bootstrap-server",
            &[("value", "${kafka.brokers}")],
        )];
        let mut topic_patterns = elem("kafka", "topic-patterns", &[]);
        topic_patterns.children = vec![elem("kafka", "topic-pattern", &[("value", "orders.raw")])];
        let mut connection = elem(
            "kafka",
            "consumer-plaintext-connection",
            &[("groupId", "order-ingest-group")],
        );
        connection.children = vec![bootstrap_servers, topic_patterns];
        let mut config = elem("kafka", "consumer-config", &[("name", name)]);
        config.children = vec![connection];
        config
    }

    /// A message-listener the mapper cannot name a topic for: no `kafka:`
    /// source and no `Done`, whether or not the shipped rules are loaded.
    fn assert_message_listener_needs_manual_review(listener: &MuleElement) {
        for rules in shipped_and_builtin_rules() {
            let mut decisions = Vec::new();
            let mut deps = Vec::new();
            let source = map_source(listener, &rules, &mut decisions, &mut deps);

            match source {
                RouteEndpoint::Uri(uri) => assert_eq!(uri, "direct:kafka-message-listener"),
                other => panic!("expected a Uri source, got {:?}", other),
            }
            assert_eq!(decisions.len(), 1);
            assert_eq!(decisions[0].mule_element, "kafka:message-listener");
            assert_eq!(decisions[0].status, DecisionStatus::ManualReview);
            assert_eq!(decisions[0].rule_id, None);
            assert!(
                decisions[0].rationale.contains("no topic attribute"),
                "rationale should say what is missing: {}",
                decisions[0].rationale
            );
        }
    }

    #[test]
    fn kafka_4x_message_listener_without_topic_needs_manual_review() {
        // The real connector shape: the listener only has config-ref; the
        // subscription lives on the consumer-config's connection.
        let listener = with_config(
            elem(
                "kafka",
                "message-listener",
                &[("config-ref", "Kafka_Consumer_config")],
            ),
            kafka_4x_consumer_config("Kafka_Consumer_config"),
        );
        assert_message_listener_needs_manual_review(&listener);
    }

    #[test]
    fn kafka_message_listener_with_blank_topic_or_no_config_needs_manual_review() {
        let blank_topic = with_config(
            elem(
                "kafka",
                "message-listener",
                &[("config-ref", "c"), ("topic", " "), ("groupId", "g")],
            ),
            kafka_config("consumer-config", "c", "broker-1:9092"),
        );
        assert_message_listener_needs_manual_review(&blank_topic);

        let bare = elem("kafka", "message-listener", &[]);
        assert_message_listener_needs_manual_review(&bare);
    }

    #[test]
    fn kafka_publish_takes_brokers_from_its_config() {
        let publish = with_config(
            elem(
                "kafka",
                "publish",
                &[
                    ("config-ref", "Kafka_Producer_config"),
                    ("topic", "orders.validated"),
                ],
            ),
            kafka_config(
                "producer-config",
                "Kafka_Producer_config",
                "${kafka.brokers}",
            ),
        );

        for rules in shipped_and_builtin_rules() {
            assert_eq!(
                to_uris(&publish, &rules),
                ["kafka:orders.validated?brokers={{kafka.brokers}}"]
            );
        }
    }

    #[test]
    fn kafka_brokers_resolved_by_the_normalizer_are_emitted_literally() {
        let consumer = with_config(
            elem(
                "kafka",
                "consumer",
                &[("topic", "orders"), ("groupId", "g")],
            ),
            kafka_config("consumer-config", "c", "broker-1:9092,broker-2:9092"),
        );

        for rules in shipped_and_builtin_rules() {
            assert_eq!(
                source_uri(&consumer, &rules),
                "kafka:orders?brokers=broker-1:9092,broker-2:9092&groupId=g"
            );
        }
    }

    #[test]
    fn kafka_endpoints_without_config_or_group_stay_bare() {
        // Brokers and group then come from camel.component.kafka.* in
        // application.properties.
        let consumer = elem("kafka", "consumer", &[("topic", "orders-topic")]);
        let publish = elem("kafka", "publish", &[("topic", "orders-out")]);

        for rules in shipped_and_builtin_rules() {
            assert_eq!(source_uri(&consumer, &rules), "kafka:orders-topic");
            assert_eq!(to_uris(&publish, &rules), ["kafka:orders-out"]);
        }
    }

    // ── Expressions: choice, set-variable, set-payload ──────────────────

    fn steps_and_decisions(
        e: &MuleElement,
        rules: &[Rule],
    ) -> (Vec<RouteStep>, Vec<MappingDecision>) {
        let mut decisions = Vec::new();
        let steps = map_processor(e, &[], rules, &mut decisions, &mut Vec::new());
        (steps, decisions)
    }

    /// `<when expression="...">` with one set-payload in it.
    fn when(expression: &str, payload: &str) -> MuleElement {
        let mut when = elem("", "when", &[("expression", expression)]);
        when.children = vec![elem("", "set-payload", &[("value", payload)])];
        when
    }

    fn otherwise(payload: &str) -> MuleElement {
        let mut otherwise = elem("", "otherwise", &[]);
        otherwise.children = vec![elem("", "set-payload", &[("value", payload)])];
        otherwise
    }

    fn choice(children: Vec<MuleElement>) -> MuleElement {
        let mut choice = elem("", "choice", &[]);
        choice.children = children;
        choice
    }

    type Branches = (Vec<(Expression, Vec<RouteStep>)>, Option<Vec<RouteStep>>);

    fn only_choice(steps: Vec<RouteStep>) -> Branches {
        match <[RouteStep; 1]>::try_from(steps) {
            Ok([RouteStep::Choice { whens, otherwise }]) => (whens, otherwise),
            other => panic!("expected one Choice step, got {other:?}"),
        }
    }

    /// The body constants a branch sets, to tell the branches apart.
    fn bodies(steps: &[RouteStep]) -> Vec<&str> {
        steps
            .iter()
            .map(|step| match step {
                RouteStep::SetBody {
                    expression: Expression::Constant(text),
                } => text.as_str(),
                other => panic!("expected setBody(constant), got {other:?}"),
            })
            .collect()
    }

    fn simple(s: &str) -> Expression {
        Expression::Simple(s.into())
    }

    fn is_unconverted(expression: &Expression, mule: &str) -> bool {
        matches!(expression, Expression::Unconverted { original, .. } if original == mule)
    }

    #[test]
    fn choice_with_convertible_conditions_is_done() {
        let router = choice(vec![
            when("#[vars.type == 'A']", "a"),
            when("#[vars.type == 'B']", "b"),
            otherwise("default"),
        ]);
        for rules in shipped_and_builtin_rules() {
            let (steps, decisions) = steps_and_decisions(&router, &rules);
            let (whens, otherwise) = only_choice(steps);

            assert_eq!(whens.len(), 2);
            assert_eq!(whens[0].0, simple("${exchangeProperty.type} == 'A'"));
            assert_eq!(bodies(&whens[0].1), ["a"]);
            assert_eq!(whens[1].0, simple("${exchangeProperty.type} == 'B'"));
            assert_eq!(bodies(&whens[1].1), ["b"]);
            assert_eq!(bodies(&otherwise.expect("otherwise branch")), ["default"]);

            let choice = decisions_for(&decisions, "choice");
            assert_eq!(choice.len(), 1, "{choice:?}");
            assert_eq!(choice[0].status, DecisionStatus::Done);
        }
    }

    /// A condition that is not converted keeps its branch (and every other
    /// branch), is never handed to Camel as an expression, and flags the
    /// choice. There is no row that still calls the choice done.
    #[test]
    fn choice_with_an_unconvertible_condition_is_flagged_and_keeps_every_branch() {
        let router = choice(vec![
            when("#[vars.type == 'A']", "a"),
            when("#[payload.type == 'B']", "b"),
            otherwise("default"),
        ]);
        for rules in shipped_and_builtin_rules() {
            let (steps, decisions) = steps_and_decisions(&router, &rules);
            let (whens, otherwise) = only_choice(steps);

            assert_eq!(whens.len(), 2);
            assert_eq!(whens[0].0, simple("${exchangeProperty.type} == 'A'"));
            assert_eq!(bodies(&whens[0].1), ["a"]);
            assert!(
                is_unconverted(&whens[1].0, "#[payload.type == 'B']"),
                "{:?}",
                whens[1].0
            );
            assert_eq!(bodies(&whens[1].1), ["b"]);
            assert_eq!(bodies(&otherwise.expect("otherwise branch")), ["default"]);

            let choice = decisions_for(&decisions, "choice");
            assert_eq!(choice.len(), 1, "{choice:?}");
            assert_eq!(choice[0].status, DecisionStatus::ManualReview);
            let rationale = &choice[0].rationale;
            assert!(rationale.contains("1 of 2 conditions"), "{rationale}");
            assert!(rationale.contains("#[payload.type == 'B']"), "{rationale}");
            assert!(!rationale.contains("#[vars.type == 'A']"), "{rationale}");
        }
    }

    #[test]
    fn choice_without_otherwise_has_no_otherwise_branch() {
        let router = choice(vec![when("#[vars.type == 'A']", "a")]);
        let (steps, _) = steps_and_decisions(&router, &[]);
        let (whens, otherwise) = only_choice(steps);
        assert_eq!(whens.len(), 1);
        assert!(otherwise.is_none());
    }

    /// A `when` without expression used to become a branch that always
    /// matches. What it should match is unknown: not converted.
    #[test]
    fn when_without_expression_is_not_made_to_match_always() {
        let mut when = elem("", "when", &[]);
        when.children = vec![elem("", "set-payload", &[("value", "a")])];
        let (steps, decisions) = steps_and_decisions(&choice(vec![when]), &[]);
        let (whens, _) = only_choice(steps);
        assert!(is_unconverted(&whens[0].0, ""), "{:?}", whens[0].0);
        assert_eq!(
            decisions_for(&decisions, "choice")[0].status,
            DecisionStatus::ManualReview
        );
    }

    /// The decision of the one step `element` maps to: its expression, and
    /// the property name for a set-variable.
    fn set_step(
        element: &MuleElement,
        rules: &[Rule],
    ) -> (Option<String>, Expression, Vec<MappingDecision>) {
        let (steps, decisions) = steps_and_decisions(element, rules);
        match <[RouteStep; 1]>::try_from(steps) {
            Ok([RouteStep::SetProperty { name, expression }]) => {
                (Some(name), expression, decisions)
            }
            Ok([RouteStep::SetBody { expression }]) => (None, expression, decisions),
            other => panic!("expected one setProperty or setBody step, got {other:?}"),
        }
    }

    #[test]
    fn set_variable_with_an_expression_reads_it_from_the_exchange() {
        for (value, expected) in [
            ("#[payload]", simple("${body}")),
            ("#[vars.other]", simple("${exchangeProperty.other}")),
            ("#[42]", Expression::Literal("42".into())),
            ("#['text']", Expression::Constant("text".into())),
        ] {
            let set = elem(
                "",
                "set-variable",
                &[("variableName", "original"), ("value", value)],
            );
            for rules in shipped_and_builtin_rules() {
                let (name, expression, decisions) = set_step(&set, &rules);
                assert_eq!(name.as_deref(), Some("original"));
                assert_eq!(expression, expected, "{value}");
                assert_eq!(decisions.len(), 1, "{decisions:?}");
                assert_eq!(decisions[0].status, DecisionStatus::Done);
            }
        }
    }

    #[test]
    fn set_variable_with_a_literal_is_a_string_constant() {
        let set = elem(
            "",
            "set-variable",
            &[("variableName", "enriched"), ("value", "true")],
        );
        for rules in shipped_and_builtin_rules() {
            let (name, expression, decisions) = set_step(&set, &rules);
            assert_eq!(name.as_deref(), Some("enriched"));
            assert_eq!(expression, Expression::Constant("true".into()));
            assert_eq!(decisions.len(), 1, "{decisions:?}");
            assert_eq!(decisions[0].status, DecisionStatus::Done);
        }
    }

    #[test]
    fn set_variable_with_an_unconvertible_expression_is_flagged() {
        let set = elem(
            "",
            "set-variable",
            &[("variableName", "id"), ("value", "#[payload.id]")],
        );
        for rules in shipped_and_builtin_rules() {
            let (name, expression, decisions) = set_step(&set, &rules);
            assert_eq!(name.as_deref(), Some("id"));
            assert!(
                is_unconverted(&expression, "#[payload.id]"),
                "{expression:?}"
            );
            assert_eq!(decisions.len(), 1, "{decisions:?}");
            assert_eq!(decisions[0].status, DecisionStatus::ManualReview);
            assert!(
                decisions[0].rationale.contains("#[payload.id]"),
                "{}",
                decisions[0].rationale
            );
        }
    }

    #[test]
    fn set_payload_with_an_expression_a_literal_and_an_unconvertible_expression() {
        for (value, expected, status) in [
            (
                "#[vars.original]",
                Some(simple("${exchangeProperty.original}")),
                DecisionStatus::Done,
            ),
            ("#[payload]", Some(simple("${body}")), DecisionStatus::Done),
            (
                "Routed to handler A",
                Some(Expression::Constant("Routed to handler A".into())),
                DecisionStatus::Done,
            ),
            ("#[now()]", None, DecisionStatus::ManualReview),
            (
                "#[output application/json --- {a: payload.b}]",
                None,
                DecisionStatus::ManualReview,
            ),
        ] {
            let set = elem("", "set-payload", &[("value", value)]);
            for rules in shipped_and_builtin_rules() {
                let (name, expression, decisions) = set_step(&set, &rules);
                assert_eq!(name, None);
                match &expected {
                    Some(expected) => assert_eq!(&expression, expected, "{value}"),
                    None => assert!(is_unconverted(&expression, value), "{expression:?}"),
                }
                assert_eq!(decisions.len(), 1, "{decisions:?}");
                assert_eq!(decisions[0].status, status, "{value}");
                if status == DecisionStatus::ManualReview {
                    assert!(
                        decisions[0].rationale.contains(value),
                        "{}",
                        decisions[0].rationale
                    );
                }
            }
        }
    }

    /// A rule without URI template (`set-payload`, `choice-router`, ...)
    /// only names the Camel construct; the built-in mapping does the work.
    /// The element gets one decision, under the rule's id, with the status
    /// the built-in mapping found - not a second row that says "done".
    #[test]
    fn rule_without_uri_template_does_not_add_a_done_row_of_its_own() {
        let [shipped, _] = shipped_and_builtin_rules();

        let set = elem("", "set-payload", &[("value", "#[now()]")]);
        let (_, decisions) = steps_and_decisions(&set, &shipped);
        assert_eq!(decisions.len(), 1, "{decisions:?}");
        assert_eq!(decisions[0].rule_id.as_deref(), Some("set-payload"));
        assert_eq!(decisions[0].status, DecisionStatus::ManualReview);

        let set = elem("", "set-payload", &[("value", "ok")]);
        let (_, decisions) = steps_and_decisions(&set, &shipped);
        assert_eq!(decisions.len(), 1, "{decisions:?}");
        assert_eq!(decisions[0].rule_id.as_deref(), Some("set-payload"));
        assert_eq!(decisions[0].status, DecisionStatus::Done);

        // A rule with URI template still decides on its own.
        let rules = rules_from_yaml(PARAM_RULES);
        let send = elem("x", "send", &[("name", "n")]);
        let (_, decisions) = steps_and_decisions(&send, &rules);
        assert_eq!(decisions.len(), 1, "{decisions:?}");
        assert_eq!(decisions[0].rule_id.as_deref(), Some("x-send"));
        assert_eq!(decisions[0].status, DecisionStatus::Done);
    }
}
