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
            let route = map_sub_flow(flow, rules, &mut decisions, &mut all_deps);
            camel.routes.push(route);
        } else {
            let route = map_flow(flow, rules, &mut decisions, &mut all_deps);
            camel.routes.push(route);
        }
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

fn map_flow(
    flow: &crate::ast::mule_ast::MuleFlow,
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
        let step = map_processor(proc, rules, decisions, deps);
        steps.extend(step);
    }

    let on_exceptions = flow
        .error_handlers
        .iter()
        .flat_map(|eh| map_error_handler(eh, rules, decisions, deps))
        .collect();

    CamelRoute {
        id: flow.name.clone(),
        source,
        steps,
        on_exceptions,
    }
}

fn map_sub_flow(
    flow: &crate::ast::mule_ast::MuleFlow,
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> CamelRoute {
    let steps: Vec<RouteStep> = flow
        .processors
        .iter()
        .flat_map(|p| map_processor(p, rules, decisions, deps))
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

fn map_processor(
    elem: &MuleElement,
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> Vec<RouteStep> {
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
        if let Some(ref uri) = rule.camel_uri_template {
            return vec![RouteStep::ToUri(rule_endpoint_uri(rule, uri, elem))];
        }
    }

    // Built-in processor mappings
    match (elem.namespace.as_str(), elem.name.as_str()) {
        ("", "set-payload") => {
            let val = elem.attr("value").unwrap_or("").to_string();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "set-payload -> setBody",
            );
            vec![RouteStep::SetBody {
                expression: Expression::Constant(val),
            }]
        }
        ("", "set-variable") => {
            let name = elem.attr("variableName").unwrap_or("var").to_string();
            let val = elem.attr("value").unwrap_or("").to_string();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "set-variable -> setProperty",
            );
            vec![RouteStep::SetProperty {
                name,
                expression: Expression::Constant(val),
            }]
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
                record_decision(decisions, elem, DecisionStatus::Done, None, "logger -> log");
                converted
            } else {
                record_decision(
                    decisions,
                    elem,
                    DecisionStatus::ManualReview,
                    None,
                    "logger -> log (message has an unconverted MEL expression)",
                );
                raw.to_string()
            };
            vec![RouteStep::Log {
                level,
                message: msg,
            }]
        }
        ("", "choice") => {
            let mut whens = Vec::new();
            let mut otherwise = None;
            for child in &elem.children {
                if child.name == "when" {
                    let expr = child.attr("expression").unwrap_or("true").to_string();
                    let steps: Vec<RouteStep> = child
                        .children
                        .iter()
                        .flat_map(|c| map_processor(c, rules, decisions, deps))
                        .collect();
                    whens.push((Expression::Simple(expr), steps));
                } else if child.name == "otherwise" {
                    let steps: Vec<RouteStep> = child
                        .children
                        .iter()
                        .flat_map(|c| map_processor(c, rules, decisions, deps))
                        .collect();
                    otherwise = Some(steps);
                }
            }
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "choice -> choice",
            );
            vec![RouteStep::Choice { whens, otherwise }]
        }
        ("", "scatter-gather") | ("", "split") => {
            let expr = elem.attr("expression").unwrap_or("#[payload]").to_string();
            let parallel = elem.attr("parallel").map(|v| v == "true").unwrap_or(false);
            let steps: Vec<RouteStep> = elem
                .children
                .iter()
                .flat_map(|c| map_processor(c, rules, decisions, deps))
                .collect();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
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
                .flat_map(|c| map_processor(c, rules, decisions, deps))
                .collect();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "foreach -> split",
            );
            vec![RouteStep::Split {
                expression: Expression::Simple(expr),
                parallel: false,
                aggregation_strategy: None,
                steps,
            }]
        }
        ("", "flow-ref") => {
            let name = elem.attr("name").unwrap_or("unknown").to_string();
            record_decision(
                decisions,
                elem,
                DecisionStatus::Done,
                None,
                "flow-ref -> direct endpoint",
            );
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
                None,
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
                None,
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
                None,
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
                None,
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
                None,
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
                None,
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
                None,
                &format!("No mapping for {} - needs manual review", qname),
            );
            vec![RouteStep::RawDsl(format!(
                "// TODO: manual mapping for {}",
                qname
            ))]
        }
    }
}

fn map_error_handler(
    eh: &MuleElement,
    rules: &[Rule],
    decisions: &mut Vec<MappingDecision>,
    deps: &mut Vec<MavenDependency>,
) -> Vec<ExceptionHandler> {
    let mut handlers = Vec::new();
    for child in &eh.children {
        if child.name == "on-error-propagate" || child.name == "on-error-continue" {
            let handled = child.name == "on-error-continue";
            let exception_type = child.attr("type").unwrap_or("java.lang.Exception");
            let steps: Vec<RouteStep> = child
                .children
                .iter()
                .flat_map(|c| map_processor(c, rules, decisions, deps))
                .collect();
            record_decision(
                decisions,
                child,
                DecisionStatus::Done,
                None,
                &format!("{} -> onException(handled={})", child.name, handled),
            );
            handlers.push(ExceptionHandler {
                exception_classes: vec![map_mule_error_type(exception_type)],
                handled,
                steps,
            });
        }
    }
    handlers
}

fn map_mule_error_type(mule_type: &str) -> String {
    match mule_type {
        "HTTP:CONNECTIVITY" | "HTTP:TIMEOUT" => "java.net.ConnectException".into(),
        "HTTP:UNAUTHORIZED" | "HTTP:FORBIDDEN" => {
            "org.apache.camel.http.base.HttpOperationFailedException".into()
        }
        "DB:CONNECTIVITY" | "DB:BAD_SQL_SYNTAX" => "java.sql.SQLException".into(),
        "VALIDATION:INVALID_PAYLOAD" => "org.apache.camel.ValidationException".into(),
        _ => "java.lang.Exception".into(),
    }
}

fn find_rule<'a>(rules: &'a [Rule], ns: &str, name: &str) -> Option<&'a Rule> {
    rules
        .iter()
        .find(|r| r.mule_ns == ns && r.mule_name == name)
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
        map_processor(e, rules, &mut Vec::new(), &mut Vec::new())
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
}
