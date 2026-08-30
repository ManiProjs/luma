use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme::LumaTheme;

pub struct MarkdownRenderer<'a> {
    theme: &'a LumaTheme,
}

#[derive(Default)]
struct RenderState {
    bold: bool,
    italic: bool,
    code_block: bool,
    blockquote: bool,

    list_depth: usize,
    ordered_list: Vec<usize>,

    heading: Option<u8>,
}

#[derive(Default)]
struct TableState {
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
}

impl<'a> MarkdownRenderer<'a> {
    pub fn new(theme: &'a LumaTheme) -> Self {
        Self { theme }
    }

    pub fn render(&self, markdown: &str) -> Vec<Line<'static>> {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;

        let parser = Parser::new_ext(markdown, options);

        let mut output = Vec::new();
        let mut current = Vec::<Span<'static>>::new();

        let mut state = RenderState::default();
        let mut table: Option<TableState> = None;

        for event in parser {
            match event {
                // ============================================================
                // Tables
                // ============================================================
                Event::Start(Tag::Table(_)) => {
                    flush_line(&mut output, &mut current);
                    table = Some(TableState::default());
                }

                Event::Start(Tag::TableHead) => {}

                Event::End(TagEnd::TableHead) => {}

                Event::Start(Tag::TableRow) => {
                    if let Some(table) = &mut table {
                        table.current_row.clear();
                    }
                }

                Event::End(TagEnd::TableRow) => {
                    if let Some(table) = &mut table {
                        table.rows.push(std::mem::take(&mut table.current_row));
                    }
                }

                Event::Start(Tag::TableCell) => {
                    if let Some(table) = &mut table {
                        table.current_cell.clear();
                    }
                }

                Event::End(TagEnd::TableCell) => {
                    if let Some(table) = &mut table {
                        table
                            .current_row
                            .push(std::mem::take(&mut table.current_cell));
                    }
                }

                Event::End(TagEnd::Table) => {
                    if let Some(table) = table.take() {
                        self.render_table(&mut output, table);
                    }
                }

                // ============================================================
                // Headings
                // ============================================================
                Event::Start(Tag::Heading { level, .. }) => {
                    flush_line(&mut output, &mut current);
                    state.heading = Some(level as u8);
                }

                Event::End(TagEnd::Heading(_)) => {
                    for span in &mut current {
                        span.style = span.style.patch(self.heading_style());
                    }

                    flush_line(&mut output, &mut current);
                    output.push(Line::from(""));

                    state.heading = None;
                }

                // ============================================================
                // Paragraphs
                // ============================================================
                Event::Start(Tag::Paragraph) => {
                    flush_line(&mut output, &mut current);

                    if state.blockquote {
                        current.push(Span::styled("│ ", Style::default().fg(self.theme.accent)));
                    }
                }

                Event::End(TagEnd::Paragraph) => {
                    flush_line(&mut output, &mut current);

                    if !state.code_block {
                        output.push(Line::from(""));
                    }
                }

                // ============================================================
                // Bold
                // ============================================================
                Event::Start(Tag::Strong) => {
                    state.bold = true;
                }

                Event::End(TagEnd::Strong) => {
                    state.bold = false;
                }

                // ============================================================
                // Italic
                // ============================================================
                Event::Start(Tag::Emphasis) => {
                    state.italic = true;
                }

                Event::End(TagEnd::Emphasis) => {
                    state.italic = false;
                }

                // ============================================================
                // Strikethrough
                // ============================================================
                Event::Start(Tag::Strikethrough) => {
                    // Ratatui doesn't have a dedicated state field for this,
                    // so the styling is handled directly by the text event.
                    state.italic = false;
                }

                Event::End(TagEnd::Strikethrough) => {}

                // ============================================================
                // Lists
                // ============================================================
                Event::Start(Tag::List(start)) => {
                    state.list_depth += 1;

                    if let Some(start) = start {
                        state.ordered_list.push(start as usize);
                    } else {
                        state.ordered_list.push(0);
                    }
                }

                Event::End(TagEnd::List(_)) => {
                    state.list_depth = state.list_depth.saturating_sub(1);
                    state.ordered_list.pop();
                }

                Event::Start(Tag::Item) => {
                    flush_line(&mut output, &mut current);

                    let indent = "  ".repeat(state.list_depth.saturating_sub(1));

                    current.push(Span::raw(indent));

                    let marker = match state.ordered_list.last_mut() {
                        Some(counter) if *counter > 0 => {
                            let value = format!("{}. ", *counter);
                            *counter += 1;
                            value
                        }
                        _ => "• ".to_string(),
                    };

                    current.push(Span::styled(
                        marker,
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                Event::End(TagEnd::Item) => {
                    flush_line(&mut output, &mut current);
                }

                // ============================================================
                // Blockquotes
                // ============================================================
                Event::Start(Tag::BlockQuote(_)) => {
                    flush_line(&mut output, &mut current);
                    state.blockquote = true;
                }

                Event::End(TagEnd::BlockQuote(_)) => {
                    flush_line(&mut output, &mut current);
                    output.push(Line::from(""));
                    state.blockquote = false;
                }

                // ============================================================
                // Code blocks
                // ============================================================
                Event::Start(Tag::CodeBlock(kind)) => {
                    flush_line(&mut output, &mut current);

                    state.code_block = true;

                    let label = match kind {
                        CodeBlockKind::Fenced(language) => {
                            if language.is_empty() {
                                String::new()
                            } else {
                                format!(" {}", language)
                            }
                        }

                        CodeBlockKind::Indented => String::new(),
                    };

                    output.push(Line::from(Span::styled(
                        format!("╭─{}", label),
                        self.theme.code_block_border_style(),
                    )));
                }

                Event::End(TagEnd::CodeBlock) => {
                    flush_line(&mut output, &mut current);

                    output.push(Line::from(Span::styled(
                        "╰────────────────────────────────",
                        self.theme.code_block_border_style(),
                    )));

                    output.push(Line::from(""));

                    state.code_block = false;
                }

                // ============================================================
                // Inline code
                // ============================================================
                Event::Code(code) => {
                    if let Some(table) = &mut table {
                        table.current_cell.push_str(&code);
                        continue;
                    }

                    current.push(Span::styled(code.to_string(), self.theme.code_style()));
                }

                // ============================================================
                // Links
                // ============================================================
                Event::Start(Tag::Link { .. }) => {
                    state.italic = true;
                }

                Event::End(TagEnd::Link) => {
                    state.italic = false;
                }

                // ============================================================
                // Text
                // ============================================================
                Event::Text(text) => {
                    if let Some(table) = &mut table {
                        table.current_cell.push_str(&text);
                        continue;
                    }

                    let style = self.current_style(&state);

                    current.push(Span::styled(text.to_string(), style));
                }

                // ============================================================
                // Line breaks
                // ============================================================
                Event::SoftBreak | Event::HardBreak => {
                    if let Some(table) = &mut table {
                        table.current_cell.push('\n');
                    } else if !state.code_block {
                        flush_line(&mut output, &mut current);

                        if state.blockquote {
                            current
                                .push(Span::styled("│ ", Style::default().fg(self.theme.accent)));
                        }
                    } else {
                        flush_line(&mut output, &mut current);
                    }
                }

                // ============================================================
                // HTML
                // ============================================================
                Event::Html(_) | Event::InlineHtml(_) => {}

                // ============================================================
                // Everything else
                // ============================================================
                _ => {}
            }
        }

        flush_line(&mut output, &mut current);

        output
    }

    fn current_style(&self, state: &RenderState) -> Style {
        let mut style = Style::default();

        if state.bold {
            style = style.add_modifier(Modifier::BOLD);
        }

        if state.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }

        if state.heading.is_some() {
            style = style.patch(self.heading_style());
        }

        style
    }

    fn heading_style(&self) -> Style {
        Style::default()
            .fg(self.theme.glow)
            .add_modifier(Modifier::BOLD)
    }

    // ================================================================
    // Tables
    // ================================================================

    fn render_table(&self, output: &mut Vec<Line<'static>>, table: TableState) {
        if table.rows.is_empty() {
            return;
        }

        let column_count = table.rows.iter().map(|row| row.len()).max().unwrap_or(0);

        if column_count == 0 {
            return;
        }

        let mut widths = vec![0usize; column_count];

        for row in &table.rows {
            for (index, cell) in row.iter().enumerate() {
                widths[index] = widths[index].max(Self::display_width(cell));
            }
        }

        let separator = Self::table_separator(&widths);

        // Top border
        output.push(Line::from(Span::styled(
            format!("┌{}┐", separator),
            self.theme.code_block_border_style(),
        )));

        // Rows
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut spans = Vec::new();

            spans.push(Span::styled("│ ", self.theme.code_block_border_style()));

            for (column, width) in widths.iter().enumerate() {
                let value = row.get(column).map(String::as_str).unwrap_or("");

                let style = if row_index == 0 {
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                spans.push(Span::styled(value.to_string(), style));

                let padding = width.saturating_sub(Self::display_width(value));

                spans.push(Span::raw(" ".repeat(padding)));

                if column + 1 < widths.len() {
                    spans.push(Span::styled(" │ ", self.theme.code_block_border_style()));
                }
            }

            spans.push(Span::styled(" │", self.theme.code_block_border_style()));

            output.push(Line::from(spans));

            // Header separator
            if row_index == 0 {
                output.push(Line::from(Span::styled(
                    format!("├{}┤", separator),
                    self.theme.code_block_border_style(),
                )));
            }
        }

        // Bottom border
        output.push(Line::from(Span::styled(
            format!("└{}┘", separator),
            self.theme.code_block_border_style(),
        )));

        output.push(Line::from(""));
    }

    fn table_separator(widths: &[usize]) -> String {
        let mut result = String::new();

        for (index, width) in widths.iter().enumerate() {
            result.push_str(&"─".repeat(width + 2));

            if index + 1 < widths.len() {
                result.push('┼');
            }
        }

        result
    }

    fn display_width(text: &str) -> usize {
        text.chars().count()
    }
}

fn flush_line(output: &mut Vec<Line<'static>>, current: &mut Vec<Span<'static>>) {
    if !current.is_empty() {
        output.push(Line::from(std::mem::take(current)));
    }
}
