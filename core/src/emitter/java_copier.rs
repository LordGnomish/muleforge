//! Java source copier: detects custom Java classes in the Mule project
//! and copies them to the output Quarkus project with package adjustments.

use crate::Result;
use std::path::Path;

/// Copy custom Java source files from the Mule project to the output.
/// Adjusts package declarations if needed and adds TODO markers.
pub fn copy_custom_java(input_dir: &Path, output_dir: &Path) -> Result<Vec<String>> {
    let mule_java = input_dir.join("src/main/java");
    if !mule_java.is_dir() {
        return Ok(vec![]);
    }

    let out_java = output_dir.join("src/main/java");
    std::fs::create_dir_all(&out_java)?;

    let mut copied = Vec::new();
    copy_dir_recursive(&mule_java, &out_java, &mut copied)?;
    Ok(copied)
}

fn copy_dir_recursive(src: &Path, dst: &Path, copied: &mut Vec<String>) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, copied)?;
        } else if src_path.extension().and_then(|e| e.to_str()) == Some("java") {
            let content = std::fs::read_to_string(&src_path)?;

            // Add a migration header comment
            let migrated = format!(
                "// MuleForge: copied from original Mule project.\n\
                 // TODO: Review for Mule-specific imports and replace with Camel equivalents.\n\
                 // Original file: {}\n\n{}",
                src_path.display(),
                adjust_imports(&content)
            );

            std::fs::write(&dst_path, migrated)?;
            copied.push(src_path.display().to_string());
        }
    }
    Ok(())
}

/// Replace common Mule-specific imports with Camel equivalents or TODO markers.
fn adjust_imports(java: &str) -> String {
    java.replace(
        "import org.mule.runtime.api.message.Message;",
        "// TODO: Replace Mule Message with Camel Exchange\nimport org.apache.camel.Exchange;"
    )
    .replace(
        "import org.mule.runtime.extension.api.annotation",
        "// TODO: Replace Mule extension annotations\n// import org.mule.runtime.extension.api.annotation"
    )
    .replace(
        "import org.mule.runtime.api.metadata.TypedValue;",
        "// TODO: Replace Mule TypedValue\n// import org.mule.runtime.api.metadata.TypedValue;"
    )
    .replace(
        "import org.mule.runtime",
        "// TODO: Replace Mule runtime import\n// import org.mule.runtime"
    )
}
