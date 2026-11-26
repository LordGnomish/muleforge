//! Route diagram generator.
//!
//! Generates a Mermaid flowchart showing all routes, their sources,
//! processors, and connections. Included in the generated docs.

use std::path::Path;

use crate::ast::camel_ir::*;
use crate::Result;

pub fn generate(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let docs_dir = output_dir.join("docs");
    std::fs::create_dir_all(&docs_dir)?;

    let mut mermaid = String::new();
    mermaid.push_str("```mermaid\nflowchart LR\n");

    for route in &ir.routes {
        let route_id = sanitize_id(&route.id);

        // Source node
        let source_label = match &route.source {
            RouteEndpoint::Uri(u) => {
                let short = u.split('?').next().unwrap_or(u);
                if short.len() > 40 {
                    format!("{}...", &short[..37])
                } else {
                    short.to_string()
                }
            }
            RouteEndpoint::Direct(d) => format!("direct:{}", d),
        };
        mermaid.push_str(&format!(
            "    {}__src[\"{}\\n{}\"]:::source\n",
            route_id, route.id, source_label
        ));

        // Process steps
        let mut prev = format!("{}__src", route_id);
        for (i, step) in route.steps.iter().enumerate() {
            let step_id = format!("{}__s{}", route_id, i);
            let label = step_label(step);
            let class = step_class(step);

            mermaid.push_str(&format!("    {}[\"{}\"]:::{}\n", step_id, label, class));
            mermaid.push_str(&format!("    {} --> {}\n", prev, step_id));
            prev = step_id;
        }

        mermaid.push('\n');
    }

    // Add direct: connections between routes
    for route in &ir.routes {
        let route_id = sanitize_id(&route.id);
        for (i, step) in route.steps.iter().enumerate() {
            if let RouteStep::ToUri(ref uri) = step {
                if uri.starts_with("direct:") {
                    let target_name = uri.strip_prefix("direct:").unwrap_or("");
                    let target_id = sanitize_id(target_name);
                    // Link to target route's source node
                    mermaid.push_str(&format!(
                        "    {}__s{} -.-> {}__src\n",
                        route_id, i, target_id
                    ));
                }
            }
        }
    }

    // Styles
    mermaid.push_str("\n    classDef source fill:#4CAF50,color:#fff,stroke:#333\n");
    mermaid.push_str("    classDef processor fill:#2196F3,color:#fff,stroke:#333\n");
    mermaid.push_str("    classDef endpoint fill:#FF9800,color:#fff,stroke:#333\n");
    mermaid.push_str("    classDef router fill:#9C27B0,color:#fff,stroke:#333\n");
    mermaid.push_str("    classDef transform fill:#F44336,color:#fff,stroke:#333\n");
    mermaid.push_str("```\n");

    // Write as part of architecture doc
    let mut md = String::new();
    md.push_str("# Route Diagram\n\n");
    md.push_str("Visual overview of all migrated routes and their connections.\n\n");
    md.push_str(&mermaid);
    md.push_str("\n\n## Legend\n\n");
    md.push_str("- **Green**: Source endpoints (HTTP listener, Kafka consumer, etc.)\n");
    md.push_str("- **Blue**: Processors (set-payload, log, etc.)\n");
    md.push_str("- **Orange**: Target endpoints (HTTP request, DB, Kafka publish, etc.)\n");
    md.push_str("- **Purple**: Routers (choice, split, etc.)\n");
    md.push_str("- **Red**: Transforms (DataWeave, bean, etc.)\n");
    md.push_str("- **Dashed arrows**: direct: endpoint connections between routes\n");

    std::fs::write(docs_dir.join("route-diagram.md"), md)?;
    Ok(())
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// The label of a step in the diagram.
///
/// A step whose value is not converted is emitted as a step that throws
/// (see `RouteStep::is_unconverted`), and an unconverted choice condition
/// as a predicate that throws: the label says so.
fn step_label(step: &RouteStep) -> String {
    if step.is_unconverted() {
        let what = match step {
            RouteStep::SetBody { .. } => "setBody".to_string(),
            RouteStep::SetHeader { name, .. } => format!("setHeader({name})"),
            RouteStep::SetProperty { name, .. } => format!("setProperty({name})"),
            _ => "split".to_string(),
        };
        return format!("NOT MIGRATED: {what} (fails here)");
    }
    match step {
        RouteStep::SetBody { .. } => "setBody".into(),
        RouteStep::SetHeader { name, .. } => format!("setHeader({})", name),
        RouteStep::SetProperty { name, .. } => format!("setProperty({})", name),
        RouteStep::RemoveProperty { name } => format!("removeProperty({})", name),
        RouteStep::Log { message, .. } => {
            let short = if message.len() > 25 {
                &message[..22]
            } else {
                message
            };
            format!("log({}...)", short)
        }
        RouteStep::ToUri(uri) => {
            let short = uri.split('?').next().unwrap_or(uri);
            if short.len() > 30 {
                format!("to({}...)", &short[..27])
            } else {
                format!("to({})", short)
            }
        }
        RouteStep::ProcessBean(b) => format!("bean({})", b),
        RouteStep::Choice { whens, .. } => {
            let unconverted = whens
                .iter()
                .filter(|(condition, _)| matches!(condition, Expression::Unconverted { .. }))
                .count();
            if unconverted > 0 {
                format!(
                    "choice({} branches, {unconverted} NOT MIGRATED)",
                    whens.len()
                )
            } else {
                format!("choice({} branches)", whens.len())
            }
        }
        RouteStep::Split { parallel, .. } => {
            if *parallel {
                "split(parallel)".into()
            } else {
                "split".into()
            }
        }
        RouteStep::Marshal(fmt) => format!("marshal({:?})", fmt),
        RouteStep::Unmarshal(fmt) => format!("unmarshal({:?})", fmt),
        RouteStep::Transform { bean_ref } => format!("transform({})", bean_ref),
        RouteStep::ThrowException { class, .. } => format!("throw({})", class),
        RouteStep::RawDsl(dsl) => {
            let short = dsl.lines().next().unwrap_or(dsl);
            if short.len() > 30 {
                format!("{}...", &short[..27])
            } else {
                short.to_string()
            }
        }
    }
}

fn step_class(step: &RouteStep) -> &'static str {
    match step {
        RouteStep::ToUri(_) => "endpoint",
        RouteStep::Choice { .. } | RouteStep::Split { .. } => "router",
        RouteStep::Transform { .. } | RouteStep::ProcessBean(_) => "transform",
        _ => "processor",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unconverted() -> Expression {
        Expression::Unconverted {
            original: "#[now()]".into(),
            reason: "why".into(),
        }
    }

    /// A source that is not migrated is a `direct:` endpoint nothing sends
    /// to: the diagram names the Mule source and says so.
    #[test]
    fn source_that_is_not_migrated_is_labelled_as_not_migrated() {
        assert_eq!(
            source_label(&RouteEndpoint::Unmigrated {
                element: "sftp:listener".into(),
                placeholder: "sftp-listener-ordersFlow".into(),
                notes: vec!["why".into()],
            }),
            "NOT MIGRATED: sftp:listener"
        );
        assert_eq!(
            source_label(&RouteEndpoint::Uri(
                "platform-http:/orders?httpMethodRestrict=GET".into()
            )),
            "platform-http:/orders"
        );
        assert_eq!(
            source_label(&RouteEndpoint::Direct("shared".into())),
            "direct:shared"
        );
    }

    /// A step whose value is not converted is emitted as a step that
    /// throws: the diagram must not label it as the step it would be.
    #[test]
    fn step_with_an_unconverted_value_is_labelled_as_not_migrated() {
        assert_eq!(
            step_label(&RouteStep::SetBody {
                expression: unconverted()
            }),
            "NOT MIGRATED: setBody (fails here)"
        );
        assert_eq!(
            step_label(&RouteStep::SetHeader {
                name: "x-id".into(),
                expression: unconverted()
            }),
            "NOT MIGRATED: setHeader(x-id) (fails here)"
        );
        assert_eq!(
            step_label(&RouteStep::SetProperty {
                name: "id".into(),
                expression: unconverted()
            }),
            "NOT MIGRATED: setProperty(id) (fails here)"
        );
        assert_eq!(
            step_label(&RouteStep::Split {
                expression: unconverted(),
                parallel: true,
                stop_on_exception: false,
                aggregation_strategy: None,
                source: SplitSource::default(),
                steps: vec![],
            }),
            "NOT MIGRATED: split (fails here)"
        );
        assert_eq!(
            step_label(&RouteStep::SetBody {
                expression: Expression::Constant("text".into())
            }),
            "setBody"
        );
    }

    #[test]
    fn choice_label_counts_the_conditions_that_are_not_migrated() {
        let choice = |conditions: Vec<Expression>| RouteStep::Choice {
            whens: conditions.into_iter().map(|c| (c, vec![])).collect(),
            otherwise: None,
        };
        assert_eq!(
            step_label(&choice(vec![
                Expression::Literal("true".into()),
                unconverted()
            ])),
            "choice(2 branches, 1 NOT MIGRATED)"
        );
        assert_eq!(
            step_label(&choice(vec![Expression::Literal("true".into())])),
            "choice(1 branches)"
        );
    }
}
