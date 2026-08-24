#[derive(Debug)]
pub enum Command {
    Help,
    Clear,
    Quit,
    Init,
    Unknown(String),
}

const COMMANDS: &[&str] = &["/help", "/clear", "/quit", "/init", "/model", "/tools"];

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

            other => Command::Unknown(other.trim_start_matches('/').to_string()),
        })
    }
}

pub fn suggestions(input: &str) -> Vec<String> {
    if !input.starts_with('/') {
        return Vec::new();
    }

    let input = input.to_lowercase();

    COMMANDS
        .iter()
        .filter(|command| command.starts_with(&input))
        .map(|command| command.to_string())
        .collect()
}
