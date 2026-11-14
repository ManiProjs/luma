use anyhow::Result;
use std::collections::HashMap;

pub mod code_intelligence;
pub mod developer;
pub mod filesystem;
pub mod git;
pub mod shell;
pub mod skills;
pub mod titlebar;
pub mod web;

struct AskQuestion;

impl Tool for AskQuestion {
    fn name(&self) -> &str {
        "ask_question"
    }
    fn description(&self) -> &str {
        "Ask the user for missing information and wait for their answer. Input JSON: {\"prompt\":\"question\",\"choices\":[\"optional choice\"]}. This is an interactive control-flow action, not an approval."
    }
    fn execute(&self, _input: &str) -> Result<String> {
        anyhow::bail!("ask_question must be handled by the Agent interaction loop")
    }
    fn is_read_only(&self) -> bool {
        true
    }
}

/// Construct the standard tool set for an isolated agent execution.
/// Keep registration in one place so Code and Teammates cannot silently gain
/// different capabilities.
pub fn default_registry() -> ToolRegistry {
    use code_intelligence::{FindReferences, FindSymbols};
    use developer::{
        CountLines, FileMetadata, FindTodos, GitBlame, GitBranches, GitLog, GitShow, InspectProject,
    };
    use filesystem::{
        list_directory::ListDirectory, patch_file::PatchFile, read_file::ReadFile,
        search_files::SearchFiles, write_file::WriteFile,
    };
    use git::{GitDiff, GitStatus};
    use shell::RunCommand;
    use skills::{ListSkills, LoadSkill};
    use titlebar::SetTitlebarStatus;

    let mut tools = ToolRegistry::new();
    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);
    tools.register(SearchFiles);
    tools.register(WriteFile);
    tools.register(PatchFile);
    tools.register(GitStatus);
    tools.register(GitDiff);
    tools.register(FindSymbols);
    tools.register(FindReferences);
    tools.register(GitLog);
    tools.register(GitShow);
    tools.register(GitBlame);
    tools.register(GitBranches);
    tools.register(FileMetadata);
    tools.register(FindTodos);
    tools.register(CountLines);
    tools.register(InspectProject);
    tools.register(ListSkills);
    tools.register(LoadSkill);
    tools.register(SetTitlebarStatus);
    tools.register(AskQuestion);
    tools
}

/// Personal-agent tools are deliberately separate from the Code tool set.
/// Teammates receive the same local capabilities plus explicit network tools.
pub fn teammate_registry() -> ToolRegistry {
    let mut tools = default_registry();
    tools.register(web::WebSearch);
    tools.register(web::FetchUrl);
    tools.register(web::HttpRequest);
    tools
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    /// Guidance for the model when deciding how and when to use this tool.
    /// Existing tools inherit a useful default from their user-facing description.
    fn model_guide(&self) -> &str {
        self.description()
    }

    fn execute(&self, input: &str) -> Result<String>;

    /// Tools are treated as mutating unless they explicitly declare that
    /// their operation is read-only. External integrations default to the
    /// safer mutating classification.
    fn is_read_only(&self) -> bool {
        false
    }

    /// Whether the tool's execution may access the public network.
    fn requires_network(&self) -> bool {
        false
    }
}

/// The three pieces of metadata exposed to the model for every registered tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub guide: String,
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn retain(&mut self, allowed: &[String]) {
        self.tools.retain(|name, _| {
            name == "ask_question"
                || name == "set_titlebar_status"
                || allowed.iter().any(|item| item == name)
        });
    }

    pub fn execute(&self, name: &str, input: &str) -> Result<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {name}"))?;

        tool.execute(input)
    }

    pub fn is_read_only(&self, name: &str) -> bool {
        self.tools.get(name).is_some_and(|tool| {
            tool.is_read_only()
                || matches!(
                    name,
                    "read_file"
                        | "list_directory"
                        | "search_files"
                        | "git_status"
                        | "git_diff"
                        | "find_symbols"
                        | "find_references"
                        | "git_log"
                        | "git_show"
                        | "git_blame"
                        | "git_branches"
                        | "file_metadata"
                        | "find_todos"
                        | "count_lines"
                        | "inspect_project"
                        | "list_skills"
                        | "load_skill"
                )
        })
    }

    pub fn requires_network(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .is_some_and(|tool| tool.requires_network())
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();

        names.sort();

        names
    }

    pub fn metadata(&self) -> Vec<ToolMetadata> {
        let mut metadata: Vec<_> = self
            .tools
            .values()
            .map(|tool| ToolMetadata {
                name: tool.name().to_owned(),
                description: tool.description().to_owned(),
                guide: tool.model_guide().to_owned(),
            })
            .collect();
        metadata.sort_by(|left, right| left.name.cmp(&right.name));
        metadata
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{default_registry, teammate_registry};

    #[test]
    fn teammates_keep_every_code_tool_and_add_personal_agent_network_tools() {
        let code = default_registry();
        let teammates = teammate_registry();
        let code_tools = code.names();
        let teammate_tools = teammates.names();

        assert!(code_tools.iter().all(|name| teammate_tools.contains(name)));
        assert!(code_tools.iter().any(|name| name == "ask_question"));
        assert!(code_tools.iter().any(|name| name == "set_titlebar_status"));
        assert!(teammate_tools.iter().any(|name| name == "ask_question"));
        assert!(
            teammate_tools
                .iter()
                .any(|name| name == "set_titlebar_status")
        );
        for name in ["web_search", "fetch_url", "http_request"] {
            assert!(teammate_tools.iter().any(|tool| tool == name));
            assert!(!code_tools.iter().any(|tool| tool == name));
        }
    }

    #[test]
    fn ask_question_remains_available_when_teammate_tools_are_allowlisted() {
        let mut tools = teammate_registry();
        tools.retain(&["read_file".into()]);
        assert!(tools.names().iter().any(|name| name == "ask_question"));
        assert!(
            tools
                .names()
                .iter()
                .any(|name| name == "set_titlebar_status")
        );
        assert!(tools.names().iter().any(|name| name == "read_file"));
    }

    #[test]
    fn built_in_read_only_tools_are_classified_for_permission_checks() {
        let tools = default_registry();
        for name in [
            "read_file",
            "list_directory",
            "search_files",
            "git_status",
            "list_skills",
            "load_skill",
        ] {
            assert!(tools.is_read_only(name), "{name} should be read-only");
        }
        for name in ["write_file", "patch_file", "run_command"] {
            assert!(!tools.is_read_only(name), "{name} must remain mutating");
        }
    }

    #[test]
    fn registry_exposes_sorted_complete_model_metadata() {
        let tools = default_registry();
        let metadata = tools.metadata();
        assert!(!metadata.is_empty());
        assert!(metadata.windows(2).all(|pair| pair[0].name <= pair[1].name));
        let read_file = metadata
            .iter()
            .find(|tool| tool.name == "read_file")
            .unwrap();
        assert!(read_file.description.contains("Read"));
        assert_eq!(read_file.guide, read_file.description);
    }
}
