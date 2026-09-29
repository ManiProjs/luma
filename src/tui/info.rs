//! The header/status data the Ratatui front end renders.
//!
//! Every field here is derived from [`SessionInfo`], [`SessionState`], and
//! [`UsageStats`] — the core already owns all of it. `LumaInfo` exists only
//! so `ui.rs` can read it in one place; it is deleted with the rest of the
//! Ratatui front end.

use crate::session::{SessionInfo, SessionState, UsageStats};

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
    pub fn new(info: &SessionInfo) -> Self {
        Self {
            provider: info.provider.clone(),
            model: info.model.clone(),
            status: "Ready".into(),
            workspace: Some(info.workspace.clone()),
            tools: info.tools.clone(),
            usage: UsageStats::default(),
        }
    }

    /// Re-read everything the core owns, leaving front-end-only state alone.
    pub fn sync(&mut self, info: &SessionInfo, state: &SessionState) {
        self.provider = info.provider.clone();
        self.model = info.model.clone();
        self.workspace = Some(info.workspace.clone());
        self.tools = info.tools.clone();
        self.usage = state.usage;
        self.status = state.status.clone();
    }
}
