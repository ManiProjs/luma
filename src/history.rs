use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[cfg(all(not(test), unix))]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(not(test))]
use std::{
    fs::OpenOptions,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(not(test))]
static HISTORY_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct History {
    pub messages: Vec<HistoryMessage>,
    /// Only the primary Code session owns the legacy history.json file.
    /// Teammate workers use an in-memory history reconstructed from runtime tasks.
    #[serde(skip)]
    persist: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

impl History {
    fn path() -> PathBuf {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".config")
            .join("luma")
            .join("history.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let mut history = match fs::read_to_string(path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        history.persist = true;
        history
    }

    #[cfg(test)]
    pub fn from_messages(messages: Vec<HistoryMessage>) -> Self {
        Self {
            messages,
            persist: false,
        }
    }

    pub fn empty_like(&self) -> Self {
        Self {
            messages: Vec::new(),
            persist: self.persist,
        }
    }

    pub fn save(&self) -> Result<()> {
        if !self.persist {
            return Ok(());
        }
        // In tests, never write to the user's real history file.
        #[cfg(test)]
        {
            Ok(())
        }

        #[cfg(not(test))]
        {
            let path = Self::path();

            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let data = serde_json::to_string_pretty(self)?;
            let sequence = HISTORY_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary =
                path.with_extension(format!("json.{}.{}.tmp", std::process::id(), sequence));
            let result = (|| -> Result<()> {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                options.mode(0o600);
                let mut file = options.open(&temporary)?;
                file.write_all(data.as_bytes())?;
                file.sync_all()?;
                drop(file);
                fs::rename(&temporary, &path)?;
                // Rename is the commit point. A directory-sync failure after
                // it must not be reported as a failed save, or the caller may
                // keep old memory while the new history is already on disk.
                #[cfg(unix)]
                if let Some(parent) = path.parent()
                    && let Err(error) =
                        fs::File::open(parent).and_then(|directory| directory.sync_all())
                {
                    tracing::warn!(%error, "history was replaced but its directory could not be synced");
                }
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{History, HistoryMessage};

    #[test]
    fn worker_history_is_ephemeral_and_never_targets_code_history() {
        let history = History::from_messages(vec![HistoryMessage {
            role: "user".into(),
            content: "teammate request".into(),
        }]);
        assert!(!history.persist);
        assert!(!history.empty_like().persist);
    }
}
