#[derive(Clone, Debug)]
pub struct LumaInfo {
    pub provider: String,

    pub model: String,

    pub status: String,

    pub workspace: Option<String>,

    pub language: Option<String>,

    pub build_system: Option<String>,

    pub git_branch: Option<String>,

    pub files_scanned: usize,

    pub context_tokens: Option<usize>,

    pub tools: Vec<String>,

    pub tips: Vec<String>,

    pub recent_tasks: Vec<String>,
}

impl LumaInfo {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, tools: Vec<String>) -> Self {
        Self {
            provider: provider.into(),

            model: model.into(),

            status: "Ready".into(),

            workspace: None,

            language: None,

            build_system: None,

            git_branch: None,

            files_scanned: 0,

            context_tokens: None,

            tools,

            tips: vec![
                "Ask me to inspect your project".into(),
                "I can edit files and create new ones".into(),
                "Use ↑ ↓ to navigate command history".into(),
                "Ctrl+C interrupts generation".into(),
            ],

            recent_tasks: Vec::new(),
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn add_recent_task(&mut self, task: impl Into<String>) {
        self.recent_tasks.push(task.into());

        // Keep the dashboard clean
        if self.recent_tasks.len() > 5 {
            self.recent_tasks.remove(0);
        }
    }

    pub fn set_workspace(&mut self, path: impl Into<String>) {
        self.workspace = Some(path.into());
    }

    pub fn set_language(&mut self, language: impl Into<String>) {
        self.language = Some(language.into());
    }

    pub fn set_build_system(&mut self, build: impl Into<String>) {
        self.build_system = Some(build.into());
    }

    pub fn set_git_branch(&mut self, branch: impl Into<String>) {
        self.git_branch = Some(branch.into());
    }

    pub fn update_files_scanned(&mut self, count: usize) {
        self.files_scanned = count;
    }

    pub fn refresh_tips(&mut self) {
        self.tips.clear();

        if self.tools.iter().any(|x| x == "write_file") {
            self.tips.push("I can create and modify files".into());
        }

        if self.tools.iter().any(|x| x == "run_command") {
            self.tips.push("I can run build and test commands".into());
        }

        if let Some(language) = &self.language {
            self.tips.push(format!("Detected {} project", language));
        }

        self.tips.push("Ctrl+C interrupts generation".into());

        self.tips.push("↑ ↓ navigates command history".into());
    }
}
