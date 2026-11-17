//! Lightweight structural navigation built on ripgrep.

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::Tool;

const MAX_RESULTS: usize = 100;
const MAX_BYTES: usize = 12_000;
const MAX_PROJECT_FILES: usize = 250;
const MAX_SOURCE_BYTES: usize = 128 * 1024;

#[derive(Debug, Deserialize)]
struct SearchInput {
    #[serde(default)]
    query: String,
    path: Option<String>,
    language: Option<String>,
}

#[derive(Default)]
pub struct FindSymbols {
    workspace: Option<PathBuf>,
}

#[derive(Default)]
pub struct FindReferences {
    workspace: Option<PathBuf>,
}

#[derive(Default)]
pub struct AnalyzeCodebase {
    workspace: Option<PathBuf>,
}

impl FindSymbols {
    pub fn scoped(workspace: PathBuf) -> Self {
        Self {
            workspace: Some(workspace),
        }
    }
}

impl FindReferences {
    pub fn scoped(workspace: PathBuf) -> Self {
        Self {
            workspace: Some(workspace),
        }
    }
}

impl AnalyzeCodebase {
    pub fn scoped(workspace: PathBuf) -> Self {
        Self {
            workspace: Some(workspace),
        }
    }
}

#[derive(Debug)]
struct SourceFile {
    path: PathBuf,
    imports: Vec<String>,
    symbols: Vec<String>,
    content: String,
}

impl Tool for AnalyzeCodebase {
    fn name(&self) -> &str {
        "analyze_codebase"
    }

    fn description(&self) -> &str {
        "Build a bounded structural overview of a project: source modules, imports, declared symbols, and cross-file references. Supports Rust, Python, JS/TS, Go, Swift, Java, C/C++, and Dart. JSON input: {\"path\":\"optional project root\",\"query\":\"optional symbol to trace\"}. Read-only."
    }

    fn model_guide(&self) -> &str {
        "Use this early when entering an unfamiliar repository or explaining architecture. It summarizes module boundaries and likely symbol relationships; treat regex-based relationships as navigation clues and inspect the actual files before making changes."
    }

    fn execute(&self, input: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Input {
            path: Option<String>,
            query: Option<String>,
        }
        let args: Input = serde_json::from_str(input).context("invalid analyze_codebase JSON")?;
        let root = resolve_search_root(args.path.as_deref(), self.workspace.as_deref())?;
        if !root.is_dir() {
            bail!(
                "analyze_codebase: project path is not a directory: {}",
                root.display()
            );
        }
        let files = collect_source_files(&root)?;
        if files.is_empty() {
            return Ok("No supported source files found.".into());
        }

        let mut directories = BTreeMap::<String, usize>::new();
        let mut extensions = BTreeMap::<String, usize>::new();
        for file in &files {
            let parent = file.path.parent().unwrap_or(Path::new("."));
            *directories.entry(parent.display().to_string()).or_default() += 1;
            let extension = file
                .path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("?");
            *extensions.entry(extension.to_owned()).or_default() += 1;
        }

        let declarations: Vec<(usize, String)> = files
            .iter()
            .enumerate()
            .flat_map(|(index, file)| {
                file.symbols
                    .iter()
                    .cloned()
                    .map(move |symbol| (index, symbol))
            })
            .collect();
        let query = args
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let mut output = format!(
            "Project: {}\nSource files analyzed: {}{}\n",
            root.display(),
            files.len(),
            if files.len() == MAX_PROJECT_FILES {
                " (file cap reached)"
            } else {
                ""
            },
        );
        output.push_str("\nLanguages / extensions:\n");
        for (extension, count) in extensions {
            output.push_str(&format!("  .{extension}: {count} files\n"));
        }
        output.push_str("\nModule directories:\n");
        for (directory, count) in directories {
            output.push_str(&format!("  {directory}/ ({count} files)\n"));
        }

        output.push_str("\nModule relationships and declarations:\n");
        let mut matched_query = false;
        for (index, file) in files.iter().enumerate() {
            let imports = &file.imports;
            let symbols: Vec<_> = file
                .symbols
                .iter()
                .filter(|symbol| {
                    query.is_none_or(|query| {
                        symbol
                            .to_ascii_lowercase()
                            .contains(&query.to_ascii_lowercase())
                    })
                })
                .collect();
            let referenced: Vec<_> = declarations
                .iter()
                .filter(|(declaring_file, symbol)| {
                    *declaring_file != index
                        && file.content.contains(symbol.as_str())
                        && query.is_none_or(|query| {
                            symbol
                                .to_ascii_lowercase()
                                .contains(&query.to_ascii_lowercase())
                        })
                })
                .map(|(declaring_file, symbol)| {
                    format!("{symbol} ← {}", files[*declaring_file].path.display())
                })
                .collect();
            if query.is_some() && symbols.is_empty() && referenced.is_empty() {
                continue;
            }
            matched_query = true;
            output.push_str(&format!("\n{}\n", file.path.display()));
            if !imports.is_empty() {
                output.push_str(&format!("  imports: {}\n", imports.join(", ")));
            }
            if !symbols.is_empty() {
                output.push_str(&format!(
                    "  declares: {}\n",
                    symbols
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !referenced.is_empty() {
                output.push_str(&format!("  references: {}\n", referenced.join("; ")));
            }
            if output.len() >= MAX_BYTES {
                break;
            }
        }
        if query.is_some() && !matched_query {
            output.push_str("\nNo matching declarations or cross-file references found.\n");
        }
        if output.len() > MAX_BYTES {
            output.truncate(char_boundary(&output, MAX_BYTES));
            output.push_str("\n… architecture summary truncated");
        } else if files.len() == MAX_PROJECT_FILES {
            output.push_str("\nAnalysis is capped at 250 source files.");
        }
        Ok(output)
    }
    fn is_read_only(&self) -> bool {
        true
    }
}

fn collect_source_files(root: &Path) -> Result<Vec<SourceFile>> {
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_string_lossy().as_ref(),
                ".git"
                    | "target"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | "out"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
            )
        });
    let mut files = Vec::new();
    for entry in walker.build() {
        let entry = entry.context("failed to walk project source files")?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) || !supported_source(entry.path())
        {
            continue;
        }
        if files.len() >= MAX_PROJECT_FILES {
            break;
        }
        let metadata = entry.metadata().ok();
        if metadata.is_some_and(|metadata| metadata.len() as usize > MAX_SOURCE_BYTES) {
            continue;
        }
        let Ok(content) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let (imports, symbols) = extract_structure(entry.path(), &content);
        files.push(SourceFile {
            path: entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_path_buf(),
            imports,
            symbols,
            content,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn supported_source(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "rs" | "py"
            | "pyi"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "go"
            | "swift"
            | "java"
            | "c"
            | "h"
            | "cc"
            | "hh"
            | "cpp"
            | "hpp"
            | "cxx"
            | "hxx"
            | "dart"
    )
}

fn extract_structure(path: &Path, content: &str) -> (Vec<String>, Vec<String>) {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut imports = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    for line in content.lines() {
        let line = line.trim();
        let rust_module = extension == "rs" && line.starts_with("mod ");
        let import = match extension.as_str() {
            "rs" => line
                .strip_prefix("pub use ")
                .or_else(|| line.strip_prefix("use "))
                .or_else(|| line.strip_prefix("mod ")),
            "py" | "pyi" => line
                .strip_prefix("import ")
                .or_else(|| line.strip_prefix("from ")),
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => line
                .strip_prefix("import ")
                .or_else(|| line.strip_prefix("export * from ")),
            "go" => line.strip_prefix("import "),
            "swift" => line.strip_prefix("import "),
            "java" => line.strip_prefix("import "),
            "c" | "h" | "cc" | "hh" | "cpp" | "hpp" | "cxx" | "hxx" => {
                line.strip_prefix("#include ")
            }
            "dart" => line
                .strip_prefix("import ")
                .or_else(|| line.strip_prefix("export ")),
            _ => None,
        };
        if let Some(import) = import {
            let value = import
                .trim()
                .trim_end_matches(';')
                .trim_matches(['\'', '"']);
            if !value.is_empty() {
                let value = if rust_module {
                    format!("mod {value}")
                } else {
                    value.to_owned()
                };
                imports.insert(value.chars().take(100).collect::<String>());
            }
        }
        let keywords: &[&str] = match extension.as_str() {
            "rs" => &[
                "fn ",
                "struct ",
                "enum ",
                "trait ",
                "type ",
                "mod ",
                "const ",
                "static ",
                "macro_rules!",
            ],
            "py" | "pyi" => &["def ", "class "],
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => &[
                "function ",
                "class ",
                "interface ",
                "type ",
                "enum ",
                "const ",
                "let ",
                "var ",
            ],
            "go" => &["func ", "type ", "var ", "const "],
            "swift" => &[
                "func ",
                "class ",
                "struct ",
                "enum ",
                "protocol ",
                "typealias ",
            ],
            "java" => &["class ", "interface ", "enum ", "record "],
            "c" | "h" | "cc" | "hh" | "cpp" | "hpp" | "cxx" | "hxx" => {
                &["struct ", "class ", "enum ", "namespace "]
            }
            "dart" => &[
                "class ",
                "enum ",
                "mixin ",
                "extension ",
                "typedef ",
                "void ",
            ],
            _ => &[],
        };
        let declaration_line = strip_declaration_modifiers(line, extension.as_str());
        for keyword in keywords {
            if let Some(rest) = declaration_line.strip_prefix(keyword) {
                let name: String = rest
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '!')
                    .collect();
                if !name.is_empty()
                    && name
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_alphabetic() || ch == '_')
                {
                    symbols.insert(name);
                }
                break;
            }
        }
    }
    (
        imports.into_iter().take(30).collect(),
        symbols.into_iter().take(80).collect(),
    )
}

fn strip_declaration_modifiers<'a>(mut line: &'a str, extension: &str) -> &'a str {
    let modifiers: &[&str] = match extension {
        "rs" => &["pub(crate) ", "pub(super) ", "pub(self) ", "pub ", "async "],
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => {
            &["export default ", "export ", "default ", "async "]
        }
        "swift" => &[
            "public ",
            "private ",
            "internal ",
            "open ",
            "final ",
            "static ",
        ],
        "java" => &[
            "public ",
            "protected ",
            "private ",
            "static ",
            "final ",
            "abstract ",
        ],
        "go" => &["//"],
        _ => &[],
    };
    loop {
        let Some(modifier) = modifiers
            .iter()
            .find(|modifier| line.starts_with(**modifier))
        else {
            return line;
        };
        line = &line[modifier.len()..];
    }
}

fn char_boundary(value: &str, limit: usize) -> usize {
    let mut end = limit.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    end
}

impl Tool for FindSymbols {
    fn name(&self) -> &str {
        "find_symbols"
    }

    fn description(&self) -> &str {
        "Find function, type, class, trait, enum, and module declarations. Input JSON: {\"query\":\"optional symbol-name regex\",\"path\":\"optional directory\",\"language\":\"optional rust|python|typescript|javascript|go|swift|java|c|cpp\"}."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: SearchInput = serde_json::from_str(input).context("invalid find_symbols JSON")?;
        let query = if args.query.trim().is_empty() {
            ".*"
        } else {
            args.query.trim()
        };
        let language = args.language.as_deref().unwrap_or("").to_ascii_lowercase();
        let declarations = match language.as_str() {
            "rust" | "rs" => {
                r"(?m)^\s*(pub(?:\([^)]*\))?\s+)?(async\s+)?(fn|struct|enum|trait|type|mod|const|static)\s+"
            }
            "python" | "py" => r"(?m)^\s*(async\s+)?(def|class)\s+",
            "typescript" | "ts" | "javascript" | "js" => {
                r"(?m)^\s*(export\s+)?(default\s+)?(async\s+)?(function|class|interface|type|enum|const|let)\s+"
            }
            "go" => r"(?m)^\s*(func|type|var|const)\s+",
            "swift" => {
                r"(?m)^\s*(public\s+|private\s+|internal\s+|open\s+)*(func|class|struct|enum|protocol|typealias)\s+"
            }
            "java" => {
                r"(?m)^\s*(public|protected|private|static|final|abstract|class|interface|enum|record)\s+"
            }
            "c" | "cpp" | "c++" => {
                r"(?m)^\s*(typedef|struct|enum|class|namespace|[A-Za-z_][\w:*&<>\s]+\s+\**)[A-Za-z_][A-Za-z0-9_]*\s*\("
            }
            _ => {
                r"(?m)^\s*(pub\s+)?(async\s+)?(fn|struct|enum|trait|type|mod|class|def|function|interface|const|func)\s+"
            }
        };
        let pattern = format!("{declarations}.*{query}");
        run_search(
            &pattern,
            args.path.as_deref().unwrap_or("."),
            self.workspace.as_deref(),
        )
    }
}

impl Tool for FindReferences {
    fn name(&self) -> &str {
        "find_references"
    }

    fn description(&self) -> &str {
        "Find usages of a symbol or text across the workspace. Input JSON: {\"query\":\"symbol name\",\"path\":\"optional directory\",\"language\":\"optional file extension\"}."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: SearchInput =
            serde_json::from_str(input).context("invalid find_references JSON")?;
        if args.query.trim().is_empty() {
            bail!("find_references: query cannot be empty");
        }
        let mut pattern = args.query.trim().to_owned();
        if pattern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            pattern = format!(r"\b{}\b", regex_escape(&pattern));
        }
        run_search(
            &pattern,
            args.path.as_deref().unwrap_or("."),
            self.workspace.as_deref(),
        )
        .map(|output| {
            if let Some(language) = args.language.as_deref() {
                let language = language.trim_start_matches('.').to_ascii_lowercase();
                let extension = match language.as_str() {
                    "rust" => "rs",
                    "python" => "py",
                    "typescript" => "ts",
                    "javascript" => "js",
                    "c++" => "cpp",
                    other => other,
                };
                output
                    .lines()
                    .filter(|line| line.contains(&format!(".{extension}:")))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                output
            }
        })
    }
}

fn regex_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn resolve_search_root(path: Option<&str>, workspace: Option<&Path>) -> Result<PathBuf> {
    let requested = Path::new(path.unwrap_or("."));
    let root = match workspace {
        Some(workspace) => {
            let workspace = workspace.canonicalize()?;
            let candidate = if requested.is_absolute() {
                requested.to_path_buf()
            } else {
                workspace.join(requested)
            };
            let candidate = candidate.canonicalize()?;
            if !candidate.starts_with(&workspace) {
                bail!("code intelligence path is outside the Teammate workspace");
            }
            candidate
        }
        None => requested.canonicalize()?,
    };
    if !root.is_dir() {
        bail!(
            "code intelligence path is not a directory: {}",
            root.display()
        );
    }
    Ok(root)
}

fn run_search(pattern: &str, path: &str, workspace: Option<&Path>) -> Result<String> {
    let root = resolve_search_root(Some(path), workspace)?;
    let mut command = Command::new("rg");
    command.args([
        "--line-number",
        "--with-filename",
        "--max-count",
        &MAX_RESULTS.to_string(),
        "--glob",
        "!.git/",
        "--glob",
        "!target/",
        "--glob",
        "!node_modules/",
    ]);
    command.arg("--").arg(pattern).arg(root);
    let output = command
        .output()
        .context("failed to run ripgrep for code intelligence; install rg")?;
    match output.status.code() {
        Some(0) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut end = text.len().min(MAX_BYTES);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let mut result = text[..end].to_owned();
            if text.len() > MAX_BYTES {
                result.push_str("\n… results truncated");
            }
            Ok(result)
        }
        Some(1) => Ok("No matches found.".into()),
        Some(code) => bail!(
            "ripgrep failed (exit {code}): {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        None => bail!("ripgrep terminated unexpectedly"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_rust_declarations() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("sample.rs"),
            "pub fn calculate() {}\nstruct Engine;\n",
        )
        .unwrap();
        let input =
            serde_json::json!({"query":"calculate", "path":directory.path(), "language":"rust"})
                .to_string();
        let output = FindSymbols::default().execute(&input).unwrap();
        assert!(output.contains("calculate"));
        assert!(!output.contains("struct Engine"));
    }

    #[test]
    fn finds_symbol_references_as_whole_words() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("sample.py"), "render()\nrenderer()\n").unwrap();
        let input = serde_json::json!({"query":"render", "path":directory.path()}).to_string();
        let output = FindReferences::default().execute(&input).unwrap();
        assert!(output.contains("render()"));
        assert!(!output.contains("renderer()"));
    }

    #[test]
    fn extracts_imports_and_declarations_for_rust_modules() {
        let path = Path::new("src/service.rs");
        let (imports, symbols) = extract_structure(
            path,
            "use crate::model::Client;\npub struct Service;\npub async fn start() {}\n",
        );
        assert!(
            imports
                .iter()
                .any(|item| item.contains("crate::model::Client"))
        );
        assert!(symbols.contains(&"Service".to_owned()));
        assert!(symbols.contains(&"start".to_owned()));
    }

    #[test]
    fn analyzes_project_modules_and_cross_file_symbol_references() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::write(
            directory.path().join("src/service.rs"),
            "pub struct Service;\npub fn start() {}\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("src/main.rs"),
            "mod service;\nuse crate::service::Service;\nfn main() { Service::new(); }\n",
        )
        .unwrap();
        let input = serde_json::json!({"path":directory.path()}).to_string();
        let output = AnalyzeCodebase::default().execute(&input).unwrap();
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/service.rs"));
        assert!(output.contains("Service ← src/service.rs"));
        assert!(output.contains("mod service"));
    }

    #[test]
    fn architecture_query_limits_results_to_matching_symbols() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("alpha.rs"), "pub struct Alpha;\n").unwrap();
        std::fs::write(directory.path().join("beta.rs"), "pub struct Beta;\n").unwrap();
        let input = serde_json::json!({"path":directory.path(),"query":"Alpha"}).to_string();
        let output = AnalyzeCodebase::default().execute(&input).unwrap();
        assert!(output.contains("Alpha"));
        assert!(!output.contains("Beta"));
    }

    #[test]
    fn teammate_code_intelligence_cannot_escape_its_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(
            outside.path().join("private.rs"),
            "fn private_symbol() {}\n",
        )
        .unwrap();
        let input = serde_json::json!({
            "path": outside.path(),
            "query": "private_symbol",
            "language": "rust"
        })
        .to_string();

        assert!(
            FindSymbols::scoped(workspace.path().to_path_buf())
                .execute(&input)
                .is_err()
        );
        assert!(
            FindReferences::scoped(workspace.path().to_path_buf())
                .execute(&input)
                .is_err()
        );
        assert!(
            AnalyzeCodebase::scoped(workspace.path().to_path_buf())
                .execute(&input)
                .is_err()
        );
    }
}
