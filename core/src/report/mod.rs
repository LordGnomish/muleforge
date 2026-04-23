//! Migration report: the audit trail for every mapping decision.
//!
//! Emitted both as a Markdown file in the output repo (`MIGRATION_REPORT.md`)
//! and as structured input to the docgen phase.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::git::AcquiredInput;
use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub source_description: String,
    pub source_commit: Option<String>,
    pub decisions: Vec<MappingDecision>,
    pub summary: ReportSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_elements: usize,
    pub done: usize,
    pub manual_review: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingDecision {
    pub mule_element: String,
    pub source_file: String,
    pub source_line: Option<u32>,
    pub flow_name: Option<String>,
    pub status: DecisionStatus,
    pub rule_id: Option<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionStatus {
    Done,
    ManualReview,
    Skipped,
}

pub fn build(decisions: &[MappingDecision], acquired: &AcquiredInput) -> MigrationReport {
    let mut summary = ReportSummary {
        total_elements: decisions.len(),
        done: 0,
        manual_review: 0,
        skipped: 0,
    };
    for d in decisions {
        match d.status {
            DecisionStatus::Done => summary.done += 1,
            DecisionStatus::ManualReview => summary.manual_review += 1,
            DecisionStatus::Skipped => summary.skipped += 1,
        }
    }
    MigrationReport {
        source_description: acquired.source_description.clone(),
        source_commit: acquired.source_commit.clone(),
        decisions: decisions.to_vec(),
        summary,
    }
}

pub fn write(report: &MigrationReport, path: &Path) -> Result<()> {
    let md = render_markdown(report);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, md)?;
    Ok(())
}

fn render_markdown(r: &MigrationReport) -> String {
    let mut out = String::new();
    out.push_str("# Migration Report\n\n");
    out.push_str(&format!("**Source:** {}\n", r.source_description));
    if let Some(sha) = &r.source_commit {
        out.push_str(&format!("**Source commit:** `{}`\n", sha));
    }
    out.push_str("\n## Summary\n\n");
    out.push_str(&format!("- Total elements: {}\n", r.summary.total_elements));
    out.push_str(&format!("- ✅ DONE:          {}\n", r.summary.done));
    out.push_str(&format!(
        "- ⚠️ MANUAL_REVIEW: {}\n",
        r.summary.manual_review
    ));
    out.push_str(&format!("- ⏭️ SKIPPED:       {}\n\n", r.summary.skipped));

    out.push_str("## Decisions\n\n");
    out.push_str("| Element | File | Line | Flow | Status | Rule | Rationale |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for d in &r.decisions {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {:?} | {} | {} |\n",
            d.mule_element,
            d.source_file,
            d.source_line.map(|n| n.to_string()).unwrap_or_default(),
            d.flow_name.clone().unwrap_or_default(),
            d.status,
            d.rule_id.clone().unwrap_or_default(),
            d.rationale.replace('|', "\\|").replace('\n', " ")
        ));
    }
    out
}
