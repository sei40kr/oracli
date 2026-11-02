use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::buffer::TextBuffer;
use crate::completion::{Completer, Completion};
use crate::history::History;
use crate::keys::{Command, KeyBindings};
use crate::lexer::Lexer;

/// What [`PromptState::handle_key`] did with a key.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Handled,
    /// The user submitted this text. It is already in the history and the input is cleared.
    Submit(String),
    /// The prompt has no use for the key; the caller may act on it.
    Ignored,
}

pub(crate) struct CompletionState {
    pub(crate) items: Vec<Completion>,
    /// `None` while the menu only follows typing, so Enter still submits.
    pub(crate) selected: Option<usize>,
    /// The text and cursor from before a candidate was previewed in the buffer.
    original: (String, usize),
    /// Whether the undo checkpoint for this menu session is taken.
    previewed: bool,
    /// First item the menu shows; kept across frames so the list does not jump.
    pub(crate) offset: usize,
}

/// The completion request whose answer is still wanted.
struct Pending {
    id: u64,
    /// Tab asked for it: select the first candidate, or apply a lone one.
    explicit: bool,
}

/// Candidates computed on a completion thread, tagged with the request they answer.
type Answer = (u64, Vec<Completion>);

pub(crate) struct Search {
    pub(crate) query: String,
    pub(crate) found: Option<usize>,
}

/// The editing state behind [`Prompt`](crate::Prompt) and [`CompletionMenu`](crate::CompletionMenu):
/// text, history, completion and search, and the key handling that changes them.
pub struct PromptState {
    buffer: TextBuffer,
    bindings: KeyBindings,
    history: History,
    completer: Option<Arc<dyn Completer + Send + Sync>>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    answers: (Sender<Answer>, Receiver<Answer>),
    pending: Option<Pending>,
    next_id: u64,
    pub(crate) lexer: Option<Box<dyn Lexer>>,
    accept: Box<dyn Fn(&str) -> bool>,
    pub(crate) completion: Option<CompletionState>,
    pub(crate) search: Option<Search>,
    // Index of the history entry being shown, and the draft it replaced.
    history_pos: Option<(usize, String)>,
    kill_ring: String,
}

impl Default for PromptState {
    fn default() -> Self {
        Self {
            buffer: TextBuffer::new(),
            bindings: KeyBindings::emacs(),
            history: History::new(),
            completer: None,
            wake: None,
            answers: channel(),
            pending: None,
            next_id: 0,
            lexer: None,
            accept: Box::new(|_| true),
            completion: None,
            search: None,
            history_pos: None,
            kill_ring: String::new(),
        }
    }
}

impl PromptState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_history(mut self, history: History) -> Self {
        self.history = history;
        self
    }

    pub fn with_completer(mut self, completer: impl Completer + Send + Sync + 'static) -> Self {
        self.completer = Some(Arc::new(completer));
        self
    }

    /// Runs the completer on a thread of its own, so a slow one does not hold up typing.
    /// `wake` is called from that thread when candidates are ready; the caller then calls
    /// [`Self::poll_completions`] on its own thread. Answers to input that has since changed
    /// are dropped.
    pub fn complete_in_thread(mut self, wake: impl Fn() + Send + Sync + 'static) -> Self {
        self.wake = Some(Arc::new(wake));
        self
    }

    pub fn with_lexer(mut self, lexer: impl Lexer + 'static) -> Self {
        self.lexer = Some(Box::new(lexer));
        self
    }

    /// Decides whether [`Command::Accept`] submits the text or inserts a newline.
    pub fn with_accept(mut self, accept: impl Fn(&str) -> bool + 'static) -> Self {
        self.accept = Box::new(accept);
        self
    }

    pub fn bindings_mut(&mut self) -> &mut KeyBindings {
        &mut self.bindings
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Lines the prompt needs to show its whole text.
    pub fn height(&self) -> u16 {
        self.display_text().matches('\n').count() as u16 + 1
    }

    /// The open completion menu's candidates and the selected one, if any.
    pub fn completions(&self) -> Option<(&[Completion], Option<usize>)> {
        self.completion
            .as_ref()
            .map(|c| (c.items.as_slice(), c.selected))
    }

    /// Whether a completion request is waiting for its candidates.
    pub fn is_completing(&self) -> bool {
        self.pending.is_some()
    }

    /// Applies candidates that arrived from the completion thread.
    pub fn poll_completions(&mut self) {
        while let Ok((id, items)) = self.answers.1.try_recv() {
            self.answer(id, items);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if key.kind == KeyEventKind::Release {
            return Outcome::Handled;
        }
        // Whatever the key does, an earlier request no longer matches the input.
        self.pending = None;
        let cmd = self.bindings.lookup(&key);

        if self.search.is_some() {
            match self.handle_search_key(&key, cmd) {
                Some(outcome) => return outcome,
                None => self.finish_search(),
            }
        }

        if let Some(c) = &self.completion {
            let len = c.items.len();
            let selected = c.selected;
            match cmd {
                Some(Command::Complete) if selected.is_none() && len == 1 => {
                    self.select(Some(0));
                    self.completion = None;
                    return Outcome::Handled;
                }
                Some(Command::Complete | Command::MoveDown) => {
                    // Past the last candidate comes the original text, as in prompt_toolkit.
                    let next = match selected {
                        None => Some(0),
                        Some(i) if i + 1 < len => Some(i + 1),
                        Some(_) => None,
                    };
                    self.select(next);
                    return Outcome::Handled;
                }
                Some(Command::CompletePrev | Command::MoveUp) => {
                    let prev = match selected {
                        None => Some(len - 1),
                        Some(0) => None,
                        Some(i) => Some(i - 1),
                    };
                    self.select(prev);
                    return Outcome::Handled;
                }
                Some(Command::Accept) if selected.is_some() => {
                    self.completion = None;
                    return Outcome::Handled;
                }
                Some(Command::Cancel) => {
                    self.select(None);
                    self.completion = None;
                    return Outcome::Handled;
                }
                // Any other key keeps the previewed candidate and acts normally.
                _ => self.completion = None,
            }
        }

        let Some(cmd) = cmd else {
            return match key.code {
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.buffer.insert_char(c);
                    self.complete_while_typing();
                    Outcome::Handled
                }
                _ => Outcome::Ignored,
            };
        };
        let outcome = self.run(cmd);
        if cmd == Command::DeleteBackward {
            self.complete_while_typing();
        }
        outcome
    }

    fn run(&mut self, cmd: Command) -> Outcome {
        let b = &mut self.buffer;
        match cmd {
            Command::MoveLeft => b.move_left(),
            Command::MoveRight | Command::LineEnd | Command::BufferEnd
                if b.cursor() == b.text().len() =>
            {
                if let Some(s) = self.suggestion().map(str::to_owned) {
                    self.buffer.insert_str(&s);
                }
            }
            Command::MoveRight => b.move_right(),
            Command::MoveUp => {
                if !b.move_up() {
                    self.history_prev();
                }
            }
            Command::MoveDown => {
                if !b.move_down() {
                    self.history_next();
                }
            }
            Command::LineStart => b.set_cursor(b.line_start()),
            Command::LineEnd => b.set_cursor(b.line_end()),
            Command::BufferStart => b.set_cursor(0),
            Command::BufferEnd => b.set_cursor(b.text().len()),
            Command::WordLeft => b.set_cursor(b.word_start_before()),
            Command::WordRight => b.set_cursor(b.word_end_after()),
            Command::DeleteBackward => b.delete_backward(),
            Command::DeleteForward if b.is_empty() => return Outcome::Ignored,
            Command::DeleteForward => b.delete_forward(),
            Command::KillLineEnd => {
                let end = b.line_end();
                // At the end of a line, kill the newline itself, like Emacs.
                let end = if end == b.cursor() && end < b.text().len() {
                    end + 1
                } else {
                    end
                };
                self.kill_ring = b.delete_range(b.cursor(), end);
            }
            Command::KillLineStart => self.kill_ring = b.delete_range(b.line_start(), b.cursor()),
            Command::KillWordBackward => {
                self.kill_ring = b.delete_range(b.word_start_before(), b.cursor())
            }
            Command::KillWordForward => {
                self.kill_ring = b.delete_range(b.cursor(), b.word_end_after())
            }
            Command::Yank if !self.kill_ring.is_empty() => b.insert_str(&self.kill_ring),
            Command::Yank => {}
            Command::Undo => b.undo(),
            Command::Redo => b.redo(),
            Command::Accept if !(self.accept)(b.text()) => b.insert_char('\n'),
            Command::Accept | Command::ForceAccept => return self.submit(),
            Command::Newline => b.insert_char('\n'),
            Command::Complete | Command::CompletePrev => self.complete(),
            Command::HistorySearch => {
                self.search = Some(Search {
                    query: String::new(),
                    found: None,
                })
            }
            Command::Cancel => return Outcome::Ignored,
        }
        Outcome::Handled
    }

    /// Returns `None` when the key ends the search and should be handled normally.
    fn handle_search_key(&mut self, key: &KeyEvent, cmd: Option<Command>) -> Option<Outcome> {
        let search = self.search.as_mut()?;
        let len = self.history.entries().len();
        match (cmd, key.code) {
            (Some(Command::HistorySearch), _) => {
                let before = search.found.unwrap_or(len);
                if let Some(i) = self.history.search(&search.query, before) {
                    search.found = Some(i);
                }
            }
            (Some(Command::Cancel), _) => self.search = None,
            (Some(Command::DeleteBackward), _) => {
                search.query.pop();
                search.found = self.history.search(&search.query, len);
            }
            (Some(Command::Accept), _) => self.finish_search(),
            (None, KeyCode::Char(c))
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                search.query.push(c);
                search.found = self.history.search(&search.query, len);
            }
            _ => return None,
        }
        Some(Outcome::Handled)
    }

    fn finish_search(&mut self) {
        if let Some(Search { found: Some(i), .. }) = self.search.take() {
            let entry = self.history.entries()[i].clone();
            self.buffer.set_text(&entry);
        }
    }

    fn submit(&mut self) -> Outcome {
        let text = self.buffer.text().to_owned();
        // A failed history write must not block the submission.
        let _ = self.history.push(&text);
        self.buffer.reset();
        self.history_pos = None;
        Outcome::Submit(text)
    }

    fn history_prev(&mut self) {
        let entries = self.history.entries();
        let idx = self.history_pos.as_ref().map_or(entries.len(), |(i, _)| *i);
        if idx == 0 {
            return;
        }
        let entry = entries[idx - 1].clone();
        let draft = match self.history_pos.take() {
            Some((_, d)) => d,
            None => self.buffer.text().to_owned(),
        };
        self.history_pos = Some((idx - 1, draft));
        self.buffer.set_text(&entry);
    }

    fn history_next(&mut self) {
        let Some((idx, draft)) = self.history_pos.take() else {
            return;
        };
        match self.history.entries().get(idx + 1) {
            Some(entry) => {
                let entry = entry.clone();
                self.history_pos = Some((idx + 1, draft));
                self.buffer.set_text(&entry);
            }
            None => self.buffer.set_text(&draft),
        }
    }

    /// Asks the completer for the word at the cursor, on its thread when there is one.
    fn request(&mut self, explicit: bool) {
        let Some(completer) = self.completer.clone() else {
            return;
        };
        self.next_id += 1;
        let id = self.next_id;
        self.pending = Some(Pending { id, explicit });
        let text = self.buffer.text().to_owned();
        let cursor = self.buffer.cursor();
        match &self.wake {
            Some(wake) => {
                let (tx, wake) = (self.answers.0.clone(), wake.clone());
                std::thread::spawn(move || {
                    let _ = tx.send((id, completer.complete(&text, cursor)));
                    wake();
                });
            }
            None => {
                let items = completer.complete(&text, cursor);
                self.answer(id, items);
            }
        }
    }

    fn answer(&mut self, id: u64, mut items: Vec<Completion>) {
        let Some(pending) = self.pending.take_if(|p| p.id == id) else {
            return;
        };
        let text = self.buffer.text();
        let cursor = self.buffer.cursor();
        items.retain(|c| c.start <= cursor && text.is_char_boundary(c.start));
        if pending.explicit {
            match items.len() {
                0 => {}
                1 => self
                    .buffer
                    .replace_before_cursor(items[0].start, &items[0].text),
                _ => {
                    self.open_menu(items);
                    self.select(Some(0));
                }
            }
        } else {
            items.retain(|c| !c.text.eq_ignore_ascii_case(&text[c.start..cursor]));
            if !items.is_empty() {
                self.open_menu(items);
            }
        }
    }

    fn open_menu(&mut self, items: Vec<Completion>) {
        self.completion = Some(CompletionState {
            items,
            selected: None,
            original: (self.buffer.text().to_owned(), self.buffer.cursor()),
            previewed: false,
            offset: 0,
        });
    }

    /// Opens the menu with the first candidate previewed; a lone candidate is applied at once.
    fn complete(&mut self) {
        self.request(true);
    }

    /// Opens the menu without a selection; never applies a lone candidate.
    fn complete_while_typing(&mut self) {
        self.completion = None;
        let text = self.buffer.text();
        let cursor = self.buffer.cursor();
        // Only inside a word or after `.`, so separators do not list every candidate.
        if text[..cursor]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
        {
            self.request(false);
        }
    }

    /// Shows candidate `index` in the buffer, or the original text for `None`. The whole menu
    /// session is one undo step.
    fn select(&mut self, index: Option<usize>) {
        let Some(c) = &mut self.completion else {
            return;
        };
        let checkpoint = index.is_some() && !c.previewed;
        c.previewed |= checkpoint;
        c.selected = index;
        let (text, cursor) = &c.original;
        let (text, cursor) = match index {
            Some(i) => {
                let item = &c.items[i];
                let preview = format!("{}{}{}", &text[..item.start], item.text, &text[*cursor..]);
                (preview, item.start + item.text.len())
            }
            None => (text.clone(), *cursor),
        };
        if checkpoint {
            self.buffer.checkpoint();
        }
        self.buffer.overwrite(text, cursor);
    }

    /// Asks the completer again for the word at the cursor, for when its candidates changed
    /// without a key press. A menu with a selected candidate is left as it is.
    pub fn refresh_completions(&mut self) {
        let navigating = self
            .completion
            .as_ref()
            .is_some_and(|c| c.selected.is_some());
        if self.search.is_none() && !navigating {
            self.complete_while_typing();
        }
    }

    /// The rest of the newest history entry that extends the current text.
    pub fn suggestion(&self) -> Option<&str> {
        if self.search.is_some()
            || self
                .completion
                .as_ref()
                .is_some_and(|c| c.selected.is_some())
            || self.buffer.cursor() != self.buffer.text().len()
        {
            return None;
        }
        let text = self.buffer.text();
        self.history.suggest(text).map(|e| &e[text.len()..])
    }

    /// The text on screen: the search match while searching, else the buffer.
    pub(crate) fn display_text(&self) -> &str {
        match &self.search {
            Some(Search { found: Some(i), .. }) => &self.history.entries()[*i],
            _ => self.buffer.text(),
        }
    }

    /// Byte offset of the cursor in [`Self::display_text`].
    pub(crate) fn display_cursor(&self) -> usize {
        if self.search.is_some() {
            self.display_text().len()
        } else {
            self.buffer.cursor()
        }
    }
}
