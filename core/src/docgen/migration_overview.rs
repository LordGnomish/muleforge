//! Migration overview document.

use crate::report::MigrationReport;
use crate::Result;
use std::path::Path;

pub fn generate(report: &MigrationReport, docs_dir: &Path) -> Result<()> {
    let mig_dir = docs_dir.join("migration");
    std::fs::create_dir_all(&mig_dir)?;

    let mut md = String::new();
    md.push_str("# Migration Overview\n\n");
    md.push_str(&format!("**Source:** {}\n", report.source_description));
    if let Some(ref sha) = report.source_commit {
        md.push_str(&format!("**Source commit:** `{}`\n", sha));
    }
    md.push_str("\n## What Changed\n\n");
    md.push_str("This project was migrated from MuleSoft 4 to Apache Camel Quarkus.\n\n");
    md.push_str("### Key Differences\n\n");
    md.push_str("| Aspect | Mule 4 | Camel Quarkus |\n");
    md.push_str("|--------|--------|---------------|\n");
    md.push_str("| Runtime | Mule Runtime | Quarkus + Camel |\n");
    md.push_str("| Language | XML + DataWeave | Java + Simple |\n");
    md.push_str("| Build | Maven (Mule plugin) | Maven (Quarkus plugin) |\n");
    md.push_str("| Deployment | CloudHub / On-prem | Container / Kubernetes |\n");
    md.push_str("| Config | Mule properties | Quarkus application.properties |\n");

    md.push_str("\n## Migration Statistics\n\n");
    md.push_str(&format!(
        "- **{}** elements processed\n",
        report.summary.total_elements
    ));
    md.push_str(&format!(
        "- **{}** automatically migrated\n",
        report.summary.done
    ));
    md.push_str(&format!(
        "- **{}** need manual review\n",
        report.summary.manual_review
    ));
    md.push_str(&format!("- **{}** skipped\n", report.summary.skipped));

    if report.summary.manual_review > 0 {
        md.push_str("\n## Manual Review Items\n\n");
        for d in &report.decisions {
            if d.status == crate::report::DecisionStatus::ManualReview {
                md.push_str(&format!("- `{}`: {}\n", d.mule_element, d.rationale));
            }
        }
    }

    std::fs::write(mig_dir.join("overview.md"), md)?;
    Ok(())
}
