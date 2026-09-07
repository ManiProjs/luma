#[derive(Debug)]
pub enum RoutedAction {
    Tool { name: String, input: String },

    Planner,
}

pub struct ToolRouter;

impl ToolRouter {
    pub fn route(input: &str) -> RoutedAction {
        let text = input.to_lowercase();

        // Directory inspection
        if Self::contains_any(
            &text,
            &[
                "list files",
                "list folders",
                "list directories",
                "show files",
                "show folders",
                "show directories",
                "project structure",
                "directory structure",
                "folder structure",
                "what files",
                "what folders",
            ],
        ) {
            return RoutedAction::Tool {
                name: "list_directory".into(),
                input: ".".into(),
            };
        }

        // Search requests — extract a useful pattern from natural language
        // instead of passing the whole sentence as a regex.
        if Self::contains_any(&text, &["find", "search", "where is", "locate"]) {
            let pattern = Self::extract_search_pattern(&text);

            if !pattern.is_empty() {
                return RoutedAction::Tool {
                    name: "search_files".into(),
                    input: serde_json::json!({ "pattern": pattern }).to_string(),
                };
            }
        }

        RoutedAction::Planner
    }

    /// Pull the searchable term out of phrases like:
    ///   "find where authentication is handled"
    ///   "search for the login function"
    ///   "where is the config file"
    fn extract_search_pattern(text: &str) -> String {
        // Strip common search prefixes to isolate the actual term.
        const PREFIXES: &[&str] = &[
            "find where ",
            "find the ",
            "find ",
            "search for ",
            "search the ",
            "search ",
            "where is the ",
            "where is ",
            "locate the ",
            "locate ",
        ];

        let mut term = text;

        for prefix in PREFIXES {
            if let Some(rest) = term.strip_prefix(prefix) {
                term = rest;
                break;
            }
        }

        // Strip a leading "the " that may survive prefix removal.
        if let Some(rest) = term.strip_prefix("the ") {
            term = rest;
        }

        // Remove trailing filler words that don't help ripgrep.
        const SUFFIXES: &[&str] = &[
            " is handled",
            " is defined",
            " is implemented",
            " is located",
            " is used",
            " is called",
            " file",
            " function",
            " code",
        ];

        for suffix in SUFFIXES {
            if let Some(rest) = term.strip_suffix(suffix) {
                term = rest;
                break;
            }
        }

        term.trim().to_string()
    }

    fn contains_any(text: &str, words: &[&str]) -> bool {
        words.iter().any(|word| text.contains(word))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_pattern_from_find_where() {
        assert_eq!(
            ToolRouter::extract_search_pattern("find where authentication is handled"),
            "authentication"
        );
    }

    #[test]
    fn extracts_pattern_from_search_for() {
        assert_eq!(
            ToolRouter::extract_search_pattern("search for the login function"),
            "login"
        );
    }

    #[test]
    fn extracts_pattern_from_where_is() {
        assert_eq!(
            ToolRouter::extract_search_pattern("where is the config file"),
            "config"
        );
    }

    #[test]
    fn routes_search_to_tool() {
        match ToolRouter::route("find where authentication is handled") {
            RoutedAction::Tool { name, input } => {
                assert_eq!(name, "search_files");
                let json: serde_json::Value = serde_json::from_str(&input).unwrap();
                assert_eq!(json["pattern"], "authentication");
            }
            RoutedAction::Planner => panic!("expected Tool, got Planner"),
        }
    }
}
