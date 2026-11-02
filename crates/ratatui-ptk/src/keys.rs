use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// An editing command a key can be bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    LineStart,
    LineEnd,
    BufferStart,
    BufferEnd,
    WordLeft,
    WordRight,
    DeleteBackward,
    DeleteForward,
    KillLineEnd,
    KillLineStart,
    KillWordBackward,
    KillWordForward,
    Yank,
    Undo,
    Redo,
    /// Submit if the accept handler agrees, otherwise insert a newline.
    Accept,
    /// Submit unconditionally.
    ForceAccept,
    Newline,
    Complete,
    CompletePrev,
    HistorySearch,
    Cancel,
}

/// Maps keys to commands. Unbound printable keys insert themselves.
#[derive(Debug, Clone, Default)]
pub struct KeyBindings {
    map: HashMap<(KeyCode, KeyModifiers), Command>,
}

impl KeyBindings {
    pub fn bind(&mut self, code: KeyCode, modifiers: KeyModifiers, command: Command) {
        self.map.insert((code, modifiers), command);
    }

    pub fn lookup(&self, key: &KeyEvent) -> Option<Command> {
        let mut modifiers = key.modifiers;
        // Shift is already reflected in the character itself.
        if matches!(key.code, KeyCode::Char(_)) {
            modifiers.remove(KeyModifiers::SHIFT);
        }
        self.map.get(&(key.code, modifiers)).copied()
    }

    /// Emacs-style bindings, close to readline's defaults.
    pub fn emacs() -> Self {
        use Command::*;
        use KeyCode::*;
        const N: KeyModifiers = KeyModifiers::NONE;
        const C: KeyModifiers = KeyModifiers::CONTROL;
        const A: KeyModifiers = KeyModifiers::ALT;
        const S: KeyModifiers = KeyModifiers::SHIFT;

        let mut kb = Self::default();
        for (code, m, cmd) in [
            (Left, N, MoveLeft),
            (Char('b'), C, MoveLeft),
            (Right, N, MoveRight),
            (Char('f'), C, MoveRight),
            (Up, N, MoveUp),
            (Char('p'), C, MoveUp),
            (Down, N, MoveDown),
            (Char('n'), C, MoveDown),
            (Home, N, LineStart),
            (Char('a'), C, LineStart),
            (End, N, LineEnd),
            (Char('e'), C, LineEnd),
            (Char('<'), A, BufferStart),
            (Char('>'), A, BufferEnd),
            (Char('b'), A, WordLeft),
            (Left, C, WordLeft),
            (Char('f'), A, WordRight),
            (Right, C, WordRight),
            (Backspace, N, DeleteBackward),
            (Char('h'), C, DeleteBackward),
            (Delete, N, DeleteForward),
            (Char('d'), C, DeleteForward),
            (Char('k'), C, KillLineEnd),
            (Char('u'), C, KillLineStart),
            (Char('w'), C, KillWordBackward),
            (Backspace, A, KillWordBackward),
            (Char('d'), A, KillWordForward),
            (Char('y'), C, Yank),
            (Char('_'), C, Undo),
            (Char('/'), C, Undo),
            (Char('7'), C, Undo), // what many terminals send for C-_
            (Char('_'), A, Redo),
            (Enter, N, Accept),
            (Char('j'), C, Accept),
            (Enter, A, ForceAccept),
            (Char('o'), C, Newline),
            (Tab, N, Complete),
            (BackTab, S, CompletePrev),
            (BackTab, N, CompletePrev),
            (Char('r'), C, HistorySearch),
            (Esc, N, Cancel),
            (Char('g'), C, Cancel),
        ] {
            kb.bind(code, m, cmd);
        }
        kb
    }
}
