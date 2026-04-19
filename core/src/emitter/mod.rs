//! Emitters: Camel IR → files on disk.
//!
//! Emitters are kept as separate sibling modules so future target platforms
//! (Spring Boot, plain Java EE, etc.) can be added without touching existing
//! ones.

use std::path::Path;

use crate::ast::camel_ir::CamelProject;
use crate::Result;

pub mod api_copier;
pub mod checklist;
pub mod ci_emitter;
pub mod config_emitter;
pub mod dataweave_emitter;
pub mod env_generator;
pub mod java_copier;
pub mod k8s_emitter;
pub mod kong_emitter;
pub mod makefile_generator;
pub mod munit_scaffold;
pub mod routes_emitter;
pub mod tests_emitter;

/// Emit the core Quarkus project (routes, beans, pom.xml, application.properties,
/// Dockerfile, tests, CI workflows).
pub fn emit_project(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;
    routes_emitter::emit(ir, output_dir)?;
    dataweave_emitter::emit(ir, output_dir)?;
    config_emitter::emit(ir, output_dir)?;
    tests_emitter::emit(ir, output_dir)?;
    ci_emitter::emit(ir, output_dir)?;
    Ok(())
}

pub fn emit_k8s_manifests(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    k8s_emitter::emit(ir, output_dir)
}

pub fn emit_kong_config(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    kong_emitter::emit(ir, output_dir)
}
