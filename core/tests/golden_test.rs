//! Golden-file tests: every `examples/<name>/` that ships both an `input/`
//! Mule project and an `expected-output/` Camel project is migrated end to
//! end, and each generated Java route is compared byte-for-byte (modulo
//! trailing whitespace) with the checked-in expectation.

use muleforge_core::{emitter, mapper, parser};
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

/// The parser expects a Mule project layout (`src/main/mule/*.xml`), so copy
/// the example's flat `input/*.xml` files into one.
fn stage_input(case: &Path) -> TempDir {
    let dir = TempDir::new().unwrap();
    let mule_dir = dir.path().join("src/main/mule");
    std::fs::create_dir_all(&mule_dir).unwrap();
    for entry in std::fs::read_dir(case.join("input")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "xml") {
            std::fs::copy(&path, mule_dir.join(path.file_name().unwrap())).unwrap();
        }
    }
    dir
}

fn java_files(root: &Path) -> Vec<PathBuf> {
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
            } else if path.extension().is_some_and(|e| e == "java") {
                out.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

fn normalize_text(s: &str) -> String {
    s.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
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
    let rules = mapper::load_rules(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../mappings"))
        .expect("load mapping rules");
    let mut failures = Vec::new();

    for case in golden_cases() {
        let name = case.file_name().unwrap().to_string_lossy().to_string();
        let input = stage_input(&case);
        let out = TempDir::new().unwrap();

        let project = parser::parse_project(input.path()).expect("parse");
        let normalized = parser::normalize(project).expect("normalize");
        let (ir, _) = mapper::map_project(&normalized, &rules, None)
            .await
            .expect("map");
        emitter::emit_project(&ir, out.path()).expect("emit");

        let expected_root = case.join("expected-output");
        let expected = java_files(&expected_root);
        assert!(!expected.is_empty(), "{name}: no expected .java files");

        for rel in expected {
            let want = std::fs::read_to_string(expected_root.join(&rel)).unwrap();
            match std::fs::read_to_string(out.path().join(&rel)) {
                Ok(got) if normalize_text(&got) == normalize_text(&want) => {}
                Ok(got) => failures.push(format!(
                    "{name}: {} differs\n--- expected\n{want}\n--- got\n{got}",
                    rel.display()
                )),
                Err(_) => failures.push(format!(
                    "{name}: {} not generated; emitted: {:?}",
                    rel.display(),
                    java_files(out.path())
                )),
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
