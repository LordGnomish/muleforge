//! Per-flow documentation pages.

use crate::ast::camel_ir::{CamelProject, RouteEndpoint, RouteStep};
use crate::report::MigrationReport;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, report: &MigrationReport, docs_dir: &Path) -> Result<()> {
    let flows_dir = docs_dir.join("flows");
    std::fs::create_dir_all(&flows_dir)?;

    for route in &ir.routes {
        let mut md = String::new();
        md.push_str(&format!("# {}\n\n", route.id));

        let source = match &route.source {
            RouteEndpoint::Uri(u) => u.clone(),
            RouteEndpoint::Direct(d) => format!("direct:{}", d),
        };
        md.push_str(&format!("**Source endpoint:** `{}`\n\n", source));
        md.push_str(&format!("**Steps:** {}\n\n", route.steps.len()));

        if !route.on_exceptions.is_empty() {
            md.push_str(&format!(
                "**Exception handlers:** {}\n\n",
                route.on_exceptions.len()
            ));
        }

        md.push_str("## Processing Steps\n\n");
        for (i, step) in route.steps.iter().enumerate() {
            md.push_str(&format!("{}. {}\n", i + 1, describe_step(step)));
        }

        // Link related migration decisions
        let related: Vec<_> = report
            .decisions
            .iter()
            .filter(|d| d.flow_name.as_deref() == Some(&route.id))
            .collect();
        if !related.is_empty() {
            md.push_str("\n## Migration Decisions\n\n");
            for d in related {
                md.push_str(&format!(
                    "- `{}` — {:?}: {}\n",
                    d.mule_element, d.status, d.rationale
                ));
            }
        }

        let filename = route.id.replace(' ', "-").to_lowercase();
        std::fs::write(flows_dir.join(format!("{}.md", filename)), md)?;
    }

    Ok(())
}

fn describe_step(step: &RouteStep) -> String {
    match step {
        RouteStep::SetBody { .. } => "Set message body".into(),
        RouteStep::SetHeader { name, .. } => format!("Set header `{}`", name),
        RouteStep::SetProperty { name, .. } => format!("Set property `{}`", name),
        RouteStep::RemoveProperty { name } => format!("Remove property `{}`", name),
        RouteStep::Log { message, .. } => {
            format!("Log: {}", message.chars().take(60).collect::<String>())
        }
        RouteStep::ToUri(uri) => format!("Send to `{}`", uri),
        RouteStep::ProcessBean(b) => format!("Process bean `{}`", b),
        RouteStep::Choice { whens, .. } => format!("Choice router ({} branches)", whens.len()),
        RouteStep::Split { parallel, .. } => {
            if *parallel {
                "Parallel split".into()
            } else {
                "Split".into()
            }
        }
        RouteStep::Marshal(fmt) => format!("Marshal ({:?})", fmt),
        RouteStep::Unmarshal(fmt) => format!("Unmarshal ({:?})", fmt),
        RouteStep::Transform { bean_ref } => format!("Transform via `{}`", bean_ref),
        RouteStep::ThrowException { class, .. } => format!("Throw `{}`", class),
        RouteStep::RawDsl(dsl) => format!("Custom: {}", dsl.chars().take(60).collect::<String>()),
    }
}
