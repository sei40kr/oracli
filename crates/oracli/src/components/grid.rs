use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::widgets::{Block, Row, Table, TableState};

use super::{Component, column_widths, fit};
use crate::app::Action;
use crate::db::DbEvent;
use crate::theme::Theme;

/// Full-screen view of the latest result set. Fetches more rows when the selection reaches
/// the end of what is loaded.
#[derive(Default)]
pub struct Grid {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    more: bool,
    loading: bool,
    selected: usize,
    first_col: usize,
    // Rows visible at the last draw, for paging.
    page: usize,
    max_cell_width: usize,
}

impl Grid {
    pub fn new(max_cell_width: usize) -> Self {
        Self {
            max_cell_width,
            ..Self::default()
        }
    }

    pub fn has_result(&self) -> bool {
        !self.columns.is_empty()
    }

    /// Forgets an in-flight fetch whose task was aborted.
    pub fn cancel_fetch(&mut self) {
        self.loading = false;
        self.more = false;
    }
}

impl Component for Grid {
    fn handle_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let last = self.rows.len().saturating_sub(1);
        let page = self.page.max(1);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::F(2) => return vec![Action::CloseGrid],
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.selected = (self.selected + page).min(last)
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = last,
            KeyCode::Right | KeyCode::Char('l') => {
                self.first_col = (self.first_col + 1).min(self.columns.len().saturating_sub(1))
            }
            KeyCode::Left | KeyCode::Char('h') => self.first_col = self.first_col.saturating_sub(1),
            _ => return vec![],
        }
        if self.more && !self.loading && self.selected + 1 >= self.rows.len() {
            self.loading = true;
            return vec![Action::FetchMore];
        }
        vec![]
    }

    fn on_db(&mut self, ev: &DbEvent) {
        match ev {
            DbEvent::Rows {
                columns,
                rows,
                more,
            } => {
                *self = Self {
                    columns: columns.clone(),
                    rows: rows.clone(),
                    more: *more,
                    ..Self::new(self.max_cell_width)
                };
            }
            DbEvent::MoreRows { rows, more } => {
                self.rows.extend(rows.iter().cloned());
                self.more = *more;
                self.loading = false;
            }
            DbEvent::Error(_) => self.loading = false,
            _ => {}
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let widths = column_widths(&self.columns, &self.rows, self.max_cell_width);
        let visible = self.first_col..self.columns.len();
        let row = |cells: &[String]| {
            Row::new(
                visible
                    .clone()
                    .map(|i| fit(cells.get(i).map_or("", String::as_str), widths[i])),
            )
        };
        let status = match (self.more, self.loading) {
            (_, true) => " (fetching…)",
            (true, false) => "+",
            _ => "",
        };
        let title = format!(
            " Row {}/{}{} · col {}/{} · q: close ",
            self.selected + 1,
            self.rows.len(),
            status,
            self.first_col + 1,
            self.columns.len()
        );
        let table = Table::new(
            self.rows.iter().map(|r| row(r)),
            visible
                .clone()
                .map(|i| Constraint::Length(widths[i] as u16)),
        )
        .header(row(&self.columns).style(theme.header))
        .row_highlight_style(theme.grid_selected)
        .block(Block::bordered().title(title));
        // Borders and the header take three rows.
        self.page = area.height.saturating_sub(3) as usize;
        let mut state = TableState::new().with_selected(Some(self.selected));
        f.render_stateful_widget(table, area, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(g: &mut Grid, code: KeyCode) -> Vec<Action> {
        g.handle_key(KeyEvent::from(code))
    }

    #[test]
    fn requests_more_rows_once_at_the_end() {
        let mut g = Grid::new(40);
        g.on_db(&DbEvent::Rows {
            columns: vec!["X".into()],
            rows: vec![vec!["1".into()], vec!["2".into()]],
            more: true,
        });
        assert!(press(&mut g, KeyCode::Down) == [Action::FetchMore]);
        assert!(press(&mut g, KeyCode::Down).is_empty());
        g.on_db(&DbEvent::MoreRows {
            rows: vec![vec!["3".into()]],
            more: false,
        });
        assert!(press(&mut g, KeyCode::End).is_empty());
        assert_eq!(g.selected, 2);
    }
}
