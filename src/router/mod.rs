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

        // Search requests
        if Self::contains_any(&text, &["find", "search", "where is", "locate"]) {
            return RoutedAction::Tool {
                name: "search_files".into(),
                input: text,
            };
        }

        RoutedAction::Planner
    }

    fn contains_any(text: &str, words: &[&str]) -> bool {
        words.iter().any(|word| text.contains(word))
    }
}
