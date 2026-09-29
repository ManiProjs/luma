pub mod agent;
pub mod commands;
pub mod config;
pub mod context;
pub mod dashboard;
pub mod desktop;
pub mod event;
pub mod history;
pub mod logging;
pub mod model;
pub mod planner;
pub mod protocol;
pub mod provider;
pub mod router;
pub mod session;
pub mod tools;
pub mod workspace;

#[cfg(feature = "tui")]
pub mod theme;

#[cfg(feature = "tui")]
pub mod tui;

#[cfg(test)]
pub mod tests;
