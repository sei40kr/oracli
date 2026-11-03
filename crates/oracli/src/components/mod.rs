mod grid;
mod input;
mod log;
mod status;

pub use grid::Grid;
pub use input::Input;
pub use log::{Entry, Log};
pub use status::Status;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::app::Action;
use crate::db::DbEvent;
use crate::theme::Theme;

/// A piece of the screen that owns its state. Components never talk to each other or the
/// database; they return [`Action`]s and the app dispatches them.
pub trait Component {
    /// Called only while the component has focus.
    fn handle_key(&mut self, _key: KeyEvent) -> Vec<Action> {
        vec![]
    }

    /// Called for every DB event, focused or not.
    fn on_db(&mut self, _ev: &DbEvent) {}

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
}

/// Display width of each column over the header and rows, capped at `max`.
fn column_widths(columns: &[String], rows: &[Vec<String>], max: usize) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            rows.iter()
                .filter_map(|r| r.get(i))
                .map(|v| cell_width(v))
                .chain([c.width()])
                .max()
                .unwrap_or(0)
                .min(max)
        })
        .collect()
}

fn cell_width(v: &str) -> usize {
    v.lines().next().unwrap_or("").width() + usize::from(v.contains('\n'))
}

/// First line of `s`, cut to `width` columns with `…` when it does not fit.
fn fit(s: &str, width: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.width() <= width && !s.contains('\n') {
        return line.to_owned();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in line.chars() {
        let cw = c.to_string().width();
        if w + cw + 1 > width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

/// `s` padded with spaces to `width` display columns.
fn pad(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(s.width())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_truncates_by_display_width() {
        assert_eq!(fit("abc", 3), "abc");
        assert_eq!(fit("abcd", 3), "ab…");
        assert_eq!(fit("あいう", 5), "あい…");
        assert_eq!(fit("a\nb", 5), "a…");
    }
}
