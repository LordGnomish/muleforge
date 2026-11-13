//! Golden-file tests: every `examples/<name>/` that ships both an `input/`
//! Mule project and an `expected-output/` Camel project is migrated end to
//! end (parse -> normalize -> map -> emit), and **every** file checked in
//! under `expected-output/` is compared with the emitted file at the same
//! relative path: the Java routes and the project files (`pom.xml`, ...).
//!
//! Normalisation — the only differences the comparison ignores:
//!
//! * whitespace at the end of a line (which also covers CRLF line endings);
//! * blank lines, or a missing final newline, at the end of the file.
//!
//! Everything else must match exactly: indentation, blank lines inside the
//! file, comments, element order.
//!
//! The suite never touches the network. That the emitted `pom.xml` really
//! builds (`mvn package`) is verified by hand when the expectation changes;
//! this test then keeps the emitter from drifting away from the verified pom.

use muleforge_core::{emitter, mapper, parser};
use pretty_assertions::StrComparison;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples")
}

/// Example directories that have both `input/` and `expected-output/`.
fn golden_cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples/ exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("input").is_dir() && p.join("expected-output").is_dir())
        .collect();
    cases.sort();
    cases
}

fn case_name(case: &Path) -> String {
    case.file_name().unwrap().to_string_lossy().to_string()
}

/// The parser expects a Mule project layout (`src/main/mule/*.xml`), so copy
/// the example's flat `input/*.xml` files into one.
///
/// The Mule project directory is named after the example, because the parser
/// infers the project name from it and that name ends up in emitted files
/// (the `artifactId` of `pom.xml`). Returns the temp dir guard and the
/// project directory inside it.
fn stage_input(case: &Path) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join(case_name(case));
    let mule_dir = project_dir.join("src/main/mule");
    std::fs::create_dir_all(&mule_dir).unwrap();
    for entry in std::fs::read_dir(case.join("input")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "xml") {
            std::fs::copy(&path, mule_dir.join(path.file_name().unwrap())).unwrap();
        }
    }
    (dir, project_dir)
}

/// Migrate one example into a fresh temp dir and return it.
async fn emit_case(case: &Path, rules: &[mapper::Rule]) -> TempDir {
    let (_input_guard, project_dir) = stage_input(case);
    let out = TempDir::new().unwrap();

    let project = parser::parse_project(&project_dir).expect("parse");
    let normalized = parser::normalize(project).expect("normalize");
    let (ir, _) = mapper::map_project(&normalized, rules, None)
        .await
        .expect("map");
    emitter::emit_project(&ir, out.path()).expect("emit");
    out
}

fn load_rules() -> Vec<mapper::Rule> {
    mapper::load_rules(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../mappings"))
        .expect("load mapping rules")
}

/// Every file below `root`, as sorted paths relative to `root`.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

fn is_java(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "java")
}

/// See the module docs: trailing whitespace per line and blank lines at the
/// end of the file are ignored, nothing else.
fn normalize_text(s: &str) -> String {
    s.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// Compare the given expected files (relative to `expected_root`) with the
/// emitted files under `emitted_root`; one message per mismatch.
fn compare_files(
    name: &str,
    expected_root: &Path,
    emitted_root: &Path,
    files: &[PathBuf],
) -> Vec<String> {
    let mut failures = Vec::new();
    for rel in files {
        let want = std::fs::read_to_string(expected_root.join(rel)).unwrap();
        match std::fs::read_to_string(emitted_root.join(rel)) {
            Ok(got) => {
                let (want, got) = (normalize_text(&want), normalize_text(&got));
                if want != got {
                    failures.push(format!(
                        "{name}: {} differs (`<` expected-output, `>` emitted)\n{}",
                        rel.display(),
                        StrComparison::new(&want, &got)
                    ));
                }
            }
            Err(_) => failures.push(format!(
                "{name}: {} not generated; emitted: {:?}",
                rel.display(),
                files_under(emitted_root)
            )),
        }
    }
    failures
}

#[test]
fn golden_cases_exist() {
    assert!(
        !golden_cases().is_empty(),
        "expected at least one examples/<name>/{{input,expected-output}} pair"
    );
}

#[tokio::test]
async fn golden_java_routes_match_expected() {
    let rules = load_rules();
    let mut failures = Vec::new();

    for case in golden_cases() {
        let name = case_name(&case);
        let out = emit_case(&case, &rules).await;

        let expected_root = case.join("expected-output");
        let expected: Vec<PathBuf> = files_under(&expected_root)
            .into_iter()
            .filter(|p| is_java(p))
            .collect();
        assert!(!expected.is_empty(), "{name}: no expected .java files");

        failures.extend(compare_files(&name, &expected_root, out.path(), &expected));
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Everything under `expected-output/` that is not a Java source: the
/// `pom.xml` and any other project file an example pins down.
#[tokio::test]
async fn golden_project_files_match_expected() {
    let rules = load_rules();
    let mut failures = Vec::new();
    let mut poms_compared = 0;

    for case in golden_cases() {
        let name = case_name(&case);
        let out = emit_case(&case, &rules).await;

        let expected_root = case.join("expected-output");
        let expected: Vec<PathBuf> = files_under(&expected_root)
            .into_iter()
            .filter(|p| !is_java(p))
            .collect();
        poms_compared += expected
            .iter()
            .filter(|p| p.as_path() == Path::new("pom.xml"))
            .count();

        failures.extend(compare_files(&name, &expected_root, out.path(), &expected));
    }

    assert!(
        poms_compared > 0,
        "no example ships expected-output/pom.xml, so nothing guards the emitted pom"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
