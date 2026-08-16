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
}
