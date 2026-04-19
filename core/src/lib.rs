//! MuleForge core: transforms a MuleSoft 4 Git repository into an
//! Apache Camel Quarkus Git repository — with generated documentation,
//! tests, container image config, and Kubernetes manifests.
//!
//! Pipeline:
//!   git::acquire → parse → normalize → map → emit (code + config + docs)
//!   → assemble → git::emit
//!
//! See `ARCHITECTURE.md` for the full design.

pub mod ast;
pub mod dataweave;
pub mod docgen;
pub mod emitter;
pub mod git;
pub mod llm;
pub mod mapper;
pub mod mel;
pub mod parser;
pub mod report;

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MuleForgeError {
    #[error("failed to read input: {0}")]
    InputIo(#[from] std::io::Error),

    #[error("failed to parse Mule XML: {0}")]
    Parse(String),

    #[error("failed to load mapping rules: {0}")]
    Mapping(String),

    #[error("emitter error: {0}")]
    Emit(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("git error: {0}")]
    Git(String),

    #[error("docgen error: {0}")]
    Docgen(String),
}

pub type Result<T> = std::result::Result<T, MuleForgeError>;

/// Configuration for a single migration run.
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Where to read the Mule application from.
    pub input: git::InputSource,
    /// Directory where the Camel Quarkus project will be written.
    pub output_dir: PathBuf,
    /// Directory containing mapping rules (YAML files).
    pub mappings_dir: PathBuf,
    /// LLM provider configuration; `None` disables LLM fallback.
    pub llm: Option<llm::LlmConfig>,
    /// Git emit options for the output repository.
    pub git: git::GitEmitOptions,
    /// Documentation generation options.
    pub docgen: docgen::DocgenConfig,
    /// Also emit Kong Konnect declarative config.
    pub emit_kong_config: bool,
    /// Also emit Kubernetes manifests.
    pub emit_k8s: bool,
    /// Overwrite non-empty output directory if true.
    pub force: bool,
}

/// Entry point: run a full migration end-to-end.
pub async fn migrate(cfg: &MigrationConfig) -> Result<report::MigrationReport> {
    tracing::info!("starting migration");

    // 0. Safety: refuse to overwrite a non-empty output directory unless forced.
    ensure_output_dir_usable(&cfg.output_dir, cfg.force)?;

    // 1. Acquire input (local path or remote clone).
    let acquired = git::acquire::acquire(&cfg.input)?;
    tracing::info!("acquired input from: {}", acquired.source_description);

    // 2. Parse + normalize.
    let mule_project = parser::parse_project(&acquired.working_dir)?;
    tracing::info!("parsed {} flow(s)", mule_project.flows.len());
    let normalized = parser::normalize(mule_project)?;

    // 3. Load mapping rules.
    let rules = mapper::load_rules(&cfg.mappings_dir)?;
    tracing::info!("loaded {} mapping rule(s)", rules.len());

    // 4. Map Mule AST → Camel IR.
    let llm_provider = match &cfg.llm {
        Some(c) => Some(llm::build_provider(c).await?),
        None => None,
    };
    let (camel_ir, decisions) =
        mapper::map_project(&normalized, &rules, llm_provider.as_deref()).await?;

    // 4b. Extract and convert DataWeave expressions.
    let dw_expressions = dataweave::converter::extract_dataweave_expressions(&normalized.flows);
    let mut camel_ir = camel_ir; // make mutable for bean injection
    if !dw_expressions.is_empty() {
        tracing::info!(
            "found {} DataWeave expression(s) to convert",
            dw_expressions.len()
        );
        let beans = if let Some(ref llm) = llm_provider {
            dataweave::converter::convert_with_llm(&dw_expressions, llm.as_ref()).await
        } else {
            tracing::warn!("no LLM configured — generating stub beans for DataWeave");
            dataweave::converter::convert_without_llm(&dw_expressions)
        };
        camel_ir.beans.extend(beans);
    }

    // 5. Emit code, config, and K8s/Kong artifacts.
    emitter::emit_project(&camel_ir, &cfg.output_dir)?;
    // 5b. Copy custom Java classes from Mule project
    let copied_java =
        emitter::java_copier::copy_custom_java(&acquired.working_dir, &cfg.output_dir)?;
    if !copied_java.is_empty() {
        tracing::info!(
            "copied {} custom Java class(es) from Mule project",
            copied_java.len()
        );
    }

    // 5c. Copy API specifications (RAML, OpenAPI)
    let copied_specs = emitter::api_copier::copy_api_specs(&acquired.working_dir, &cfg.output_dir)?;
    if !copied_specs.is_empty() {
        tracing::info!("copied {} API spec file(s)", copied_specs.len());
    }

    // 5d. Scaffold JUnit tests from MUnit
    let munit_tests =
        emitter::munit_scaffold::scaffold_tests(&acquired.working_dir, &cfg.output_dir)?;
    if !munit_tests.is_empty() {
        tracing::info!("scaffolded {} JUnit test(s) from MUnit", munit_tests.len());
    }

    if cfg.emit_k8s {
        emitter::emit_k8s_manifests(&camel_ir, &cfg.output_dir)?;
    }
    if cfg.emit_kong_config {
        emitter::emit_kong_config(&camel_ir, &cfg.output_dir)?;
    }

    // 6. Build report (used both for MIGRATION_REPORT.md and as docgen input).
    let report = report::build(&decisions, &acquired);
    report::write(&report, &cfg.output_dir.join("MIGRATION_REPORT.md"))?;

    // 7. Docgen — README, CONTRIBUTING, per-flow docs, runbooks, etc.
    docgen::generate(
        &camel_ir,
        &report,
        &cfg.output_dir,
        &cfg.docgen,
        llm_provider.as_deref(),
    )
    .await?;

    // 7b. Generate developer-friendly files.
    emitter::checklist::generate(&camel_ir, &report, &cfg.output_dir)?;
    emitter::env_generator::generate(&camel_ir, &cfg.output_dir)?;
    emitter::makefile_generator::generate(&camel_ir, &cfg.output_dir)?;
    tracing::info!("generated CHECKLIST.md, .env.example, Makefile");

    // 8. Git emit — init the output repo and commit.
    git::emit::emit(&cfg.output_dir, &cfg.git)?;

    tracing::info!("migration complete");
    Ok(report)
}

/// Convenience: check whether a directory looks like a Mule 4 project.
pub fn is_mule_project(dir: &Path) -> bool {
    dir.join("src/main/mule").is_dir() || dir.join("mule-artifact.json").exists()
}

fn ensure_output_dir_usable(path: &Path, force: bool) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        return Err(MuleForgeError::Emit(format!(
            "output path exists and is not a directory: {}",
            path.display()
        )));
    }
    let mut entries = std::fs::read_dir(path)?;
    if entries.next().is_some() && !force {
        return Err(MuleForgeError::Emit(format!(
            "output directory {} is not empty (use --force to overwrite)",
            path.display()
        )));
    }
    Ok(())
}
