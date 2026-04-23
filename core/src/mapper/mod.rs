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
    pub camel_component: Option<String>,
    pub maven_deps: Vec<MavenDependency>,
    pub notes: String,
}

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
            return RouteEndpoint::Uri(expand_uri_template(tmpl, elem));
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
        ("kafka", "consumer") | ("kafka", "listener") => {
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
            RouteEndpoint::Uri(format!("kafka:{}", topic))
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
            return vec![RouteStep::ToUri(expand_uri_template(uri, elem))];
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
            let msg = elem.attr("message").unwrap_or("").to_string();
            let level = match elem.attr("level").unwrap_or("INFO") {
                "DEBUG" => LogLevel::Debug,
                "WARN" | "WARNING" => LogLevel::Warn,
                "ERROR" => LogLevel::Error,
                "TRACE" => LogLevel::Trace,
                _ => LogLevel::Info,
            };
            record_decision(decisions, elem, DecisionStatus::Done, None, "logger -> log");
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
            vec![RouteStep::ToUri(format!("kafka:{}", topic))]
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
