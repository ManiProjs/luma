use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme::LumaTheme;

pub struct MarkdownRenderer<'a> {
    theme: &'a LumaTheme,
}

impl<'a> MarkdownRenderer<'a> {
    pub fn new(theme: &'a LumaTheme) -> Self {
        Self { theme }
    }

    pub fn render(&self, markdown: &str) -> Vec<Line<'static>> {
        let parser = Parser::new(markdown);

        let mut output = Vec::new();

        let mut current = Vec::<Span<'static>>::new();

        let mut table: Option<TableState> = None;

        for event in parser {
            match event {
                // ====================================================
                // Tables
                // ====================================================
                Event::Start(Tag::Table(_)) => {
                    table = Some(TableState::default());
                }

                Event::Start(Tag::TableHead) => {
                    if let Some(table) = &mut table {
                        table.in_header = true;
                    }
                }

                Event::End(TagEnd::TableHead) => {
                    if let Some(table) = &mut table {
                        table.in_header = false;
                    }
                }

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

                // ====================================================
                // Text
                // ====================================================
                Event::Text(text) => {
                    if let Some(table) = &mut table {
                        table.current_cell.push_str(&text);
                        continue;
                    }

                    current.push(Span::raw(text.to_string()));
                }

                Event::Code(code) => {
                    if let Some(table) = &mut table {
                        table.current_cell.push_str(&code);
                        continue;
                    }

                    current.push(Span::styled(code.to_string(), self.theme.code_style()));
                }

                // ====================================================
                // Strong
                // ====================================================
                Event::Start(Tag::Strong) => {}

                Event::End(TagEnd::Strong) => {
                    // For a richer renderer, you'd track style state.
                }

                // ====================================================
                // Paragraph
                // ====================================================
                Event::Start(Tag::Paragraph) => {
                    current.clear();
                }

                Event::End(TagEnd::Paragraph) => {
                    if !current.is_empty() {
                        output.push(Line::from(std::mem::take(&mut current)));
                    }

                    output.push(Line::from(""));
                }

                // ====================================================
                // Headings
                // ====================================================
                Event::Start(Tag::Heading { .. }) => {
                    current.clear();
                }

                Event::End(TagEnd::Heading(_)) => {
                    if !current.is_empty() {
                        for span in &mut current {
                            span.style = span.style.patch(self.theme.heading_style());
                        }

                        output.push(Line::from(std::mem::take(&mut current)));
                    }

                    output.push(Line::from(""));
                }

                // ====================================================
                // Everything else
                // ====================================================
                Event::SoftBreak | Event::HardBreak => {
                    if let Some(table) = &mut table {
                        table.current_cell.push('\n');
                    } else {
                        output.push(Line::from(std::mem::take(&mut current)));
                    }
                }

                Event::Start(Tag::CodeBlock(_)) => {
                    output.push(Line::from(Span::styled(
                        "│ ┌────────────────────────────────",
                        self.theme.code_block_border_style(),
                    )));
                }

                Event::End(TagEnd::CodeBlock) => {
                    output.push(Line::from(Span::styled(
                        "│ └────────────────────────────────",
                        self.theme.code_block_border_style(),
                    )));
                }

                Event::Html(_) | Event::InlineHtml(_) => {}

                _ => {}
            }
        }

        if !current.is_empty() {
            output.push(Line::from(current));
        }

        output
    }

    // ============================================================
    // Table rendering
    // ============================================================

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

        output.push(Line::from(Span::styled(
            format!("│ {}", separator),
            self.theme.code_block_border_style(),
        )));

        for (row_index, row) in table.rows.iter().enumerate() {
            let mut line = String::from("│ ");

            for column in 0..column_count {
                let value = row.get(column).map(String::as_str).unwrap_or("");

                let width = widths[column];

                line.push_str(value);

                let padding = width.saturating_sub(Self::display_width(value));

                line.push_str(&" ".repeat(padding));

                if column + 1 < column_count {
                    line.push_str(" │ ");
                }
            }

            output.push(Line::from(Span::styled(
                line,
                if row_index == 0 {
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(self.theme.accent)
                } else {
                    Style::default()
                },
            )));

            if row_index == 0 {
                output.push(Line::from(Span::styled(
                    format!("│ {}", separator),
                    self.theme.code_block_border_style(),
                )));
            }
        }

        output.push(Line::from(Span::styled(
            format!("│ {}", separator),
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

#[derive(Default)]
struct TableState {
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_header: bool,
}
