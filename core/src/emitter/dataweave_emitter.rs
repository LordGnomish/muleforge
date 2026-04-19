//! DataWeave emitter: generates Java bean classes for complex transformations
//! that were originally DataWeave expressions.

use std::path::Path;

use crate::ast::camel_ir::CamelProject;
use crate::Result;

pub fn emit(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    if ir.beans.is_empty() {
        return Ok(());
    }

    let beans_dir = output_dir.join("src/main/java/generated/beans");
    std::fs::create_dir_all(&beans_dir)?;

    for bean in &ir.beans {
        let file_path = beans_dir.join(format!("{}.java", bean.class_name));
        let mut java = String::new();
        java.push_str(&format!("package {};\n\n", bean.package));
        if let Some(ref dw) = bean.origin_dw {
            java.push_str(&format!(
                "// Original DataWeave:\n// {}\n\n",
                dw.lines().collect::<Vec<_>>().join("\n// ")
            ));
        }
        java.push_str(&bean.java_source);
        std::fs::write(&file_path, java)?;
    }

    Ok(())
}
