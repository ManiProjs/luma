#[derive(Clone, Debug, Default)]
pub struct UsageStats {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Clone, Debug)]
pub struct LumaInfo {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub workspace: Option<String>,
    pub tools: Vec<String>,
    pub usage: UsageStats,
}

impl LumaInfo {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, tools: Vec<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            status: "Ready".into(),
            workspace: None,
            tools,
            usage: UsageStats::default(),
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn add_usage(
        &mut self,
        prompt_tokens: u64,
        completion_tokens: u64,
        total_tokens: u64,
        cost_usd: f64,
    ) {
        self.usage.prompt_tokens += prompt_tokens;
        self.usage.completion_tokens += completion_tokens;
        self.usage.total_tokens += total_tokens;
        self.usage.cost_usd += cost_usd;
    }
}
