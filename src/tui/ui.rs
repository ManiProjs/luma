use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
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

    // The status bar needs real height because it has a border
    // and one line of content inside it.
    let status_height = if app.welcome_visible { 0 } else { 3 };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),             // Header
            Constraint::Min(8),                // Main content
            Constraint::Length(status_height), // Status
            Constraint::Length(input_height),  // Input
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
        let content = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(layout[1]);

        let chat = ChatView::new(theme, app);
        chat.render(frame, content[0]);

        render_info_panel(frame, content[1], theme, info, app);
    }

    // ------------------------------------------------------------
    // Status
    // ------------------------------------------------------------

    if !app.welcome_visible {
        render_status_bar(frame, layout[2], theme, info, app);
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
        Span::styled(
            " LUMA",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled(
            "CODING AGENT",
            Style::default()
                .fg(theme.space)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled(provider, Style::default().fg(theme.space)),
        Span::styled(" / ", Style::default().fg(theme.space)),
        Span::styled(
            model,
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        ),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

// ============================================================
// Status bar
// ============================================================

fn render_status_bar(frame: &mut Frame, area: Rect, theme: &LumaTheme, info: &LumaInfo, app: &App) {
    let (indicator, status, status_style) = if app.confirmation_pending() {
        (
            "●",
            "WAITING FOR CONFIRMATION",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else if let Some(_) = &app.current_tool {
        (
            "●",
            "WORKING",
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        )
    } else if app.thinking {
        (
            "●",
            "THINKING",
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        )
    } else {
        (
            "●",
            if info.status.trim().is_empty() {
                "READY"
            } else {
                info.status.as_str()
            },
            Style::default()
                .fg(theme.space)
                .add_modifier(Modifier::BOLD),
        )
    };

    let mut spans = vec![
        Span::styled(format!(" {} ", indicator), status_style),
        Span::styled(status, status_style),
    ];

    if let Some(tool) = &app.current_tool {
        spans.push(Span::styled("  ·  ", Style::default().fg(theme.space)));

        spans.push(Span::styled(
            tool.name.clone(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));

        if !tool.input.is_empty() {
            spans.push(Span::styled("  ", Style::default().fg(theme.space)));

            spans.push(Span::styled(
                tool.input.clone(),
                Style::default().fg(theme.space),
            ));
        }
    }

    let border_style = if app.confirmation_pending() {
        Style::default().fg(theme.accent)
    } else if app.thinking || app.current_tool.is_some() {
        Style::default().fg(theme.glow)
    } else {
        Style::default().fg(theme.space)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);

    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

// ============================================================
// Info panel
// ============================================================

fn render_info_panel(frame: &mut Frame, area: Rect, theme: &LumaTheme, info: &LumaInfo, app: &App) {
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                " INFO ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("• SESSION ", Style::default().fg(theme.space)),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.space));

    let inner = block.inner(area);

    frame.render_widget(block, area);

    if inner.width < 12 || inner.height < 5 {
        return;
    }

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

    let status = if app.confirmation_pending() {
        "waiting"
    } else if app.current_tool.is_some() {
        "working"
    } else if app.thinking {
        "thinking"
    } else {
        "ready"
    };

    let status_style = match status {
        "waiting" => Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),

        "working" | "thinking" => Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),

        _ => Style::default()
            .fg(theme.space)
            .add_modifier(Modifier::BOLD),
    };

    let mut lines = Vec::new();

    // --------------------------------------------------------
    // Status
    // --------------------------------------------------------

    lines.push(Line::from(vec![Span::styled(
        "STATUS",
        Style::default()
            .fg(theme.space)
            .add_modifier(Modifier::BOLD),
    )]));

    lines.push(Line::from(vec![
        Span::styled("● ", status_style),
        Span::styled(status, status_style),
    ]));

    lines.push(Line::from(""));

    // --------------------------------------------------------
    // Model
    // --------------------------------------------------------

    lines.push(Line::from(vec![Span::styled(
        "MODEL",
        Style::default()
            .fg(theme.space)
            .add_modifier(Modifier::BOLD),
    )]));

    lines.push(Line::from(vec![
        Span::styled("Provider  ", Style::default().fg(theme.space)),
        Span::styled(
            provider,
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Model     ", Style::default().fg(theme.space)),
        Span::styled(
            model,
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(""));

    // --------------------------------------------------------
    // Session
    // --------------------------------------------------------

    lines.push(Line::from(vec![Span::styled(
        "SESSION",
        Style::default()
            .fg(theme.space)
            .add_modifier(Modifier::BOLD),
    )]));

    lines.push(Line::from(vec![
        Span::styled("Messages  ", Style::default().fg(theme.space)),
        Span::styled(
            app.messages.len().to_string(),
            Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Thinking  ", Style::default().fg(theme.space)),
        Span::styled(
            if app.thinking { "yes" } else { "no" },
            if app.thinking {
                Style::default().fg(theme.glow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.space)
            },
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Confirm   ", Style::default().fg(theme.space)),
        Span::styled(
            if app.confirmation_pending() {
                "required"
            } else {
                "none"
            },
            if app.confirmation_pending() {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.space)
            },
        ),
    ]));

    lines.push(Line::from(""));

    // --------------------------------------------------------
    // Active tool
    // --------------------------------------------------------

    lines.push(Line::from(vec![Span::styled(
        "ACTIVE TOOL",
        Style::default()
            .fg(theme.space)
            .add_modifier(Modifier::BOLD),
    )]));

    if let Some(tool) = &app.current_tool {
        lines.push(Line::from(vec![
            Span::styled(
                "◇ ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                tool.name.clone(),
                Style::default().fg(theme.glow).add_modifier(Modifier::BOLD),
            ),
        ]));

        if !tool.input.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(theme.space)),
                Span::styled(tool.input.clone(), Style::default().fg(theme.space)),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "None",
            Style::default().fg(theme.space),
        )));
    }

    lines.push(Line::from(""));

    // --------------------------------------------------------
    // Tools
    // --------------------------------------------------------

    lines.push(Line::from(vec![Span::styled(
        "TOOLS",
        Style::default()
            .fg(theme.space)
            .add_modifier(Modifier::BOLD),
    )]));

    lines.push(Line::from(vec![Span::styled(
        format!("{} available", info.tools.len()),
        Style::default().fg(theme.glow),
    )]));

    lines.push(Line::from(""));

    for tool in &info.tools {
        lines.push(Line::from(vec![
            Span::styled(
                "◇ ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(tool, Style::default().fg(theme.space)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
