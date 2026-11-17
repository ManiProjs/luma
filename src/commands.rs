#[derive(Debug)]
pub enum Command {
    Help,
    Clear,
    Quit,
    Init,
    Unknown,
}

/// A command advertised by Luma Core to every interactive client. Keep the
/// registry here so Ink, desktop, and future clients share names and intent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandDescriptor {
    pub id: String,
    pub command: String,
    pub description: String,
    pub category: String,
    pub shortcut: Option<String>,
}

pub fn registry() -> Vec<CommandDescriptor> {
    const ITEMS: &[(&str, &str, &str, &str, Option<&str>)] = &[
        (
            "help",
            "/help",
            "Show all available commands",
            "General",
            None,
        ),
        (
            "command_palette",
            "/commands",
            "Open the command palette",
            "General",
            Some("Ctrl+K"),
        ),
        (
            "new",
            "/new",
            "Start a fresh conversation",
            "Conversation",
            Some("Ctrl+N"),
        ),
        (
            "clear",
            "/clear",
            "Clear the current conversation",
            "Conversation",
            None,
        ),
        (
            "copy",
            "/copy",
            "Copy the latest assistant response",
            "Conversation",
            None,
        ),
        (
            "rename",
            "/rename",
            "Rename the active conversation",
            "Conversation",
            None,
        ),
        (
            "compact",
            "/compact",
            "Compact the active model context",
            "Conversation",
            None,
        ),
        (
            "cancel",
            "/cancel",
            "Cancel the active request",
            "Conversation",
            Some("Esc"),
        ),
        (
            "history",
            "/history",
            "Search saved conversations",
            "Conversation",
            None,
        ),
        (
            "history_show",
            "/history show",
            "Inspect a saved conversation",
            "Conversation",
            None,
        ),
        (
            "history_list",
            "/history list",
            "List saved conversations",
            "Conversation",
            None,
        ),
        (
            "history_delete",
            "/history delete",
            "Delete a saved conversation",
            "Conversation",
            None,
        ),
        (
            "history_prune",
            "/history prune",
            "Delete all saved conversations",
            "Conversation",
            None,
        ),
        (
            "restore",
            "/restore",
            "Restore a saved conversation",
            "Conversation",
            None,
        ),
        (
            "summary",
            "/summary:add",
            "Save a conversation summary",
            "Conversation",
            None,
        ),
        (
            "model",
            "/model",
            "Choose the active model",
            "Models",
            Some("Ctrl+G"),
        ),
        (
            "model_session",
            "/model session",
            "Choose a model for this session only",
            "Models",
            None,
        ),
        (
            "workspace",
            "/workspace",
            "Inspect the active workspace",
            "Workspace",
            None,
        ),
        (
            "files",
            "/files",
            "Browse workspace files",
            "Workspace",
            None,
        ),
        (
            "find_files",
            "/find",
            "Find and open workspace files by fuzzy path",
            "Workspace",
            None,
        ),
        (
            "search",
            "/search",
            "Search workspace files and saved conversations",
            "Workspace",
            None,
        ),
        (
            "git",
            "/git",
            "Inspect branches, changes, and history",
            "Git",
            None,
        ),
        (
            "teammates",
            "/teammates",
            "Open the Teammate control center",
            "Teammates",
            None,
        ),
        (
            "agents",
            "/agents",
            "Alias for the Teammate control center",
            "Teammates",
            None,
        ),
        (
            "tasks",
            "/tasks",
            "Monitor queued and running tasks",
            "Teammates",
            None,
        ),
        (
            "approvals",
            "/approvals",
            "Review pending approvals",
            "Teammates",
            None,
        ),
        (
            "schedules",
            "/schedules",
            "Manage scheduled teammate work",
            "Teammates",
            None,
        ),
        (
            "memory",
            "/memory",
            "Inspect saved memories",
            "Teammates",
            None,
        ),
        (
            "plugins",
            "/plugins",
            "Manage installed plugins",
            "Extensions",
            None,
        ),
        ("mcp", "/mcp", "Manage MCP connections", "Extensions", None),
        (
            "skills",
            "/skills",
            "Browse available skills",
            "Extensions",
            None,
        ),
        (
            "tools",
            "/tools",
            "List tools available to the active agent",
            "Extensions",
            None,
        ),
        (
            "status",
            "/status",
            "Show session and runtime status",
            "Diagnostics",
            None,
        ),
        (
            "usage",
            "/usage",
            "Show session token and cost totals",
            "Diagnostics",
            None,
        ),
        (
            "monitor",
            "/monitor",
            "Open the live system monitor",
            "Diagnostics",
            None,
        ),
        (
            "doctor",
            "/doctor",
            "Run workspace and runtime diagnostics",
            "Diagnostics",
            None,
        ),
        (
            "version",
            "/version",
            "Show the installed Luma version",
            "Diagnostics",
            None,
        ),
        (
            "init",
            "/init",
            "Inspect this workspace and create its project guide",
            "Workspace",
            None,
        ),
        (
            "plan",
            "/plan",
            "Create a plan without implementing it",
            "Conversation",
            None,
        ),
        (
            "resume",
            "/resume",
            "Resume an interrupted workspace task",
            "Conversation",
            None,
        ),
        ("exit", "/exit", "Exit Luma", "General", Some("Ctrl+C")),
        (
            "fullscreen",
            "/fullscreen",
            "Toggle terminal fullscreen",
            "View",
            Some("Ctrl+F"),
        ),
        ("quit", "/quit", "Exit Luma", "General", Some("Ctrl+C")),
    ];
    ITEMS
        .iter()
        .map(
            |(id, command, description, category, shortcut)| CommandDescriptor {
                id: (*id).into(),
                command: (*command).into(),
                description: (*description).into(),
                category: (*category).into(),
                shortcut: shortcut.map(str::to_owned),
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::registry;
    use std::collections::HashSet;

    #[test]
    fn core_command_registry_has_unique_ids_and_slash_commands() {
        let commands = registry();
        let ids = commands
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        let names = commands
            .iter()
            .map(|item| item.command.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), commands.len());
        assert_eq!(names.len(), commands.len());
        assert!(
            commands
                .iter()
                .any(|item| item.id == "compact" && item.command == "/compact")
        );
        assert!(
            commands
                .iter()
                .any(|item| item.id == "git" && item.command == "/git")
        );
        assert!(
            commands
                .iter()
                .any(|item| item.id == "find_files" && item.command == "/find")
        );
        assert!(
            commands
                .iter()
                .any(|item| item.id == "tasks" && item.command == "/tasks")
        );
    }
}

impl Command {
    pub fn parse(input: &str) -> Option<Self> {
        if !input.starts_with('/') {
            return None;
        }

        let command = input.trim();

        Some(match command {
            "/help" => Command::Help,

            "/clear" | "/new" => Command::Clear,

            "/quit" | "/exit" => Command::Quit,

            "/init" => Command::Init,

            _ => Command::Unknown,
        })
    }
}

#[cfg(feature = "tui")]
pub fn suggestions(input: &str) -> Vec<String> {
    // Don't open the autocomplete panel for a bare slash. Apart from being
    // noisy, the full command list can consume most of a short terminal; wait
    // until the user starts typing a command name.
    if input.len() < 2 || !input.starts_with('/') {
        return Vec::new();
    }

    let input = input.to_lowercase();

    registry()
        .iter()
        .map(|item| item.command.as_str())
        .filter(|command| command.starts_with(&input))
        .map(str::to_owned)
        .collect()
}

#[cfg(all(test, feature = "tui"))]
mod tests {
    use super::{Command, suggestions};

    #[test]
    fn bare_slash_does_not_open_autocomplete() {
        assert!(suggestions("/").is_empty());
    }

    #[test]
    fn slash_prefix_suggests_matching_commands() {
        let matches = suggestions("/h");
        assert!(matches.contains(&"/help".to_owned()));
        assert!(matches.contains(&"/history".to_owned()));
        assert!(matches.contains(&"/history delete".to_owned()));
        assert!(matches.contains(&"/history prune".to_owned()));
    }

    #[test]
    fn non_command_input_has_no_suggestions() {
        assert!(suggestions("help").is_empty());
    }

    #[test]
    fn argument_prefix_suggests_history_subcommands() {
        assert_eq!(suggestions("/history d"), vec!["/history delete"]);
    }

    #[test]
    fn suggestions_include_personal_agent_management_commands() {
        assert_eq!(suggestions("/tea"), vec!["/teammates"]);
        assert!(suggestions("/mem").contains(&"/memory".to_owned()));
        assert!(suggestions("/app").contains(&"/approvals".to_owned()));
        assert_eq!(suggestions("/plu"), vec!["/plugins"]);
        assert_eq!(suggestions("/mc"), vec!["/mcp"]);
    }

    #[test]
    fn new_is_an_alias_for_clear() {
        assert!(matches!(Command::parse("/new"), Some(Command::Clear)));
    }

    #[test]
    fn cancel_is_a_suggested_command() {
        assert_eq!(suggestions("/can"), vec!["/cancel"]);
    }
}
