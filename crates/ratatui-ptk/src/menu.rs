use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;
use unicode_width::UnicodeWidthStr;

use crate::state::PromptState;

/// The popup list of completion candidates. It floats over other widgets: place it with
/// [`CompletionMenu::area`] and render it after them.
pub struct CompletionMenu {
    style: Style,
    selected_style: Style,
    meta_style: Style,
    max_height: u16,
}

impl Default for CompletionMenu {
    fn default() -> Self {
        Self {
            style: Style::new().bg(Color::DarkGray).fg(Color::White),
            selected_style: Style::new().bg(Color::Gray).fg(Color::Black),
            meta_style: Style::new(),
            max_height: 8,
        }
    }
}

impl CompletionMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    /// Patched over the row style for the note beside each candidate.
    pub fn meta_style(mut self, style: Style) -> Self {
        self.meta_style = style;
        self
    }

    pub fn max_height(mut self, rows: u16) -> Self {
        self.max_height = rows;
        self
    }

    /// Where the menu goes inside `screen` for a prompt whose cursor is at `cursor`: lined up
    /// with the start of the word being completed, above the cursor when it fits, else below.
    /// `None` when no menu is open.
    pub fn area(&self, state: &PromptState, cursor: Position, screen: Rect) -> Option<Rect> {
        let c = state.completion.as_ref()?;
        let text_w = c.items.iter().map(|i| i.text.width()).max().unwrap_or(0);
        let meta_w = c
            .items
            .iter()
            .map(|i| i.meta.as_deref().map_or(0, str::width))
            .max()
            .unwrap_or(0);
        let width = ((text_w + meta_w + 3) as u16).min(screen.width);
        let height = (c.items.len() as u16).min(self.max_height);

        let typed = &state.buffer().text()[c.items[0].start..state.buffer().cursor()];
        let x = cursor
            .x
            .saturating_sub(typed.width() as u16 + 1)
            .clamp(screen.x, screen.right().saturating_sub(width));
        let above = cursor.y.saturating_sub(screen.y);
        let y = if above >= height {
            cursor.y - height
        } else {
            cursor.y + 1
        };
        let height = height.min(screen.bottom().saturating_sub(y));
        Some(Rect::new(x, y, width, height))
    }
}

impl StatefulWidget for CompletionMenu {
    type State = PromptState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut PromptState) {
        let Some(c) = &mut state.completion else {
            return;
        };
        let rows = area.height as usize;
        if rows == 0 {
            return;
        }
        // Scroll only as far as needed to keep the selection visible.
        if let Some(sel) = c.selected {
            if sel < c.offset {
                c.offset = sel;
            } else if sel >= c.offset + rows {
                c.offset = sel + 1 - rows;
            }
        }
        c.offset = c.offset.min(c.items.len().saturating_sub(rows));

        let visible = &c.items[c.offset..c.items.len().min(c.offset + rows)];
        let text_w = c.items.iter().map(|i| i.text.width()).max().unwrap_or(0);
        for (i, item) in visible.iter().enumerate() {
            let row_style = if c.selected == Some(c.offset + i) {
                self.selected_style
            } else {
                self.style
            };
            let y = area.y + i as u16;
            buf.set_style(Rect::new(area.x, y, area.width, 1), row_style);
            let text = format!(" {}", item.text);
            let (x, _) = buf.set_stringn(area.x, y, &text, area.width as usize, row_style);
            if let Some(meta) = &item.meta {
                let gap = (text_w - item.text.width() + 1) as u16;
                let meta_x = x + gap;
                if meta_x < area.right() {
                    buf.set_stringn(
                        meta_x,
                        y,
                        meta,
                        (area.right() - meta_x) as usize,
                        row_style.patch(self.meta_style),
                    );
                }
            }
        }
    }
}
