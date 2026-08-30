#[derive(Clone, Debug)]
pub struct LumaInfo {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub workspace: Option<String>,
    pub tools: Vec<String>,
}

impl LumaInfo {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, tools: Vec<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            status: "Ready".into(),
            workspace: None,
            tools,
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }
}
