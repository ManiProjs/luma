use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::fs;

use crate::tools::Tool;

#[derive(Debug, Deserialize)]
struct PatchFileInput {
    path: String,
    old: String,
    new: String,
}

pub struct PatchFile;

impl Tool for PatchFile {
    fn name(&self) -> &str {
        "patch_file"
    }

    fn description(&self) -> &str {
        "Apply an exact text replacement to an existing file. \
         The old text must match exactly once."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: PatchFileInput =
            serde_json::from_str(input).context("invalid patch_file JSON")?;

        if input.path.trim().is_empty() {
            bail!("patch_file: path cannot be empty");
        }

        if input.old.is_empty() {
            bail!("patch_file: old text cannot be empty");
        }

        if input.old == input.new {
            bail!("patch_file: old and new text are identical");
        }

        let path = std::path::Path::new(&input.path);

        if !path.exists() {
            bail!("patch_file: file does not exist: {}", input.path);
        }

        if !path.is_file() {
            bail!("patch_file: path is not a file: {}", input.path);
        }

        let content =
            fs::read_to_string(path).with_context(|| format!("failed to read {}", input.path))?;

        let matches = content.matches(&input.old).count();

        match matches {
            0 => {
                bail!("patch_file: old text was not found in {}", input.path);
            }

            1 => {}

            count => {
                bail!(
                    "patch_file: old text matched {} times in {}. \
                     Make the patch more specific.",
                    count,
                    input.path
                );
            }
        }

        let patched = content.replacen(&input.old, &input.new, 1);

        fs::write(path, patched).with_context(|| format!("failed to write {}", input.path))?;

        Ok(format!("Patched {} successfully.", input.path))
    }
}
