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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_patch_success() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "hello world\nthis is a test").unwrap();
        let path = file.path().to_str().unwrap().to_string();

        let input = serde_json::json!({
            "path": path,
            "old": "hello world",
            "new": "hi universe"
        })
        .to_string();

        let result = PatchFile.execute(&input).unwrap();
        assert!(result.contains("Patched"));

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("hi universe"));
        assert!(content.contains("this is a test"));
    }

    #[test]
    fn test_patch_file_not_found() {
        let input = serde_json::json!({
            "path": "non_existent_file_xyz.txt",
            "old": "anything",
            "new": "something"
        })
        .to_string();

        let result = PatchFile.execute(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("file does not exist")
        );
    }

    #[test]
    fn test_patch_target_not_found() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "existing content").unwrap();
        let path = file.path().to_str().unwrap().to_string();

        let input = serde_json::json!({
            "path": path,
            "old": "missing content",
            "new": "something"
        })
        .to_string();

        let result = PatchFile.execute(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("old text was not found")
        );
    }

    #[test]
    fn test_patch_ambiguous_matches() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "match\nmatch\nmatch").unwrap();
        let path = file.path().to_str().unwrap().to_string();

        let input = serde_json::json!({
            "path": path,
            "old": "match",
            "new": "replaced"
        })
        .to_string();

        let result = PatchFile.execute(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("matched 3 times"));
    }

    #[test]
    fn test_patch_empty_inputs() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "content").unwrap();
        let path = file.path().to_str().unwrap().to_string();

        let input_empty_old = serde_json::json!({
            "path": path,
            "old": "",
            "new": "something"
        })
        .to_string();
        assert!(PatchFile.execute(&input_empty_old).is_err());

        let input_empty_path = serde_json::json!({
            "path": "",
            "old": "something",
            "new": "something"
        })
        .to_string();
        assert!(PatchFile.execute(&input_empty_path).is_err());
    }
}
