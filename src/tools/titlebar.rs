use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::Tool;

#[derive(Deserialize)]
struct TitlebarStatusInput {
    text: String,
    #[serde(default = "default_color")]
    color: String,
}

fn default_color() -> String {
    "blue".to_string()
}

pub struct SetTitlebarStatus;

impl Tool for SetTitlebarStatus {
    fn name(&self) -> &str {
        "set_titlebar_status"
    }

    fn description(&self) -> &str {
        "Update the desktop app's titlebar status pill while you work. JSON input: {\"text\":\"Running tests\",\"color\":\"blue|green|amber|red|muted\"}. Use concise status text; set text to an empty string to restore the automatic status."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let value: TitlebarStatusInput = serde_json::from_str(input)
            .context("set_titlebar_status expects JSON with text and optional color")?;
        if value.text.chars().count() > 48 {
            bail!("Titlebar status text must be 48 characters or fewer.");
        }
        if !["blue", "green", "amber", "red", "muted"].contains(&value.color.as_str()) {
            bail!("Unsupported status color. Choose blue, green, amber, red, or muted.");
        }
        let Some(path) = env::var_os("LUMA_TITLEBAR_STATUS_FILE").map(PathBuf::from) else {
            bail!("The desktop titlebar status channel is unavailable in this session.");
        };
        let parent = path
            .parent()
            .context("titlebar status path has no parent")?;
        fs::create_dir_all(parent).context("creating titlebar status directory")?;
        let temporary = path.with_extension("json.tmp");
        fs::write(
            &temporary,
            serde_json::json!({ "text": value.text, "color": value.color }).to_string(),
        )
        .context("writing titlebar status")?;
        fs::rename(&temporary, &path).context("publishing titlebar status")?;
        Ok("Desktop titlebar status updated.".to_string())
    }

    fn is_read_only(&self) -> bool {
        true
    }
}
