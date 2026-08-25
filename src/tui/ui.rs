use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    theme::LumaTheme,
    tui::{
        app::App,
        components::{chat::ChatView, input::InputBox, welcome::WelcomeScreen},
        info::LumaInfo,
    },
};

pub fn draw(frame: &mut Frame, app: &App, theme: &LumaTheme, info: &LumaInfo, _confirm_exit: bool) {
    let area = frame.area().inner(Margin {
        vertical: 0,
        horizontal: 1,
    });

    let input_height = if app.suggestions.is_empty() {
        3
    } else {
        (5 + app.suggestions.len() as u16).min(11)
    };

    let tool_height = if app.current_tool.is_some() { 1 } else { 0 };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(tool_height),
            Constraint::Length(input_height),
        ])
        .split(area);

    // ------------------------------------------------------------
    // Header
    // ------------------------------------------------------------

    render_header(frame, layout[0], theme, info);

    // ------------------------------------------------------------
    // Main content
    // ------------------------------------------------------------

    if app.welcome_visible {
        let welcome = WelcomeScreen::new(theme, info);
        welcome.render(frame, layout[1]);
    } else {
        let chat = ChatView::new(theme, app);
        chat.render(frame, layout[1]);
    }

    // ------------------------------------------------------------
    // Active tool
    // ------------------------------------------------------------

    if let Some(tool) = &app.current_tool {
        render_tool_activity(frame, layout[2], theme, &tool.name, &tool.input);
    }

    // ------------------------------------------------------------
    // Input
    // ------------------------------------------------------------

    let input = InputBox::new(
        theme,
        &app.input,
        &app.suggestions,
        app.selected_suggestion,
        app.thinking,
    );

    input.render(frame, layout[3]);
}

// ============================================================
// Header
// ============================================================

fn render_header(frame: &mut Frame, area: Rect, theme: &LumaTheme, info: &LumaInfo) {
    let status = if info.status.trim().is_empty() {
        "Ready"
    } else {
        info.status.as_str()
    };

    let status_style = status_style(theme, status);

    let provider = if info.provider.trim().is_empty() {
        "unknown"
    } else {
        info.provider.as_str()
    };

    let model = if info.model.trim().is_empty() {
        "unknown"
    } else {
        info.model.as_str()
    };

    let line = Line::from(vec![
        // LUMA
        Span::styled(
            " LUMA",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        // Status
        Span::styled("● ", status_style),
        Span::styled(status, status_style),
        Span::raw("   "),
        // Provider
        Span::styled(provider, Style::default().fg(theme.space)),
        Span::styled(" / ", Style::default().fg(theme.space)),
        // Model
        Span::styled(
            model,
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        // Tools
        Span::styled(
            format!("{} tools", info.tools.len()),
            Style::default().fg(theme.space),
        ),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

// ============================================================
// Status styling
// ============================================================

fn status_style(theme: &LumaTheme, status: &str) -> Style {
    let status = status.to_ascii_lowercase();

    // Successful / idle states
    if matches!(
        status.as_str(),
        "ready" | "done" | "all set" | "mission complete" | "wrapped up" | "ship it"
    ) {
        return Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD);
    }

    // Error states
    if matches!(
        status.as_str(),
        "hit a snag"
            | "something went sideways"
            | "ran into a wall"
            | "that didn't work"
            | "lost the trail"
            | "hmm..."
    ) {
        return Style::default().fg(theme.star).add_modifier(Modifier::BOLD);
    }

    // Everything else is considered active.
    Style::default().fg(theme.glow).add_modifier(Modifier::BOLD)
}

// ============================================================
// Tool activity
// ============================================================

fn render_tool_activity(frame: &mut Frame, area: Rect, theme: &LumaTheme, name: &str, input: &str) {
    let line = Line::from(vec![
        Span::styled(
            " ◇ ",
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            name,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(input, Style::default().fg(theme.space)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}
