# ratatui-ptk

A [prompt_toolkit](https://github.com/prompt-toolkit/python-prompt-toolkit)-style input line for [ratatui](https://ratatui.rs): multi-line editing, history, completion and highlighting. It renders into a ratatui `Buffer` and leaves the terminal and the event loop to you. It was written for [oracli](../../README.md) but knows nothing about SQL.

## Features

- Multi-line text with undo/redo grouped by word, and a kill ring
- Emacs key bindings
- File-backed history with Ctrl-R reverse incremental search and fish-style suggestions
- A completion menu that follows typing, previews the selected candidate in place, and restores the input on Esc; one menu session is one undo step
- Completion on a background thread, with stale answers dropped
- Highlighting through a `Lexer` you supply
- A hook that decides whether Enter submits or inserts a newline
- Every style set by builder methods; the defaults are plain ANSI colors

## Parts

| Type | Role |
|---|---|
| `PromptState` | The model: text buffer, history, completion and search state. Handles keys. |
| `Prompt` | `StatefulWidget` that draws the prompt, the highlighted text and the history suggestion, or the search line. |
| `CompletionMenu` | `StatefulWidget` for the popup list, placed at the word being completed. Render it last. |
| `Completer` | Trait you implement to offer `Completion`s. |
| `Lexer` | Trait you implement to tag byte ranges with token classes such as `"keyword"`. |
| `History` | In-memory or file-backed history. |
| `KeyBindings` | Maps keys to editing `Command`s. |

## Usage

```toml
[dependencies]
ratatui-ptk = { git = "https://github.com/sei40kr/oracli" }
```

Build the state once:

```rust
use ratatui_ptk::{Completer, Completion, History, Outcome, PromptState};

struct Words;

impl Completer for Words {
    fn complete(&self, text: &str, cursor: usize) -> Vec<Completion> {
        let start = text[..cursor].rfind(' ').map_or(0, |i| i + 1);
        ["select", "from", "where"]
            .iter()
            .filter(|w| w.starts_with(&text[start..cursor]))
            .map(|w| Completion::new(*w, start))
            .collect()
    }
}

let mut state = PromptState::new()
    .with_history(History::with_file("history")?)
    .with_completer(Words)
    .with_accept(|text| text.ends_with(';')); // Enter inserts a newline otherwise
```

Feed it keys from your event loop. Keys the prompt has no use for, such as Ctrl-C or Ctrl-D on an empty input, come back as `Ignored`:

```rust
match state.handle_key(key) {
    Outcome::Submit(text) => run(text), // already in the history; the input is cleared
    Outcome::Handled => {}
    Outcome::Ignored => { /* your own bindings */ }
}
```

Draw the prompt, then the menu over everything else:

```rust
use ratatui_ptk::{CompletionMenu, Prompt};

let prompt = Prompt::new("> ");
let cursor = prompt.cursor_position(&state, area);
frame.render_stateful_widget(prompt, area, &mut state);
frame.set_cursor_position(cursor);

let menu = CompletionMenu::new();
if let Some(menu_area) = menu.area(&state, cursor, frame.area()) {
    frame.render_stateful_widget(menu, menu_area, &mut state);
}
```

`PromptState::height` tells you how many rows the prompt needs for its current text.

### Completing on a thread

A slow completer should not block typing. `complete_in_thread` runs each request on a thread and calls `wake` when the answer is ready; send yourself an event from it and call `poll_completions` on the UI side:

```rust
let state = PromptState::new()
    .with_completer(Words)
    .complete_in_thread(move || { let _ = tx.send(Event::CompletionsReady); });

// in the event loop
Event::CompletionsReady => state.poll_completions(),
```

If the completer's data changes while the menu is open (say, metadata finished loading), `refresh_completions` asks again for the same input.

### Highlighting

Implement `Lexer` to tag byte ranges with class names, and map the names to styles when drawing:

```rust
let syntax = |class: &str| match class {
    "keyword" => Style::new().blue().bold(),
    _ => Style::new(),
};
let prompt = Prompt::new("> ").syntax(&syntax);
```

## Key bindings

Only an Emacs set ships, from `KeyBindings::emacs`. `PromptState::bindings_mut` lets you bind more keys to the built-in `Command`s.

## License

[MIT](../../LICENSE)
