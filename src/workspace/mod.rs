use anyhow::Result;
use std::path::Path;

use crate::tools::ToolRegistry;

pub mod language;

pub async fn gather_project_context(tools: &ToolRegistry) -> Result<String> {
    let mut context = String::new();

    // Get files
    let files = tools.execute("list_directory", ".")?;

    context.push_str("Project files:\n");

    context.push_str(&files);

    // Try README
    for file in ["README.md", "readme.md", "Cargo.toml", "package.json"] {
        if Path::new(file).exists() {
            if let Ok(content) = tools.execute("read_file", file) {
                context.push_str(&format!("\n\n{}:\n{}", file, content));
            }
        }
    }

    Ok(context)
}
