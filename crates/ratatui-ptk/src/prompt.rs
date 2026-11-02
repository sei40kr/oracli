use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget};
use unicode_width::UnicodeWidthStr;

use crate::state::PromptState;

fn plain(_class: &str) -> Style {
    Style::new()
}

/// Draws a [`PromptState`]: the prompt string, the highlighted text, and the history
/// auto-suggestion. While searching history it shows the search query and the match instead.
pub struct Prompt<'a> {
    prompt: &'a str,
    style: Style,
    prompt_style: Style,
    suggestion_style: Style,
    syntax: &'a dyn Fn(&str) -> Style,
}

impl<'a> Prompt<'a> {
    pub fn new(prompt: &'a str) -> Self {
        Self {
            prompt,
            style: Style::new(),
            prompt_style: Style::new().add_modifier(Modifier::BOLD),
            suggestion_style: Style::new().fg(Color::DarkGray),
            syntax: &plain,
        }
    }

    /// Base style of the whole area, under the syntax styles.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Style of the prompt string and the history search prefix.
    pub fn prompt_style(mut self, style: Style) -> Self {
        self.prompt_style = style;
        self
    }

    pub fn suggestion_style(mut self, style: Style) -> Self {
        self.suggestion_style = style;
        self
    }

    /// Maps the token classes from the state's [`Lexer`](crate::Lexer) to styles.
    pub fn syntax(mut self, syntax: &'a dyn Fn(&str) -> Style) -> Self {
        self.syntax = syntax;
        self
    }

    fn first_prefix(&self, state: &PromptState) -> String {
        match &state.search {
            Some(s) => format!("(reverse-i-search)`{}': ", s.query),
            None => self.prompt.to_owned(),
        }
    }

    /// Scroll offsets and the cursor's position relative to `area`, before scrolling.
    fn layout(&self, state: &PromptState, area: Rect) -> (u16, u16, u16, u16) {
        let text = state.display_text();
        let cursor = state.display_cursor();
        let row = text[..cursor].matches('\n').count();
        let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let col = self.first_prefix(state).width() + text[line_start..cursor].width();
        let vscroll = (row + 1).saturating_sub(area.height as usize);
        let hscroll = (col + 1).saturating_sub(area.width as usize);
        (vscroll as u16, hscroll as u16, row as u16, col as u16)
    }

    /// Where to place the terminal cursor when `state` is rendered into `area`.
    pub fn cursor_position(&self, state: &PromptState, area: Rect) -> Position {
        let (vscroll, hscroll, row, col) = self.layout(state, area);
        Position::new(area.x + col - hscroll, area.y + row - vscroll)
    }
}

impl StatefulWidget for Prompt<'_> {
    type State = PromptState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut PromptState) {
        let text = state.display_text();
        let mut styles = vec![Style::new(); text.len()];
        if let Some(lexer) = &state.lexer {
            for (range, class) in lexer.lex(text) {
                let end = range.end.min(text.len());
                styles[range.start.min(end)..end].fill((self.syntax)(class));
            }
        }

        let first_prefix = self.first_prefix(state);
        let continuation = " ".repeat(first_prefix.width());
        let mut lines = Vec::new();
        let mut offset = 0;
        for (n, line) in text.split('\n').enumerate() {
            let prefix = if n == 0 {
                first_prefix.clone()
            } else {
                continuation.clone()
            };
            let mut spans = vec![Span::styled(prefix, self.prompt_style)];
            let mut start = 0;
            for (i, _) in line.char_indices().skip(1) {
                if styles[offset + i] != styles[offset + start] {
                    spans.push(Span::styled(&line[start..i], styles[offset + start]));
                    start = i;
                }
            }
            if start < line.len() {
                spans.push(Span::styled(&line[start..], styles[offset + start]));
            }
            lines.push(Line::from(spans));
            offset += line.len() + 1;
        }
        if let Some(s) = state.suggestion() {
            let s = s.split('\n').next().unwrap_or("");
            if let Some(last) = lines.last_mut() {
                last.push_span(Span::styled(s, self.suggestion_style));
            }
        }

        // shortcut: long lines scroll horizontally instead of soft-wrapping; wrap if users ask.
        let (vscroll, hscroll, _, _) = self.layout(state, area);
        Paragraph::new(lines)
            .style(self.style)
            .scroll((vscroll, hscroll))
            .render(area, buf);
    }
}
