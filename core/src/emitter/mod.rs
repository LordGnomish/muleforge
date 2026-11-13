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
pub mod docker_compose;
pub mod env_generator;
pub mod java_copier;
pub mod k8s_emitter;
pub mod kong_emitter;
pub mod makefile_generator;
pub mod munit_scaffold;
pub mod route_diagram;
pub mod routes_emitter;
pub mod tests_emitter;

/// Quarkus platform release the emitted project is built on.
///
/// The emitted `pom.xml` imports `io.quarkus.platform:quarkus-bom` and
/// `io.quarkus.platform:quarkus-camel-bom` and runs
/// `io.quarkus.platform:quarkus-maven-plugin`, all three at this one
/// version, so it has to be a version that Maven Central has for each of
/// them (not every Quarkus version is a platform release: there is no
/// platform 3.8.0, for example). The platform's Camel BOM decides the Camel
/// Quarkus version; nothing else pins it.
///
/// When changing it, rebuild an emitted project (`mvn package`) before
/// updating `examples/*/expected-output/pom.xml`.
pub const QUARKUS_PLATFORM_VERSION: &str = "3.33.1";

/// Java release the emitted project is compiled for (`maven.compiler.release`).
/// The platform above needs Java 17 or newer.
pub const JAVA_RELEASE: &str = "21";

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
