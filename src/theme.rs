use ratatui::prelude::{Color, Modifier, Style};

pub struct LumaTheme {
    pub star: Color,
    pub glow: Color,
    pub space: Color,
    pub accent: Color,
}

impl LumaTheme {
    pub fn default() -> Self {
        Self {
            star: Color::Yellow,
            glow: Color::LightYellow,
            space: Color::Blue,
            accent: Color::Cyan,
        }
    }

    pub fn heading_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn code_style(&self) -> Style {
        Style::default().fg(self.glow)
    }

    pub fn quote_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::ITALIC)
    }

    pub fn code_block_border_style(&self) -> Style {
        Style::default().fg(self.space)
    }

    pub fn bold_style(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }
}
