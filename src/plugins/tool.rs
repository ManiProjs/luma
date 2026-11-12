use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

use super::{PluginManifest, PluginToolManifest};
use crate::tools::Tool;

pub struct PluginTool {
    root: PathBuf,
    manifest: PluginManifest,
    tool: PluginToolManifest,
    name: String,
    description: String,
    model_guide: String,
}

impl PluginTool {
    pub(super) fn new(root: PathBuf, manifest: PluginManifest, tool: PluginToolManifest) -> Self {
        let name = format!("plugin__{}__{}", manifest.id, tool.name);
        let schema = tool
            .input_schema
            .as_ref()
            .map(|schema| format!(" Input schema: {schema}"))
            .unwrap_or_default();
        let description = format!(
            "{} (Luma plugin {} v{}).{}{}",
            tool.description,
            manifest.name,
            manifest.version,
            schema,
            if tool.requires_network {
                " Requires internet access."
            } else {
                ""
            }
        );
        let model_guide = tool
            .model_guide
            .clone()
            .filter(|guide| !guide.trim().is_empty())
            .unwrap_or_else(|| description.clone());
        Self {
            root,
            manifest,
            tool,
            name,
            description,
            model_guide,
        }
    }
}

impl Tool for PluginTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn model_guide(&self) -> &str {
        &self.model_guide
    }
    // Manifest hints are descriptive, not authority. External plugin actions
    // remain approval-gated even if the plugin labels them read-only.
    fn is_read_only(&self) -> bool {
        false
    }
    fn requires_network(&self) -> bool {
        // A child process has ambient network access unless an OS sandbox
        // removes it. A manifest cannot certify that it is offline.
        true
    }

    fn execute(&self, input: &str) -> Result<String> {
        let root = self.root.clone();
        let manifest = self.manifest.clone();
        let tool = self.tool.clone();
        let input = input.to_owned();
        run_async(async move { execute_plugin(root, manifest, tool, input).await })
    }
}

fn run_async<F>(future: F) -> Result<String>
where
    F: std::future::Future<Output = Result<String>> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(future),
            Err(error) => Err(anyhow!(error)),
        };
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| anyhow!("Luma plugin worker thread panicked"))?
}

async fn execute_plugin(
    root: PathBuf,
    manifest: PluginManifest,
    tool: PluginToolManifest,
    input: String,
) -> Result<String> {
    let command_path = PathBuf::from(&manifest.command);
    let program = if command_path.components().count() > 1 || manifest.command.starts_with('.') {
        root.join(command_path)
    } else {
        command_path
    };
    let request = serde_json::to_vec(&json!({
        "protocol": "luma-plugin",
        "version": 1,
        "plugin": manifest.id,
        "tool": tool.name,
        "input": serde_json::from_str::<Value>(&input).unwrap_or(Value::String(input)),
    }))?;
    if request.len() > 1_000_000 {
        bail!("plugin request exceeded the 1 MB limit");
    }
    let mut command = tokio::process::Command::new(program);
    command
        .env_clear()
        .args(&manifest.args)
        .current_dir(&root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    for key in ["PATH", "HOME", "TMPDIR", "TEMP", "TMP", "SYSTEMROOT"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for (key, value) in &manifest.env {
        let resolved = if value.starts_with("${") && value.ends_with('}') {
            let variable = &value[2..value.len() - 1];
            std::env::var(variable).with_context(|| {
                format!(
                    "plugin '{}' requires environment variable {variable}",
                    manifest.id
                )
            })?
        } else {
            value.clone()
        };
        command.env(key, resolved);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("starting Luma plugin {}", manifest.id))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("plugin stdin was not available"))?;
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(Duration::from_secs(30), async {
        stdin.write_all(&request).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await
    })
    .await
    .context("timed out writing the Luma plugin request")??;
    drop(stdin);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("plugin stdout was not available"))?;
    use tokio::io::AsyncReadExt;
    let (output, status) = tokio::time::timeout(Duration::from_secs(30), async {
        let mut output = Vec::new();
        stdout.take(1_000_001).read_to_end(&mut output).await?;
        if output.len() > 1_000_000 {
            child.kill().await?;
            bail!("plugin response exceeded the 1 MB limit");
        }
        let status = child.wait().await?;
        Ok::<_, anyhow::Error>((output, status))
    })
    .await
    .context("Luma plugin timed out after 30 seconds")??;
    if !status.success() {
        bail!("plugin {} exited with {}", manifest.id, status);
    }
    let response: Value =
        serde_json::from_slice(&output).context("plugin returned invalid JSON")?;
    if response.get("ok").and_then(Value::as_bool) == Some(false) {
        bail!(
            "plugin error: {}",
            response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unspecified error")
        );
    }
    if let Some(output) = response.get("output").and_then(Value::as_str) {
        return Ok(output.to_owned());
    }
    if let Some(result) = response.get("result") {
        return Ok(result.to_string());
    }
    bail!("plugin response must contain an `output` string or `result` value")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};

    #[test]
    fn executes_the_native_luma_plugin_json_protocol() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("plugin.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nIFS= read -r request\nprintf '{\"output\":\"native-plugin-ok\"}\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let manifest = PluginManifest {
            schema_version: 1,
            id: "test-plugin".into(),
            name: "Test plugin".into(),
            version: "1.0.0".into(),
            description: String::new(),
            command: "./plugin.sh".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            tools: Vec::new(),
        };
        let definition = PluginToolManifest {
            name: "hello".into(),
            description: "Return a test response".into(),
            model_guide: None,
            read_only: true,
            requires_network: false,
            input_schema: None,
        };
        let tool = PluginTool::new(directory.path().to_owned(), manifest, definition);
        assert_eq!(tool.execute("{}").unwrap(), "native-plugin-ok");
        assert_eq!(tool.model_guide(), tool.description());
    }
}
