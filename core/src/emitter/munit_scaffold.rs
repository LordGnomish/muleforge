//! MUnit test scaffold generator.
//!
//! Reads MUnit XML test files from the Mule project and generates
//! JUnit 5 test skeletons in the output project.

use crate::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::path::Path;

/// Detect MUnit test files and generate JUnit scaffolds.
pub fn scaffold_tests(input_dir: &Path, output_dir: &Path) -> Result<Vec<String>> {
    let munit_dir = input_dir.join("src/test/munit");
    if !munit_dir.is_dir() {
        return Ok(vec![]);
    }

    let test_out = output_dir.join("src/test/java/generated/munit");
    std::fs::create_dir_all(&test_out)?;

    let mut generated = Vec::new();

    for entry in std::fs::read_dir(&munit_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("xml") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let test_names = extract_munit_test_names(&content);
                if !test_names.is_empty() {
                    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("test");
                    let class_name = to_class_name(file_stem);
                    let java = generate_junit_scaffold(&class_name, &test_names, file_stem);
                    let out_path = test_out.join(format!("{}.java", class_name));
                    std::fs::write(&out_path, java)?;
                    generated.push(format!("{} -> {}", path.display(), out_path.display()));
                }
            }
        }
    }

    Ok(generated)
}

fn extract_munit_test_names(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.contains("test") || name.ends_with(":test") {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        if key == "name" {
                            if let Ok(val) = attr.unescape_value() {
                                names.push(val.to_string());
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    names
}

fn generate_junit_scaffold(class_name: &str, test_names: &[String], original_file: &str) -> String {
    let mut java = String::new();
    java.push_str("package generated.munit;\n\n");
    java.push_str("import io.quarkus.test.junit.QuarkusTest;\n");
    java.push_str("import org.junit.jupiter.api.Test;\n");
    java.push_str("import org.junit.jupiter.api.DisplayName;\n");
    java.push_str("import static org.junit.jupiter.api.Assertions.*;\n\n");
    java.push_str(&format!(
        "/**\n * Scaffolded from MUnit test: {}\n",
        original_file
    ));
    java.push_str(
        " * TODO: Implement test logic — MUnit assertions need manual conversion.\n */\n",
    );
    java.push_str("@QuarkusTest\n");
    java.push_str(&format!("public class {} {{\n\n", class_name));

    for name in test_names {
        let method = to_method_name(name);
        java.push_str("    @Test\n");
        java.push_str(&format!("    @DisplayName(\"{}\")\n", name));
        java.push_str(&format!("    public void {}() {{\n", method));
        java.push_str(&format!(
            "        // TODO: Port MUnit test '{}' to JUnit 5\n",
            name
        ));
        java.push_str("        // Original MUnit test logic needs manual conversion.\n");
        java.push_str("        // Steps:\n");
        java.push_str("        // 1. Set up test input (payload, variables, headers)\n");
        java.push_str("        // 2. Invoke the route via ProducerTemplate or REST Assured\n");
        java.push_str("        // 3. Assert the expected output\n");
        java.push_str("        fail(\"Not yet implemented — port from MUnit\");\n");
        java.push_str("    }\n\n");
    }

    java.push_str("}\n");
    java
}

fn to_class_name(s: &str) -> String {
    let mut result = String::new();
    let mut cap = true;
    for c in s.chars() {
        if c == '-' || c == '_' || c == ' ' || c == '.' {
            cap = true;
        } else if cap {
            result.push(c.to_uppercase().next().unwrap_or(c));
            cap = false;
        } else {
            result.push(c);
        }
    }
    format!("{}Test", result)
}

fn to_method_name(s: &str) -> String {
    let mut result = String::from("test");
    let mut cap = true;
    for c in s.chars() {
        if c == '-' || c == '_' || c == ' ' || c == '.' || c == ':' {
            cap = true;
        } else if cap {
            result.push(c.to_uppercase().next().unwrap_or(c));
            cap = false;
        } else {
            result.push(c);
        }
    }
    result
}
