use ratatui::prelude::{Color, Style};

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

    pub fn code_style(&self) -> Style {
        Style::default().fg(self.glow)
    }

    pub fn code_block_border_style(&self) -> Style {
        Style::default().fg(self.space)
    }
}
