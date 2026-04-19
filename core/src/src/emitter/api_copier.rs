//! API specification copier: detects RAML or OpenAPI specs in the Mule
//! project and copies them to the output project.

use crate::Result;
use std::path::Path;

/// Copy API specifications (RAML, OAS/Swagger) from Mule project to output.
pub fn copy_api_specs(input_dir: &Path, output_dir: &Path) -> Result<Vec<String>> {
    let mut copied = Vec::new();

    // Check common locations for API specs
    let search_dirs = [
        input_dir.join("src/main/resources/api"),
        input_dir.join("src/main/api"),
        input_dir.join("api"),
    ];

    let api_out = output_dir.join("src/main/resources/api");

    for dir in &search_dirs {
        if dir.is_dir() {
            std::fs::create_dir_all(&api_out)?;
            copy_specs_recursive(dir, &api_out, &mut copied)?;
        }
    }

    // Also check root for standalone spec files
    for entry in std::fs::read_dir(input_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "raml" | "yaml" | "yml" | "json") {
                    // Check if it looks like an API spec
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if is_api_spec(&content) {
                            std::fs::create_dir_all(&api_out)?;
                            let dst = api_out.join(entry.file_name());
                            std::fs::copy(&path, &dst)?;
                            copied.push(path.display().to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(copied)
}

fn copy_specs_recursive(src: &Path, dst: &Path, copied: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_specs_recursive(&src_path, &dst_path, copied)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
            copied.push(src_path.display().to_string());
        }
    }
    Ok(())
}

fn is_api_spec(content: &str) -> bool {
    // RAML
    if content.starts_with("#%RAML") {
        return true;
    }
    // OpenAPI / Swagger
    if content.contains("\"openapi\"")
        || content.contains("openapi:")
        || content.contains("\"swagger\"")
        || content.contains("swagger:")
    {
        return true;
    }
    false
}
