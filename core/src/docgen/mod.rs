//! Documentation generation for the output Camel Quarkus project.
//!
//! Generates a complete `/docs` tree: architecture overview, per-flow
//! reference pages, operations runbook, local development guide, and more.
//! When an LLM provider is configured, the generated docs include rich
//! explanations. Without LLM, structured stubs with TODO markers are written.

use std::path::Path;

use crate::ast::camel_ir::CamelProject;
use crate::llm::LlmProvider;
use crate::report::MigrationReport;
use crate::Result;

pub mod architecture;
pub mod contributing;
pub mod debugging;
pub mod deployment;
pub mod flow_pages;
pub mod local_setup;
pub mod migration_gotchas;
pub mod migration_overview;
pub mod observability;
pub mod readme;
pub mod runbook;
pub mod testing;

/// What to generate.
#[derive(Debug, Clone)]
pub struct DocgenConfig {
    pub enabled: bool,
    pub sections: Vec<DocSection>,
    pub style: DocStyle,
}

impl Default for DocgenConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sections: vec![
                DocSection::Architecture,
                DocSection::PerFlow,
                DocSection::Runbook,
                DocSection::LocalSetup,
                DocSection::Testing,
                DocSection::MigrationOverview,
            ],
            style: DocStyle::Technical,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocSection {
    Architecture,
    PerFlow,
    Runbook,
    LocalSetup,
    Testing,
    MigrationOverview,
    MigrationGotchas,
    Observability,
    Deployment,
    Debugging,
    Contributing,
    Readme,
}

#[derive(Debug, Clone)]
pub enum DocStyle {
    Technical,
    Accessible,
}

/// Generate all configured documentation sections.
pub async fn generate(
    ir: &CamelProject,
    report: &MigrationReport,
    output_dir: &Path,
    config: &DocgenConfig,
    _llm: Option<&dyn LlmProvider>,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let docs_dir = output_dir.join("docs");
    std::fs::create_dir_all(&docs_dir)?;

    for section in &config.sections {
        match section {
            DocSection::Architecture => architecture::generate(ir, &docs_dir)?,
            DocSection::PerFlow => flow_pages::generate(ir, report, &docs_dir)?,
            DocSection::Runbook => runbook::generate(ir, &docs_dir)?,
            DocSection::LocalSetup => local_setup::generate(ir, &docs_dir)?,
            DocSection::Testing => testing::generate(ir, &docs_dir)?,
            DocSection::MigrationOverview => migration_overview::generate(report, &docs_dir)?,
            DocSection::MigrationGotchas => migration_gotchas::generate(report, &docs_dir)?,
            DocSection::Observability => observability::generate(ir, &docs_dir)?,
            DocSection::Deployment => deployment::generate(ir, &docs_dir)?,
            DocSection::Debugging => debugging::generate(ir, &docs_dir)?,
            DocSection::Contributing => contributing::generate(&docs_dir)?,
            DocSection::Readme => readme::generate(ir, report, output_dir)?,
        }
    }

    // Always generate README
    if !config.sections.contains(&DocSection::Readme) {
        readme::generate(ir, report, output_dir)?;
    }

    // Always generate CONTRIBUTING
    if !config.sections.contains(&DocSection::Contributing) {
        contributing::generate(&docs_dir)?;
    }

    Ok(())
}
