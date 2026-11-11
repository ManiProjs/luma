#[derive(Debug)]
pub enum Command {
    Help,
    Clear,
    Quit,
    Init,
    Unknown,
}

#[cfg(feature = "tui")]
const COMMANDS: &[&str] = &[
    "/help",
    "/clear",
    "/new",
    "/quit",
    "/init",
    "/model",
    "/model session",
    "/tools",
    "/skills",
    "/teammates",
    "/agents",
    "/tasks",
    "/approvals",
    "/schedules",
    "/memory",
    "/history",
    "/restore",
    "/summary:add",
    "/status",
    "/workspace",
    "/history list",
    "/history show",
    "/history delete",
    "/history prune",
    "/plan",
    "/resume",
    "/version",
    "/doctor",
];

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

    COMMANDS
        .iter()
        .filter(|command| command.starts_with(&input))
        .map(|command| command.to_string())
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
    }

    #[test]
    fn new_is_an_alias_for_clear() {
        assert!(matches!(Command::parse("/new"), Some(Command::Clear)));
    }
}
