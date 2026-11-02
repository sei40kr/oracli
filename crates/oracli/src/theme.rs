//! Every style oracli draws with. Components and the prompt take their styles from here.

use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub keyword: Style,
    pub string: Style,
    pub number: Style,
    pub quoted_identifier: Style,
    pub comment: Style,
    pub prompt: Style,
    pub suggestion: Style,
    pub menu: Style,
    pub menu_selected: Style,
    pub menu_meta: Style,
    pub warning: Style,
    pub error: Style,
    pub header: Style,
    pub muted: Style,
    pub grid_selected: Style,
    pub status: Style,
    pub status_dirty: Style,
    pub status_busy: Style,
}

impl Theme {
    /// Monochrome when `NO_COLOR` is set to a non-empty value (<https://no-color.org>).
    pub fn from_env() -> Self {
        match std::env::var_os("NO_COLOR") {
            Some(v) if !v.is_empty() => Self::monochrome(),
            _ => Self::default(),
        }
    }

    /// Attributes only, for terminals or users that want no color.
    pub fn monochrome() -> Self {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let reversed = Style::new().add_modifier(Modifier::REVERSED);
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            keyword: bold,
            string: Style::new(),
            number: Style::new(),
            quoted_identifier: Style::new(),
            comment: dim,
            prompt: bold,
            suggestion: dim,
            menu: reversed,
            menu_selected: bold,
            menu_meta: Style::new(),
            warning: bold,
            error: bold,
            header: bold,
            muted: dim,
            grid_selected: reversed,
            status: reversed,
            status_dirty: bold,
            status_busy: bold,
        }
    }

    /// Style for a token class from [`SqlLexer`](crate::sql::SqlLexer).
    pub fn syntax(&self, class: &str) -> Style {
        match class {
            "keyword" => self.keyword,
            "string" => self.string,
            "number" => self.number,
            "quoted_identifier" => self.quoted_identifier,
            "comment" => self.comment,
            _ => Style::new(),
        }
    }
}

/// ANSI colors, so the terminal's own palette decides the exact shades.
impl Default for Theme {
    fn default() -> Self {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        Self {
            keyword: Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD),
            string: Style::new().fg(Color::Green),
            number: Style::new().fg(Color::Magenta),
            quoted_identifier: Style::new().fg(Color::Yellow),
            comment: Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            prompt: bold,
            suggestion: Style::new().fg(Color::DarkGray),
            menu: Style::new().bg(Color::DarkGray).fg(Color::White),
            menu_selected: Style::new().bg(Color::Gray).fg(Color::Black),
            menu_meta: Style::new().add_modifier(Modifier::ITALIC),
            warning: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::Red),
            header: bold,
            muted: Style::new().fg(Color::DarkGray),
            grid_selected: Style::new().add_modifier(Modifier::REVERSED),
            status: Style::new().bg(Color::DarkGray).fg(Color::White),
            status_dirty: Style::new().bg(Color::Yellow).fg(Color::Black),
            status_busy: Style::new().fg(Color::Yellow),
        }
    }
}
