use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui_ptk::{CompletionMenu, History, Outcome, Prompt, PromptState};

use super::Component;
use crate::app::Action;
use crate::catalog::{SharedCatalog, SqlCompleter};
use crate::db::DbEvent;
use crate::sql::{SqlLexer, is_complete};
use crate::theme::Theme;

/// The SQL prompt at the bottom of the screen.
pub struct Input {
    prompt: PromptState,
}

impl Input {
    /// `wake` runs on the completion thread when candidates are ready for [`Self::poll_completions`].
    pub fn new(
        history: History,
        catalog: SharedCatalog,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            prompt: PromptState::new()
                .with_history(history)
                .with_lexer(SqlLexer)
                .with_completer(SqlCompleter(catalog))
                .complete_in_thread(wake)
                .with_accept(is_complete),
        }
    }

    pub fn poll_completions(&mut self) {
        self.prompt.poll_completions();
    }

    #[cfg(test)]
    pub fn is_completing(&self) -> bool {
        self.prompt.is_completing()
    }

    pub fn height(&self) -> u16 {
        self.prompt.height()
    }

    pub fn paste(&mut self, s: &str) {
        self.prompt
            .buffer_mut()
            .insert_str(&s.replace("\r\n", "\n").replace('\r', "\n"));
    }

    pub fn clear(&mut self) {
        self.prompt.buffer_mut().reset();
    }
}

impl Component for Input {
    fn on_db(&mut self, ev: &DbEvent) {
        // The completer may have queued this schema for the word being typed.
        if matches!(ev, DbEvent::Schema { .. }) {
            self.prompt.refresh_completions();
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match self.prompt.handle_key(key) {
            Outcome::Submit(text) => vec![Action::Submit(text)],
            Outcome::Handled => vec![],
            Outcome::Ignored => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('c') if ctrl => vec![Action::Interrupt],
                    KeyCode::Char('d') if ctrl => vec![Action::Quit],
                    KeyCode::PageUp => vec![Action::ScrollLog(1)],
                    KeyCode::PageDown => vec![Action::ScrollLog(-1)],
                    KeyCode::F(2) => vec![Action::OpenGrid],
                    KeyCode::F(5) => vec![Action::RefreshCatalog],
                    _ => vec![],
                }
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let syntax = |class: &str| theme.syntax(class);
        let prompt = Prompt::new("SQL> ")
            .prompt_style(theme.prompt)
            .suggestion_style(theme.suggestion)
            .syntax(&syntax);
        let cursor = prompt.cursor_position(&self.prompt, area);
        f.render_stateful_widget(prompt, area, &mut self.prompt);
        f.set_cursor_position(cursor);

        let menu = CompletionMenu::new()
            .style(theme.menu)
            .selected_style(theme.menu_selected)
            .meta_style(theme.menu_meta);
        if let Some(menu_area) = menu.area(&self.prompt, cursor, f.area()) {
            f.render_stateful_widget(menu, menu_area, &mut self.prompt);
        }
    }
}
