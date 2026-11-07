//! Resolve terminal colour policy without changing the process environment.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Appearance {
    Color,
    Monochrome,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorMode {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    #[must_use]
    pub fn apply(self) -> Appearance {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        let appearance = if self == Self::Never || (self == Self::Auto && no_color) {
            Appearance::Monochrome
        } else {
            Appearance::Color
        };
        crossterm::style::force_color_output(appearance == Appearance::Color);
        appearance
    }
}
