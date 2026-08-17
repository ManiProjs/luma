use anyhow::Result;
use std::collections::HashMap;

pub mod filesystem;
pub mod shell;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn execute(&self, input: &str) -> Result<String>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    pub fn execute(&self, name: &str, input: &str) -> Result<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {name}"))?;

        tool.execute(input)
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();

        names.sort();

        names
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }

    pub fn descriptions(&self) -> Vec<String> {
        self.tools
            .values()
            .map(|tool| format!("{}: {}", tool.name(), tool.description()))
            .collect()
    }
}
