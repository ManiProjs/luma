use anyhow::{Result, bail};
use std::{collections::BTreeMap, path::Path};

pub fn handles(args: &[String]) -> bool {
    matches!(args.first().map(String::as_str), Some("plugins" | "mcp"))
}

pub fn run(args: &[String]) -> Result<()> {
    let Some(group) = args.first().map(String::as_str) else {
        return Ok(());
    };
    let action = args.get(1).map(String::as_str).unwrap_or("list");
    let values = &args[2..];
    match (group, action) {
        ("plugins", "list") => {
            let entries = crate::plugins::PluginManager::default().list()?;
            if entries.is_empty() {
                println!("No native Luma plugins installed.");
            }
            for (record, manifest) in entries {
                println!(
                    "{}  {} v{}  {}\n  {}\n  tools: {}",
                    record.id,
                    manifest.name,
                    manifest.version,
                    if record.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    manifest.description,
                    manifest
                        .tools
                        .iter()
                        .map(|tool| tool.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        ("plugins", "install") => {
            let path = one(values, "luma plugins install <path>")?;
            let record = crate::plugins::PluginManager::default().install(Path::new(path))?;
            println!(
                "Installed {} (disabled). Inspect it, then run: luma plugins enable {}",
                record.id, record.id
            );
        }
        ("plugins", "enable" | "disable") => {
            let id = one(values, "luma plugins enable|disable <id>")?;
            let record =
                crate::plugins::PluginManager::default().set_enabled(id, action == "enable")?;
            println!(
                "{}: {}",
                record.id,
                if record.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        ("plugins", "uninstall") => {
            let id = one(values, "luma plugins uninstall <id>")?;
            if !crate::plugins::PluginManager::default().uninstall(id)? {
                bail!("plugin not installed: {id}");
            }
            println!("Uninstalled plugin {id}.");
        }
        ("mcp", "list") => {
            let servers = crate::mcp::McpManager::default().list()?;
            if servers.is_empty() {
                println!("No MCP servers configured.");
            }
            for server in servers {
                println!(
                    "{}  {}  {} {}",
                    server.id,
                    if server.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    server.command,
                    server.args.join(" ")
                );
            }
        }
        ("mcp", "add") => {
            if values.len() < 2 {
                bail!("usage: luma mcp add <id> <command> [args...] [--env KEY=VALUE]");
            }
            let id = values[0].clone();
            let command = values[1].clone();
            let mut server_args = Vec::new();
            let mut env = BTreeMap::new();
            let mut index = 2;
            while index < values.len() {
                if values[index] == "--env" {
                    index += 1;
                    let pair = values
                        .get(index)
                        .ok_or_else(|| anyhow::anyhow!("--env requires KEY=VALUE"))?;
                    let (key, value) = pair
                        .split_once('=')
                        .ok_or_else(|| anyhow::anyhow!("--env requires KEY=VALUE"))?;
                    if key.is_empty() {
                        bail!("environment variable name cannot be empty");
                    }
                    env.insert(key.to_owned(), value.to_owned());
                } else {
                    server_args.push(values[index].clone());
                }
                index += 1;
            }
            let server = crate::mcp::McpManager::default().add(id, command, server_args, env)?;
            println!(
                "Registered MCP server {} (disabled). Run `luma mcp tools {}` to inspect its tools, then enable it.",
                server.id, server.id
            );
        }
        ("mcp", "enable" | "disable") => {
            let id = one(values, "luma mcp enable|disable <id>")?;
            let server = crate::mcp::McpManager::default().set_enabled(id, action == "enable")?;
            println!(
                "{}: {}",
                server.id,
                if server.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        ("mcp", "remove") => {
            let id = one(values, "luma mcp remove <id>")?;
            if !crate::mcp::McpManager::default().remove(id)? {
                bail!("MCP server not found: {id}");
            }
            println!("Removed MCP server {id}.");
        }
        ("mcp", "tools") => {
            let id = one(values, "luma mcp tools <id>")?;
            let tools = crate::mcp::McpManager::default().tools(id)?;
            if tools.is_empty() {
                println!("This MCP server exposes no tools.");
            }
            for tool in tools {
                println!(
                    "{}\n  {}\n  {}",
                    tool.name, tool.description, tool.input_schema
                );
            }
        }
        _ => bail!("unknown {group} command `{action}`; run `luma {group} --help`"),
    }
    Ok(())
}

fn one<'a>(values: &'a [String], usage: &str) -> Result<&'a str> {
    if values.len() != 1 {
        bail!("usage: {usage}");
    }
    Ok(&values[0])
}

#[cfg(test)]
mod tests {
    use super::handles;

    #[test]
    fn integration_commands_are_dispatched_separately() {
        assert!(handles(&["plugins".into(), "list".into()]));
        assert!(handles(&["mcp".into(), "add".into()]));
        assert!(!handles(&["teammates".into()]));
        assert!(!handles(&["desktop-server".into()]));
    }
}
