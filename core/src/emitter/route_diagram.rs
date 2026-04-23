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

fn step_label(step: &RouteStep) -> String {
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
        RouteStep::Choice { whens, .. } => format!("choice({} branches)", whens.len()),
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
