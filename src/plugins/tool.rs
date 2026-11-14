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
    let parsed_input = serde_json::from_str::<Value>(&input).unwrap_or(Value::String(input));
    if let Some(schema) = &tool.input_schema {
        validate_schema_definition(schema, "$schema")?;
        validate_input_schema(&parsed_input, schema, "$input")?;
    }
    let workspace = std::env::current_dir().unwrap_or_else(|_| root.clone());
    let request = serde_json::to_vec(&json!({
        "protocol": "luma-plugin",
        "version": 1,
        "plugin": manifest.id,
        "tool": tool.name,
        "input": parsed_input,
        "context": {
            "luma_version": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "workspace": workspace,
            "plugin_directory": root,
        },
        "tool_metadata": {
            "description": tool.description,
            "model_guide": tool.model_guide,
        },
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
    let timeout = Duration::from_secs(tool.timeout_seconds.unwrap_or(30));
    let max_output_bytes = tool.max_output_bytes.unwrap_or(1_000_000);
    let output_read_limit = max_output_bytes
        .checked_add(1)
        .ok_or_else(|| anyhow!("plugin output limit is too large"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("plugin stdout was not available"))?;
    use tokio::io::AsyncReadExt;
    let (output, status) = tokio::time::timeout(timeout, async {
        let write_request = async {
            stdin.write_all(&request).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
            drop(stdin);
            Ok::<_, anyhow::Error>(())
        };
        let read_response = async {
            let mut output = Vec::new();
            stdout
                .take(output_read_limit as u64)
                .read_to_end(&mut output)
                .await?;
            if output.len() > max_output_bytes {
                bail!("plugin response exceeded the configured {max_output_bytes} byte limit");
            }
            Ok::<_, anyhow::Error>(output)
        };
        let ((), output) = tokio::try_join!(write_request, read_response)?;
        let status = child.wait().await?;
        Ok::<_, anyhow::Error>((output, status))
    })
    .await
    .with_context(|| format!("Luma plugin timed out after {}s", timeout.as_secs()))??;
    if !status.success() {
        bail!("plugin {} exited with {}", manifest.id, status);
    }
    let response: Value =
        serde_json::from_slice(&output).context("plugin returned invalid JSON")?;
    parse_plugin_response(response)
}

fn parse_plugin_response(response: Value) -> Result<String> {
    if !response.is_object() {
        bail!("plugin response must be a JSON object");
    }
    if response.get("ok").and_then(Value::as_bool) == Some(false) {
        bail!(
            "plugin error: {}",
            response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unspecified error")
        );
    }
    if let Some(output) = response.get("output") {
        return output
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("plugin response `output` must be a string"));
    }
    if let Some(content) = response.get("content") {
        let content = content
            .as_array()
            .ok_or_else(|| anyhow!("plugin response `content` must be an array"))?;
        let mut blocks = Vec::new();
        for block in content {
            let object = block
                .as_object()
                .ok_or_else(|| anyhow!("plugin content blocks must be objects"))?;
            let kind = object
                .get("type")
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| anyhow!("plugin content block `type` must be a string"))
                })
                .transpose()?
                .unwrap_or("text");
            let text = object
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("plugin content blocks must contain a string `text`"))?;
            if !text.is_empty() {
                blocks.push(match kind {
                    "text" | "markdown" => text.to_owned(),
                    "code" => format!("```\n{text}\n```"),
                    other => format!("[{other}] {text}"),
                });
            }
        }
        return Ok(blocks.join("\n\n"));
    }
    if let Some(result) = response.get("result") {
        return Ok(result.to_string());
    }
    bail!("plugin response must contain an `output` string, `content` array, or `result` value")
}

/// Validate the documented JSON Schema subset accepted by native plugins.
/// Unknown keywords are ignored, but malformed supported keywords are errors.
fn validate_schema_definition(schema: &Value, path: &str) -> Result<()> {
    let schema = schema
        .as_object()
        .ok_or_else(|| anyhow!("plugin input schema at {path} must be an object"))?;
    if let Some(value) = schema.get("type") {
        let types = match value {
            Value::String(value) => vec![value.as_str()],
            Value::Array(values) => values
                .iter()
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        anyhow!("plugin input schema at {path}: `type` entries must be strings")
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            _ => bail!("plugin input schema at {path}: `type` must be a string or array"),
        };
        if types.is_empty()
            || types.iter().any(|kind| {
                !matches!(
                    *kind,
                    "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
                )
            })
        {
            bail!("plugin input schema at {path}: `type` contains an unsupported or empty type");
        }
    }
    if schema.get("enum").is_some_and(|value| !value.is_array()) {
        bail!("plugin input schema at {path}: `enum` must be an array");
    }
    if let Some(required) = schema.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| anyhow!("plugin input schema at {path}: `required` must be an array"))?;
        if required.iter().any(|value| !value.is_string()) {
            bail!("plugin input schema at {path}: `required` entries must be strings");
        }
    }
    if let Some(properties) = schema.get("properties") {
        let properties = properties.as_object().ok_or_else(|| {
            anyhow!("plugin input schema at {path}: `properties` must be an object")
        })?;
        for (name, child) in properties {
            validate_schema_definition(child, &format!("{path}.properties.{name}"))?;
        }
    }
    if let Some(items) = schema.get("items") {
        validate_schema_definition(items, &format!("{path}.items"))?;
    }
    if let Some(additional) = schema.get("additionalProperties") {
        match additional {
            Value::Bool(_) => {}
            Value::Object(_) => {
                validate_schema_definition(additional, &format!("{path}.additionalProperties"))?
            }
            _ => bail!(
                "plugin input schema at {path}: `additionalProperties` must be a boolean or schema"
            ),
        }
    }
    for keyword in [
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "minProperties",
        "maxProperties",
    ] {
        if schema
            .get(keyword)
            .is_some_and(|value| value.as_u64().is_none())
        {
            bail!("plugin input schema at {path}: `{keyword}` must be a non-negative integer");
        }
    }
    for keyword in ["minimum", "maximum"] {
        if schema
            .get(keyword)
            .is_some_and(|value| value.as_f64().is_none())
        {
            bail!("plugin input schema at {path}: `{keyword}` must be a number");
        }
    }
    Ok(())
}

fn validate_input_schema(value: &Value, schema: &Value, path: &str) -> Result<()> {
    let schema = schema
        .as_object()
        .ok_or_else(|| anyhow!("plugin input schema at {path} must be an object"))?;
    if let Some(expected) = schema.get("type") {
        let types = match expected {
            Value::String(value) => vec![value.as_str()],
            Value::Array(values) => values
                .iter()
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        anyhow!("plugin input schema at {path}: `type` entries must be strings")
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            _ => bail!("plugin input schema at {path}: `type` must be a string or array"),
        };
        if types.is_empty() {
            bail!("plugin input schema at {path}: `type` must not be empty");
        }
        let matches = types.iter().try_fold(false, |matched, expected| {
            let this_matches = match *expected {
                "object" => value.is_object(),
                "array" => value.is_array(),
                "string" => value.is_string(),
                "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
                "number" => value.is_number(),
                "boolean" => value.is_boolean(),
                "null" => value.is_null(),
                other => bail!("plugin input schema at {path}: unsupported type '{other}'"),
            };
            Ok::<_, anyhow::Error>(matched || this_matches)
        })?;
        if !matches {
            bail!("plugin input validation failed at {path}: expected one of {types:?}");
        }
    }
    if let Some(options) = schema.get("enum") {
        let options = options
            .as_array()
            .ok_or_else(|| anyhow!("plugin input schema at {path}: `enum` must be an array"))?;
        if !options.contains(value) {
            bail!("plugin input validation failed at {path}: value is not an allowed enum option");
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required") {
            let required = required.as_array().ok_or_else(|| {
                anyhow!("plugin input schema at {path}: `required` must be an array")
            })?;
            for key in required {
                let key = key.as_str().ok_or_else(|| {
                    anyhow!("plugin input schema at {path}: `required` entries must be strings")
                })?;
                if !object.contains_key(key) {
                    bail!(
                        "plugin input validation failed at {path}: missing required property '{key}'"
                    );
                }
            }
        }
        if let Some(properties) = schema.get("properties") {
            let properties = properties.as_object().ok_or_else(|| {
                anyhow!("plugin input schema at {path}: `properties` must be an object")
            })?;
            for (key, child_schema) in properties {
                if let Some(child) = object.get(key) {
                    validate_input_schema(child, child_schema, &format!("{path}.{key}"))?;
                }
            }
        }
        validate_count_bound(schema, "minProperties", object.len(), path)?;
        validate_max_count_bound(schema, "maxProperties", object.len(), path)?;
        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
            let properties = schema.get("properties").and_then(Value::as_object);
            if let Some(key) = object
                .keys()
                .find(|key| !properties.is_some_and(|properties| properties.contains_key(*key)))
            {
                bail!("plugin input validation failed at {path}: unexpected property '{key}'");
            }
        } else if let Some(additional_schema) = schema
            .get("additionalProperties")
            .filter(|value| value.is_object())
        {
            let properties = schema.get("properties").and_then(Value::as_object);
            for (key, child) in object {
                if !properties.is_some_and(|properties| properties.contains_key(key)) {
                    validate_input_schema(child, additional_schema, &format!("{path}.{key}"))?;
                }
            }
        }
    }
    if let Some(array) = value.as_array() {
        validate_count_bound(schema, "minItems", array.len(), path)?;
        validate_max_count_bound(schema, "maxItems", array.len(), path)?;
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_input_schema(item, item_schema, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(text) = value.as_str() {
        validate_count_bound(schema, "minLength", text.chars().count(), path)?;
        validate_max_count_bound(schema, "maxLength", text.chars().count(), path)?;
    }
    if let Some(number) = value.as_f64() {
        if let Some(min) = schema.get("minimum") {
            let min = min.as_f64().ok_or_else(|| {
                anyhow!("plugin input schema at {path}: `minimum` must be a number")
            })?;
            if number < min {
                bail!("plugin input validation failed at {path}: number is below minimum");
            }
        }
        if let Some(max) = schema.get("maximum") {
            let max = max.as_f64().ok_or_else(|| {
                anyhow!("plugin input schema at {path}: `maximum` must be a number")
            })?;
            if number > max {
                bail!("plugin input validation failed at {path}: number exceeds maximum");
            }
        }
    }
    Ok(())
}

fn validate_count_bound(
    schema: &serde_json::Map<String, Value>,
    keyword: &str,
    actual: usize,
    path: &str,
) -> Result<()> {
    if let Some(minimum) = schema.get(keyword) {
        let minimum = minimum.as_u64().ok_or_else(|| {
            anyhow!("plugin input schema at {path}: `{keyword}` must be a non-negative integer")
        })?;
        if (actual as u64) < minimum {
            bail!("plugin input validation failed at {path}: value is below {keyword}");
        }
    }
    Ok(())
}

fn validate_max_count_bound(
    schema: &serde_json::Map<String, Value>,
    keyword: &str,
    actual: usize,
    path: &str,
) -> Result<()> {
    if let Some(maximum) = schema.get(keyword) {
        let maximum = maximum.as_u64().ok_or_else(|| {
            anyhow!("plugin input schema at {path}: `{keyword}` must be a non-negative integer")
        })?;
        if (actual as u64) > maximum {
            bail!("plugin input validation failed at {path}: value exceeds {keyword}");
        }
    }
    Ok(())
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
            timeout_seconds: None,
            max_output_bytes: None,
        };
        let tool = PluginTool::new(directory.path().to_owned(), manifest, definition);
        assert_eq!(tool.execute("{}").unwrap(), "native-plugin-ok");
        assert_eq!(tool.model_guide(), tool.description());
    }

    #[test]
    fn validates_plugin_input_against_declared_json_schema() {
        let schema = json!({
            "type":"object",
            "required":["query"],
            "properties":{"query":{"type":"string","minLength":2}}
        });
        assert!(validate_input_schema(&json!({"query":"hello"}), &schema, "$input").is_ok());
        assert!(validate_input_schema(&json!({"query":"x"}), &schema, "$input").is_err());
        assert!(validate_input_schema(&json!({}), &schema, "$input").is_err());
    }

    #[test]
    fn validates_nested_plugin_schema_values() {
        let schema = json!({"type":"array","items":{"type":"integer","minimum":1}});
        assert!(validate_input_schema(&json!([1, 2, 3]), &schema, "$input").is_ok());
        assert!(validate_input_schema(&json!([1, 0]), &schema, "$input").is_err());
        assert!(validate_input_schema(&json!("not an array"), &schema, "$input").is_err());
    }

    #[test]
    fn validates_json_schema_type_unions_and_object_constraints() {
        let schema = json!({
            "type": ["object", "null"],
            "required": ["name"],
            "properties": {"name": {"type": "string", "minLength": 2}},
            "additionalProperties": false,
            "minProperties": 1
        });
        assert!(validate_input_schema(&json!({"name":"Luma"}), &schema, "$input").is_ok());
        assert!(validate_input_schema(&Value::Null, &schema, "$input").is_ok());
        assert!(validate_input_schema(&json!({"name":"x"}), &schema, "$input").is_err());
        assert!(
            validate_input_schema(&json!({"name":"Luma","extra":true}), &schema, "$input").is_err()
        );
    }

    #[test]
    fn rejects_malformed_supported_schema_keywords() {
        assert!(
            validate_input_schema(&json!("hello"), &json!({"type":"strng"}), "$input").is_err()
        );
        assert!(validate_input_schema(&json!({}), &json!({"required":"name"}), "$input").is_err());
        assert!(
            validate_input_schema(&json!("hello"), &json!({"minLength":-1}), "$input").is_err()
        );
        assert!(validate_input_schema(&json!(1), &json!({"minimum":"1"}), "$input").is_err());
    }

    #[test]
    fn parses_text_markdown_code_and_json_result_responses() {
        assert_eq!(
            parse_plugin_response(json!({"content":[
                {"type":"text","text":"hello"},
                {"type":"markdown","text":"**world**"},
                {"type":"code","text":"let x = 1;"}
            ]}))
            .unwrap(),
            "hello\n\n**world**\n\n```\nlet x = 1;\n```"
        );
        assert_eq!(
            parse_plugin_response(json!({"result":{"count":2}})).unwrap(),
            "{\"count\":2}"
        );
        assert_eq!(parse_plugin_response(json!({"content":[]})).unwrap(), "");
        assert!(parse_plugin_response(json!({"content":[{"type":"text"}]})).is_err());
        assert!(parse_plugin_response(json!({"ok":false,"error":"failed"})).is_err());
    }

    #[test]
    fn drains_stdout_while_sending_large_requests() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("plugin.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"output\":\"'\nhead -c 200000 /dev/zero | tr '\\000' x\nprintf '\"}\\n'\nIFS= read -r request\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let manifest = PluginManifest {
            schema_version: 1,
            id: "pipe-test".into(),
            name: "Pipe test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            command: "./plugin.sh".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            tools: Vec::new(),
        };
        let tool = PluginToolManifest {
            name: "large".into(),
            description: "Exercise concurrent pipe IO".into(),
            model_guide: None,
            read_only: false,
            requires_network: true,
            input_schema: None,
            timeout_seconds: Some(5),
            max_output_bytes: Some(300_000),
        };
        let result = execute_plugin(
            directory.path().to_owned(),
            manifest,
            tool,
            format!("\"{}\"", "i".repeat(500_000)),
        );
        let output = run_async(result).unwrap();
        assert_eq!(output.len(), 200_000);
    }

    #[test]
    fn enforces_configured_plugin_timeout() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("plugin.sh");
        std::fs::write(&script, "#!/bin/sh\nIFS= read -r request\nexec sleep 2\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let manifest = PluginManifest {
            schema_version: 1,
            id: "timeout-test".into(),
            name: "Timeout test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            command: "./plugin.sh".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            tools: Vec::new(),
        };
        let tool = PluginToolManifest {
            name: "slow".into(),
            description: "Sleep past the deadline".into(),
            model_guide: None,
            read_only: false,
            requires_network: true,
            input_schema: None,
            timeout_seconds: Some(1),
            max_output_bytes: None,
        };
        let error = run_async(execute_plugin(
            directory.path().to_owned(),
            manifest,
            tool,
            "{}".into(),
        ))
        .unwrap_err();
        assert!(error.to_string().contains("1s"));
    }

    #[test]
    fn rejects_plugin_stdout_over_the_configured_limit() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("plugin.sh");
        std::fs::write(&script, "#!/bin/sh\nhead -c 2048 /dev/zero\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let manifest = PluginManifest {
            schema_version: 1,
            id: "output-limit-test".into(),
            name: "Output limit test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            command: "./plugin.sh".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            tools: Vec::new(),
        };
        let tool = PluginToolManifest {
            name: "large-output".into(),
            description: "Exceed the output limit".into(),
            model_guide: None,
            read_only: false,
            requires_network: true,
            input_schema: None,
            timeout_seconds: Some(5),
            max_output_bytes: Some(1_024),
        };
        let error = run_async(execute_plugin(
            directory.path().to_owned(),
            manifest,
            tool,
            "{}".into(),
        ))
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exceeded the configured 1024 byte limit")
        );
    }
}
