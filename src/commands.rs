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
    "/quit",
    "/init",
    "/model",
    "/tools",
    "/history",
    "/restore",
    "/summary:add",
];

impl Command {
    pub fn parse(input: &str) -> Option<Self> {
        if !input.starts_with('/') {
            return None;
        }

        let command = input.trim();

        Some(match command {
            "/help" => Command::Help,

            "/clear" => Command::Clear,

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
    use super::suggestions;

    #[test]
    fn bare_slash_does_not_open_autocomplete() {
        assert!(suggestions("/").is_empty());
    }

    #[test]
    fn slash_prefix_suggests_matching_commands() {
        assert_eq!(suggestions("/h"), vec!["/help", "/history"]);
    }

    #[test]
    fn non_command_input_has_no_suggestions() {
        assert!(suggestions("help").is_empty());
    }
}
