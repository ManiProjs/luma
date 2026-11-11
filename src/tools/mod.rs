use anyhow::Result;
use std::collections::HashMap;

pub mod code_intelligence;
pub mod developer;
pub mod filesystem;
pub mod git;
pub mod shell;
pub mod skills;
pub mod web;

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

    fn execute(&self, input: &str) -> Result<String>;
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

    pub fn retain(&mut self, allowed: &[String]) {
        self.tools
            .retain(|name, _| allowed.iter().any(|item| item == name));
    }

    pub fn execute(&self, name: &str, input: &str) -> Result<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {name}"))?;

        tool.execute(input)
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();

        names.sort();

        names
    }

    pub fn descriptions(&self) -> Vec<String> {
        self.tools
            .values()
            .map(|tool| format!("{}: {}", tool.name(), tool.description()))
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
