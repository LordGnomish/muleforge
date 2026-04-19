//! Architecture overview document.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let mut md = String::new();
    md.push_str("# Architecture\n\n");
    md.push_str(&format!("## {}\n\n", ir.name));
    md.push_str(
        "This project was migrated from MuleSoft 4 to Apache Camel Quarkus using MuleForge.\n\n",
    );

    md.push_str("## Technology Stack\n\n");
    md.push_str("- **Runtime:** Quarkus (supersonic, subatomic Java)\n");
    md.push_str("- **Integration:** Apache Camel\n");
    md.push_str("- **Build:** Maven\n");
    md.push_str("- **Container:** Docker (JVM and native variants)\n");
    md.push_str("- **Orchestration:** Kubernetes with Kustomize overlays\n\n");

    md.push_str("## Route Overview\n\n");
    md.push_str("| Route ID | Source | Steps |\n");
    md.push_str("|----------|--------|-------|\n");
    for route in &ir.routes {
        let source = match &route.source {
            crate::ast::camel_ir::RouteEndpoint::Uri(u) => u.clone(),
            crate::ast::camel_ir::RouteEndpoint::Direct(d) => format!("direct:{}", d),
        };
        md.push_str(&format!(
            "| {} | `{}` | {} |\n",
            route.id,
            source,
            route.steps.len()
        ));
    }

    md.push_str("\n## Dependencies\n\n");
    for dep in &ir.maven_dependencies {
        md.push_str(&format!("- `{}:{}`", dep.group_id, dep.artifact_id));
        if let Some(ref v) = dep.version {
            md.push_str(&format!(" ({})", v));
        }
        md.push('\n');
    }

    std::fs::write(docs_dir.join("architecture.md"), md)?;
    Ok(())
}
