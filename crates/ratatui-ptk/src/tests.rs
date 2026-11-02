use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;

use crate::*;

fn key(code: KeyCode, m: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, m)
}

fn type_str(p: &mut PromptState, s: &str) {
    for c in s.chars() {
        p.handle_key(key(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

/// Renders the prompt at the bottom of a `w`×`h` screen with the menu floating over it.
fn render(p: &mut PromptState, w: u16, h: u16) -> Buffer {
    let screen = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(screen);
    let area = Rect::new(0, h - p.height().min(h), w, p.height().min(h));
    let prompt = Prompt::new("> ");
    let cursor = prompt.cursor_position(p, area);
    prompt.render(area, &mut buf, p);
    let menu = CompletionMenu::new();
    if let Some(menu_area) = menu.area(p, cursor, screen) {
        menu.render(menu_area, &mut buf, p);
    }
    buf
}

fn lines(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

struct Words;
impl Completer for Words {
    fn complete(&self, text: &str, cursor: usize) -> Vec<Completion> {
        let start = text[..cursor].rfind(' ').map_or(0, |i| i + 1);
        let word = &text[start..cursor];
        ["select", "sequence", "set"]
            .into_iter()
            .filter(|w| w.starts_with(word))
            .map(|w| Completion::new(w, start).with_meta("kw"))
            .collect()
    }
}

#[test]
fn accept_handler_decides_between_submit_and_newline() {
    let mut p = PromptState::new().with_accept(|t| t.ends_with(';'));
    type_str(&mut p, "select 1");
    assert_eq!(
        p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        Outcome::Handled
    );
    type_str(&mut p, "from dual;");
    assert_eq!(
        p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        Outcome::Submit("select 1\nfrom dual;".into())
    );
    assert!(p.buffer().is_empty());
    type_str(&mut p, "x");
    assert_eq!(
        p.handle_key(key(KeyCode::Enter, KeyModifiers::ALT)),
        Outcome::Submit("x".into())
    );
}

#[test]
fn ctrl_d_on_empty_input_is_left_to_the_caller() {
    let mut p = PromptState::new();
    let ctrl_d = key(KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(p.handle_key(ctrl_d), Outcome::Ignored);
    type_str(&mut p, "a");
    p.handle_key(key(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(p.handle_key(ctrl_d), Outcome::Handled);
    assert!(p.buffer().is_empty());
}

#[test]
fn history_navigation_restores_draft() {
    let mut p = PromptState::new();
    for s in ["one", "two"] {
        type_str(&mut p, s);
        p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    }
    type_str(&mut p, "dra");
    p.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "two");
    p.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
    p.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "one");
    p.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    p.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "dra");
}

#[test]
fn suggestion_is_shown_and_accepted() {
    let mut p = PromptState::new();
    type_str(&mut p, "select 1;");
    p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    type_str(&mut p, "sel");
    assert_eq!(p.suggestion(), Some("ect 1;"));
    assert_eq!(lines(&render(&mut p, 20, 1)), ["> select 1;         "]);
    p.handle_key(key(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(p.buffer().text(), "select 1;");
}

#[test]
fn reverse_search_loads_match_into_buffer() {
    let mut p = PromptState::new();
    for s in ["select a;", "drop b;", "select c;"] {
        type_str(&mut p, s);
        p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    }
    p.handle_key(key(KeyCode::Char('r'), KeyModifiers::CONTROL));
    type_str(&mut p, "sel");
    p.handle_key(key(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(
        lines(&render(&mut p, 40, 1)),
        ["(reverse-i-search)`sel': select a;      "]
    );
    p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "select a;");
}

#[test]
fn selecting_a_candidate_previews_it_and_escape_restores() {
    let mut p = PromptState::new().with_completer(Words);
    type_str(&mut p, "x se");
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "x sequence");
    assert_eq!(
        lines(&render(&mut p, 20, 4)),
        [
            "    select   kw     ",
            "    sequence kw     ",
            "    set      kw     ",
            "> x sequence        ",
        ]
    );
    p.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "x se");
    assert_eq!(p.completions(), None);
}

#[test]
fn enter_keeps_the_previewed_candidate_and_undo_restores_the_typed_text() {
    let mut p = PromptState::new().with_completer(Words);
    type_str(&mut p, "se");
    for _ in 0..4 {
        p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    }
    // select, sequence, set, back to the typed text, then select again.
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "select");
    p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "select");
    assert_eq!(p.completions(), None);
    p.handle_key(key(KeyCode::Char('_'), KeyModifiers::CONTROL));
    assert_eq!(p.buffer().text(), "se");
}

#[test]
fn menu_follows_typing_without_stealing_enter() {
    let mut p = PromptState::new().with_completer(Words);
    type_str(&mut p, "se");
    assert_eq!(
        lines(&render(&mut p, 20, 4)),
        [
            "  select   kw       ",
            "  sequence kw       ",
            "  set      kw       ",
            "> se                ",
        ]
    );
    type_str(&mut p, "l");
    assert_eq!(lines(&render(&mut p, 20, 2))[0], "  select kw         ");
    type_str(&mut p, "ect");
    assert_eq!(lines(&render(&mut p, 20, 2))[0], "                    ");
    p.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(lines(&render(&mut p, 20, 2))[0], "  select kw         ");
    assert_eq!(
        p.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        Outcome::Submit("selec".into())
    );
}

#[test]
fn single_completion_applies_immediately() {
    let mut p = PromptState::new().with_completer(Words);
    type_str(&mut p, "x sel");
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(p.buffer().text(), "x select");
}

#[test]
fn menu_scrolls_to_keep_the_selection_visible() {
    let mut p = PromptState::new().with_completer(Words);
    type_str(&mut p, "s");
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    let screen = Rect::new(0, 0, 20, 3);
    let mut buf = Buffer::empty(screen);
    let menu = CompletionMenu::new().max_height(2);
    let area = menu.area(&p, Position::new(3, 2), screen).unwrap();
    menu.render(area, &mut buf, &mut p);
    assert_eq!(
        lines(&buf)[..2],
        [" sequence kw        ", " set      kw        "]
    );
}

#[test]
fn styles_come_from_the_caller() {
    struct Classes;
    impl Lexer for Classes {
        fn lex(&self, text: &str) -> Vec<(std::ops::Range<usize>, &'static str)> {
            vec![(0..text.len().min(3), "keyword")]
        }
    }
    let mut p = PromptState::new().with_lexer(Classes);
    type_str(&mut p, "sel x");
    let syntax = |class: &str| match class {
        "keyword" => Style::new().fg(Color::Blue),
        _ => Style::new(),
    };
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
    Prompt::new("> ")
        .prompt_style(Style::new().fg(Color::Red))
        .syntax(&syntax)
        .render(buf.area, &mut buf, &mut p);
    assert_eq!(buf[(0, 0)].fg, Color::Red);
    assert_eq!(buf[(2, 0)].fg, Color::Blue);
    assert_eq!(buf[(6, 0)].fg, Color::Reset);
}

#[test]
fn kill_and_yank() {
    let mut p = PromptState::new();
    type_str(&mut p, "select foo");
    p.handle_key(key(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(p.buffer().text(), "select ");
    p.handle_key(key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    p.handle_key(key(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(p.buffer().text(), "fooselect ");
}

#[test]
fn cursor_position_accounts_for_wide_chars_and_lines() {
    let mut p = PromptState::new();
    type_str(&mut p, "a\nあい");
    assert_eq!(
        Prompt::new("SQL> ").cursor_position(&p, Rect::new(0, 5, 20, 2)),
        Position::new(9, 6)
    );
}

#[test]
fn refresh_shows_candidates_that_arrived_after_the_key() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Late(Arc<AtomicBool>);
    impl Completer for Late {
        fn complete(&self, _text: &str, cursor: usize) -> Vec<Completion> {
            if !self.0.load(Ordering::Relaxed) {
                return vec![];
            }
            ["sys.dual", "sys.dba_users"]
                .into_iter()
                .map(|w| Completion::new(w, cursor - 4))
                .collect()
        }
    }
    let loaded = Arc::new(AtomicBool::new(false));
    let mut p = PromptState::new().with_completer(Late(loaded.clone()));
    type_str(&mut p, "sys.");
    assert!(p.completions().is_none());
    loaded.store(true, Ordering::Relaxed);
    p.refresh_completions();
    assert_eq!(
        p.completions().map(|(c, sel)| (c.len(), sel)),
        Some((2, None))
    );
}

#[test]
fn threaded_completer_answers_through_poll_and_drops_stale_answers() {
    use std::sync::Mutex;
    use std::sync::mpsc::channel;
    /// Candidates that tell which input they answer.
    struct Echo;
    impl Completer for Echo {
        fn complete(&self, text: &str, cursor: usize) -> Vec<Completion> {
            let word = &text[..cursor];
            vec![
                Completion::new(format!("{word}1"), 0),
                Completion::new(format!("{word}2"), 0),
            ]
        }
    }
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let mut p = PromptState::new()
        .with_completer(Echo)
        .complete_in_thread(move || tx.lock().unwrap().send(()).unwrap());
    let texts = |p: &PromptState| {
        p.completions()
            .map(|(c, _)| c.iter().map(|c| c.text.clone()).collect::<Vec<_>>())
    };

    type_str(&mut p, "se");
    assert!(p.is_completing());
    assert_eq!(texts(&p), None);
    // Both threads have answered; whatever the order, only the one for "se" counts.
    rx.recv().unwrap();
    rx.recv().unwrap();
    p.poll_completions();
    assert!(!p.is_completing());
    assert_eq!(texts(&p), Some(vec!["se1".into(), "se2".into()]));

    // Tab answers through the thread too, and previews the first candidate.
    p.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    p.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    rx.recv().unwrap();
    p.poll_completions();
    assert_eq!(p.buffer().text(), "se1");

    // A key that changes nothing but the cursor still drops the request in flight.
    type_str(&mut p, "x");
    p.handle_key(key(KeyCode::Left, KeyModifiers::NONE));
    rx.recv().unwrap();
    p.poll_completions();
    assert_eq!(texts(&p), None);
}
