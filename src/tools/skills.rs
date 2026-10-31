//! Agent Skills discovery and progressive loading.
//!
//! Skills follow the Agent Skills convention: a directory containing a
//! `SKILL.md` file with YAML frontmatter (`name` and `description`).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use super::Tool;

const MAX_SKILL_BYTES: u64 = 256 * 1024;
const MAX_SKILLS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Skill {
    name: String,
    description: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct LoadInput {
    name: String,
}

pub struct ListSkills;
pub struct LoadSkill;

impl Tool for ListSkills {
    fn name(&self) -> &str {
        "list_skills"
    }

    fn description(&self) -> &str {
        "Discover available Agent Skills and show their names, descriptions, and SKILL.md paths. Input: {}. Workspace skills override user skills with the same name."
    }

    fn execute(&self, _input: &str) -> Result<String> {
        let skills = discover_skills(&workspace_dir()?, &user_skill_dirs());
        if skills.is_empty() {
            return Ok("No skills found. Add a SKILL.md directory under .agents/skills, .luma/skills, ~/.agents/skills, or ~/.config/luma/skills.".into());
        }

        Ok(skills
            .iter()
            .map(|skill| {
                format!(
                    "{} — {}\n  {}",
                    skill.name,
                    skill.description,
                    skill.path.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

impl Tool for LoadSkill {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load the full instructions for one discovered Agent Skill. Input JSON: {\"name\":\"skill-name\"}. Load a relevant skill before following its workflow; the skill's files remain in its directory."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: LoadInput =
            serde_json::from_str(input).context("load_skill: invalid JSON input")?;
        validate_name(&input.name)?;
        let skills = discover_skills(&workspace_dir()?, &user_skill_dirs());
        let skill = skills
            .iter()
            .find(|skill| skill.name == input.name)
            .ok_or_else(|| {
                anyhow!(
                    "Skill '{}' was not found. Run list_skills to see available skills.",
                    input.name
                )
            })?;

        let metadata = fs::metadata(&skill.path)?;
        if metadata.len() > MAX_SKILL_BYTES {
            bail!(
                "Skill '{}' exceeds the {} KB size limit",
                skill.name,
                MAX_SKILL_BYTES / 1024
            );
        }
        let content = fs::read_to_string(&skill.path)
            .with_context(|| format!("failed to read skill {}", skill.path.display()))?;
        let (_, body) = split_frontmatter(&content)?;

        Ok(format!(
            "# Skill: {}\n\nDescription: {}\nSkill directory: {}\n\n{}",
            skill.name,
            skill.description,
            skill.path.parent().unwrap_or(Path::new(".")).display(),
            body.trim()
        ))
    }
}

fn workspace_dir() -> Result<PathBuf> {
    std::env::current_dir().context("could not determine workspace directory")
}

fn user_skill_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".agents/skills"));
    }
    dirs.push(crate::config::config_dir().join("skills"));
    dirs
}

fn discover_skills(workspace: &Path, user_dirs: &[PathBuf]) -> Vec<Skill> {
    // Scan workspace skills first so project instructions intentionally
    // override personal skills with the same name.
    let mut skills = BTreeMap::new();
    let workspace_dirs = [
        workspace.join(".agents/skills"),
        workspace.join(".luma/skills"),
    ];
    for root in workspace_dirs.iter().chain(user_dirs.iter()) {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let Ok(canonical_root) = fs::canonicalize(root) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if validate_name(&name).is_err() {
                continue;
            }
            let skill_path = entry.path().join("SKILL.md");
            let Ok(skill_type) = fs::symlink_metadata(&skill_path) else {
                continue;
            };
            if !skill_type.file_type().is_file() || skill_type.len() > MAX_SKILL_BYTES {
                continue;
            }
            let Ok(canonical_skill) = fs::canonicalize(&skill_path) else {
                continue;
            };
            if !canonical_skill.starts_with(&canonical_root) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&canonical_skill) else {
                continue;
            };
            let Ok((frontmatter, _)) = split_frontmatter(&content) else {
                continue;
            };
            let Some((declared_name, description)) = parse_metadata(frontmatter) else {
                continue;
            };
            if declared_name != name || description.trim().is_empty() || description.len() > 1024 {
                continue;
            }
            skills.entry(name.clone()).or_insert_with(|| Skill {
                name,
                description,
                path: canonical_skill,
            });
            if skills.len() >= MAX_SKILLS {
                break;
            }
        }
        if skills.len() >= MAX_SKILLS {
            break;
        }
    }
    skills.into_values().collect()
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
    {
        bail!("skill name must be 1-64 lowercase letters, digits, or single hyphens");
    }
    Ok(())
}

fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let content = content
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow!("SKILL.md must begin with YAML frontmatter (---)"))?;
    let end = content
        .find("\n---\n")
        .ok_or_else(|| anyhow!("SKILL.md frontmatter is missing its closing ---"))?;
    Ok((&content[..end], &content[end + 5..]))
}

fn parse_metadata(frontmatter: &str) -> Option<(String, String)> {
    let mut name = None;
    let mut description = None;
    let mut lines = frontmatter.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']);
        match key.trim() {
            "name" => name = Some(value.to_string()),
            "description" => {
                if value == ">" || value == "|" || value == ">-" || value == "|-" {
                    let mut parts = Vec::new();
                    while let Some(next) = lines.peek() {
                        if next.trim().is_empty() {
                            lines.next();
                            continue;
                        }
                        if !next.starts_with(' ') && !next.starts_with('\t') {
                            break;
                        }
                        parts.push(lines.next()?.trim());
                    }
                    description = Some(parts.join(" "));
                } else {
                    description = Some(value.to_string());
                }
            }
            _ => {}
        }
    }
    Some((name?, description?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(
        root: &Path,
        directory: &str,
        declared_name: &str,
        description: &str,
        body: &str,
    ) {
        let dir = root.join(directory);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {declared_name}\ndescription: {description}\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    #[test]
    fn discovers_workspace_and_user_skills_with_workspace_precedence() {
        let workspace = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let workspace_skills = workspace.path().join(".agents/skills");
        write_skill(
            user.path(),
            "review",
            "review",
            "Personal review",
            "Personal",
        );
        write_skill(user.path(), "docs", "docs", "Documentation", "Docs");
        write_skill(
            &workspace_skills,
            "review",
            "review",
            "Project review",
            "Project",
        );

        let skills = discover_skills(workspace.path(), &[user.path().to_path_buf()]);
        assert_eq!(skills.len(), 2);
        let review = skills.iter().find(|skill| skill.name == "review").unwrap();
        assert!(
            review
                .path
                .starts_with(fs::canonicalize(workspace.path()).unwrap())
        );
    }

    #[test]
    fn rejects_mismatched_directory_and_frontmatter_names() {
        let workspace = tempfile::tempdir().unwrap();
        write_skill(workspace.path(), "actual", "different", "Mismatch", "Body");
        assert!(discover_skills(workspace.path(), &[]).is_empty());
    }

    #[test]
    fn accepts_skills_up_to_256_kib_and_rejects_larger_files() {
        let workspace = tempfile::tempdir().unwrap();
        let skills_root = workspace.path().join(".agents/skills");
        let skill_dir = skills_root.join("size-limit");
        fs::create_dir_all(&skill_dir).unwrap();

        let header = "---\nname: size-limit\ndescription: Size boundary test\n---\n\n";
        let body_len = MAX_SKILL_BYTES as usize - header.len();
        let content = format!("{header}{}", "x".repeat(body_len));
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, &content).unwrap();
        assert_eq!(fs::metadata(&skill_path).unwrap().len(), MAX_SKILL_BYTES);
        assert_eq!(discover_skills(workspace.path(), &[]).len(), 1);

        fs::write(&skill_path, format!("{content}x")).unwrap();
        assert_eq!(
            fs::metadata(&skill_path).unwrap().len(),
            MAX_SKILL_BYTES + 1
        );
        assert!(discover_skills(workspace.path(), &[]).is_empty());
    }

    #[test]
    fn validates_skill_names() {
        assert!(validate_name("rust-testing").is_ok());
        assert!(validate_name("../secret").is_err());
        assert!(validate_name("Bad_Name").is_err());
        assert!(validate_name("bad--name").is_err());
    }

    #[test]
    fn splits_frontmatter_and_body() {
        let (frontmatter, body) =
            split_frontmatter("---\nname: test\ndescription: Test\n---\n\nDo this.").unwrap();
        assert!(frontmatter.contains("name: test"));
        assert_eq!(body, "\nDo this.");
    }

    #[test]
    fn parses_folded_multiline_description() {
        let metadata = parse_metadata(
            "name: rust-testing\ndescription: >\n  Run focused tests.\n  Explain failures clearly.",
        )
        .unwrap();
        assert_eq!(metadata.0, "rust-testing");
        assert_eq!(metadata.1, "Run focused tests. Explain failures clearly.");
    }
}
