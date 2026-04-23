//! README.md generator for the output project.

use crate::ast::camel_ir::CamelProject;
use crate::report::MigrationReport;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, report: &MigrationReport, output_dir: &Path) -> Result<()> {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", ir.name));
    md.push_str("Migrated from MuleSoft 4 to Apache Camel Quarkus using [MuleForge](https://github.com/muleforge/muleforge).\n\n");

    md.push_str("## Quick Start\n\n");
    md.push_str("```bash\n");
    md.push_str("# Build\n");
    md.push_str("mvn clean package\n\n");
    md.push_str("# Run\n");
    md.push_str("java -jar target/quarkus-app/quarkus-run.jar\n\n");
    md.push_str("# Run in dev mode\n");
    md.push_str("mvn quarkus:dev\n\n");
    md.push_str("# Build native\n");
    md.push_str("mvn package -Pnative\n");
    md.push_str("```\n\n");

    md.push_str("## Routes\n\n");
    md.push_str(&format!(
        "This project contains {} route(s):\n\n",
        ir.routes.len()
    ));
    for route in &ir.routes {
        let source = match &route.source {
            crate::ast::camel_ir::RouteEndpoint::Uri(u) => u.clone(),
            crate::ast::camel_ir::RouteEndpoint::Direct(d) => format!("direct:{}", d),
        };
        md.push_str(&format!("- **{}** — `{}`\n", route.id, source));
    }

    md.push_str("\n## Migration Summary\n\n");
    md.push_str(&format!(
        "- Total elements: {}\n",
        report.summary.total_elements
    ));
    md.push_str(&format!(
        "- Automatically migrated: {}\n",
        report.summary.done
    ));
    md.push_str(&format!(
        "- Needs manual review: {}\n",
        report.summary.manual_review
    ));
    md.push_str(&format!("- Skipped: {}\n", report.summary.skipped));
    md.push_str("\nSee [MIGRATION_REPORT.md](./MIGRATION_REPORT.md) for details.\n\n");

    md.push_str("## Documentation\n\n");
    md.push_str("- [Architecture](./docs/architecture.md)\n");
    md.push_str("- [Local Setup](./docs/development/local-setup.md)\n");
    md.push_str("- [Testing](./docs/development/testing.md)\n");
    md.push_str("- [Operations Runbook](./docs/operations/runbook.md)\n");

    std::fs::write(output_dir.join("README.md"), md)?;
    Ok(())
}
