use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{Component, column_widths, fit, pad};
use crate::db::DbEvent;
use crate::theme::Theme;

/// Rows of a result set kept in the log; the grid holds the rest.
const LOG_ROWS: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Input(String),
    Info(String),
    Warn(String),
    Error(String),
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        more: bool,
    },
}

/// Scrollback of submitted statements and their results.
#[derive(Default)]
pub struct Log {
    entries: Vec<Entry>,
    /// Pages scrolled up from the bottom.
    scroll: usize,
}

impl Log {
    pub fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
        self.scroll = 0;
    }

    pub fn scroll(&mut self, pages: i32) {
        self.scroll = self.scroll.saturating_add_signed(pages as isize);
    }

    fn lines(&self, theme: &Theme) -> Vec<Line<'_>> {
        let mut lines = Vec::new();
        for e in &self.entries {
            match e {
                Entry::Input(s) => {
                    for (i, l) in s.lines().enumerate() {
                        let prefix = if i == 0 { "SQL> " } else { "     " };
                        lines.push(Line::from(vec![
                            Span::styled(prefix, theme.prompt),
                            Span::raw(l),
                        ]));
                    }
                }
                Entry::Info(s) => lines.extend(s.lines().map(Line::raw)),
                Entry::Warn(s) => lines.extend(s.lines().map(|l| Line::styled(l, theme.warning))),
                Entry::Error(s) => lines.extend(s.lines().map(|l| Line::styled(l, theme.error))),
                Entry::Table {
                    columns,
                    rows,
                    more,
                } => {
                    let widths = column_widths(columns, rows);
                    let row_line = |cells: &[String]| {
                        let s: Vec<String> = widths
                            .iter()
                            .enumerate()
                            .map(|(i, &w)| pad(&fit(cells.get(i).map_or("", String::as_str), w), w))
                            .collect();
                        s.join(" ").trim_end().to_owned()
                    };
                    lines.push(Line::styled(row_line(columns), theme.header));
                    let rule: Vec<String> = widths.iter().map(|&w| "-".repeat(w)).collect();
                    lines.push(Line::raw(rule.join(" ")));
                    lines.extend(rows.iter().map(|r| Line::raw(row_line(r))));
                    let footer = if *more {
                        format!("{} rows shown. F2 opens all rows in the grid.", rows.len())
                    } else {
                        format!("{} row(s) selected.", rows.len())
                    };
                    lines.push(Line::styled(footer, theme.muted));
                }
            }
            lines.push(Line::raw(""));
        }
        lines
    }
}

impl Component for Log {
    fn on_db(&mut self, ev: &DbEvent) {
        let entry = match ev {
            DbEvent::Connected(label) => Entry::Info(format!("Connected to {label}.")),
            DbEvent::Message(m) => Entry::Info(m.clone()),
            DbEvent::Error(e) => Entry::Error(e.clone()),
            DbEvent::Rows {
                columns,
                rows,
                more,
            } => Entry::Table {
                columns: columns.clone(),
                rows: rows.iter().take(LOG_ROWS).cloned().collect(),
                more: *more || rows.len() > LOG_ROWS,
            },
            _ => return,
        };
        self.push(entry);
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let lines = self.lines(theme);
        let height = area.height as usize;
        let max_top = lines.len().saturating_sub(height);
        let top = max_top.saturating_sub(self.scroll * height.div_ceil(2));
        // shortcut: lines wider than the screen are cut; scroll the grid for wide results.
        f.render_widget(Paragraph::new(lines).scroll((top as u16, 0)), area);
    }
}
