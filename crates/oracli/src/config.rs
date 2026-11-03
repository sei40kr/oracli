//! User settings from `$XDG_CONFIG_HOME/oracli/config.toml`.

use std::io;
use std::path::PathBuf;

use serde::Deserialize;

use crate::theme::Theme;

pub struct Config {
    pub theme: Theme,
    /// Rows of a result set kept in the log; the grid holds the rest.
    pub log_rows: usize,
    /// Widest a column gets, so one wide value cannot push the rest off screen.
    pub max_cell_width: usize,
}

/// The file as written. `theme` stays a table so it can be laid over the built-in theme.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct File {
    log_rows: usize,
    max_cell_width: usize,
    theme: toml::Table,
}

impl Default for File {
    fn default() -> Self {
        Self {
            log_rows: 20,
            max_cell_width: 40,
            theme: toml::Table::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::parse("", Theme::default()).expect("empty config parses")
    }
}

impl Config {
    /// A missing file means the defaults.
    pub fn load() -> io::Result<Self> {
        let path = config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(io::Error::new(e.kind(), format!("{}: {e}", path.display()))),
        };
        Self::parse(&text, Theme::from_env())
            .map_err(|e| io::Error::other(format!("{}: {e}", path.display())))
    }

    /// Each style in `[theme]` replaces that style of `base` whole.
    fn parse(text: &str, base: Theme) -> Result<Self, toml::de::Error> {
        let file: File = toml::from_str(text)?;
        let mut theme = toml::Table::try_from(&base).expect("theme serializes");
        theme.extend(file.theme);
        Ok(Self {
            theme: theme.try_into()?,
            log_rows: file.log_rows,
            max_cell_width: file.max_cell_width,
        })
    }
}

fn config_path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default();
    config.join("oracli/config.toml")
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier, Style};

    use super::*;

    #[test]
    fn overrides_only_what_is_set() {
        let c = Config::parse(
            r##"
            log_rows = 5
            [theme]
            keyword = { fg = "red", add_modifier = "BOLD | ITALIC" }
            status = { bg = "#102030" }
            "##,
            Theme::default(),
        )
        .unwrap();
        assert_eq!(c.log_rows, 5);
        assert_eq!(c.max_cell_width, 40);
        assert_eq!(
            c.theme.keyword,
            Style::new()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC)
        );
        assert_eq!(
            c.theme.status,
            Style::new().bg(Color::Rgb(0x10, 0x20, 0x30))
        );
        assert_eq!(c.theme.string, Theme::default().string);
    }

    #[test]
    fn monochrome_round_trips() {
        let c = Config::parse("", Theme::monochrome()).unwrap();
        assert_eq!(c.theme.menu, Theme::monochrome().menu);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Config::parse("log_row = 5", Theme::default()).is_err());
        assert!(Config::parse("[theme]\nkeywrd = {}", Theme::default()).is_err());
    }
}
