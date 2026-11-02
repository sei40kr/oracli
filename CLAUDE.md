# oracli

A rich terminal client for Oracle Database, written in Rust on ratatui.
It is a REPL: an input box at the bottom, a scrolling log of results above it.

## Workspace

One Cargo workspace, three crates:

| Crate | Role |
|-------|------|
| `crates/ratatui-ptk` | prompt_toolkit-like input library built on ratatui. Knows nothing about SQL or Oracle. |
| `crates/tree-sitter-plsql` | tree-sitter grammar for Oracle SQL and PL/SQL. Grown here, meant to be usable from editors too. |
| `crates/oracli` | The TUI application. Depends on the other two and `oracle-rs`. |

Keep the dependencies one-way: `oracli` → `ratatui-ptk`, `oracli` → `tree-sitter-plsql`. Anything SQL-specific (lexer, completer, statement terminator rules) lives in `oracli` and plugs into `ratatui-ptk` through its traits.

## ratatui-ptk

Parts that render into a ratatui `Buffer`; it does not own the terminal or the event loop. Laid out like prompt_toolkit: state and behavior in one place, widgets that only draw it.

- **PromptState**: the model, like prompt_toolkit's `Buffer`. Holds the `TextBuffer` (multi-line text, cursor, undo/redo), history, completion and search state, kill ring, and handles keys. `handle_key` returns `Outcome::{Handled, Submit, Ignored}`; `Ignored` keys (Ctrl-C, Ctrl-D on empty input, ...) are the caller's to act on.
- **Prompt**: `StatefulWidget` that draws the prompt string, the highlighted text and the history auto-suggestion, or the Ctrl-R search line while searching. `cursor_position` tells the caller where the terminal cursor goes.
- **CompletionMenu**: `StatefulWidget` for the popup list, like prompt_toolkit's `CompletionsMenu` in a `Float`. `area` places it at the word being completed, above the cursor when it fits; the caller renders it last, over other widgets.
- **Completion**: `Completer` trait. Selecting a candidate previews it in the text; Esc restores what was typed, any other key keeps the candidate, and one menu session is one undo step. While typing, the menu follows with nothing selected, so Enter still submits; Tab selects. `complete_in_thread` runs the completer on a thread per request, like prompt_toolkit's `ThreadedCompleter`: a wake callback tells the caller to `poll_completions`, and answers to input that has changed since are dropped.
- **Key bindings**: a registry mapping keys to editing commands. Only an Emacs set ships; the registry exists so a Vi set can be added later. No user remapping.
- **Highlighting**: `Lexer` trait that tags byte ranges with token classes (`"keyword"`, ...); `Prompt::syntax` maps classes to styles.
- **History**: file-backed history with reverse incremental search (Ctrl-R).
- **Accept handler**: a hook that decides whether Enter submits or inserts a newline.
- **Styles**: widgets take every style through builder methods. The defaults are plain ANSI colors; there is no theme in the library.

## oracli architecture

Event driven, component based, on tokio.

```
input thread ──┐
tick interval ─┼─► mpsc<Event> ─► App ─► Component::handle_key / on_db ─► Vec<Action>
DB task ───────┘                   ▲                                         │
                                   └─────────────── App::dispatch ◄──────────┘
```

- `Event`: terminal input (read on a std thread), tick, resize, paste, completions ready, and `DbEvent`s.
- `Component` (`src/components/`): owns its state; `handle_key` runs only for the focused component, `on_db` runs for all. Returns `Action`s and draws itself. Components never call each other or the database directly.
- `Action`: what the app should do (execute SQL, fetch more, open grid, quit, ...). `App` dispatches actions, routes focus, and sends DB requests.
- **DB task** (`src/db.rs`): a single tokio task owns the `oracle_rs::Connection`, receives `Request`s over an mpsc channel and sends `DbEvent`s back, ending every request with `DbEvent::Idle`. The UI never awaits the database.
- **Catalog** (`src/catalog.rs`): shared with the completer, which runs on its own thread, through `Arc<Mutex<_>>`. The completer cannot send requests, so it queues wanted schemas and the app drains them after each event.

### Components (MVP)

- **Log**: scrollback of executed statements and results. A query result shows its first 20 rows. PageUp/PageDown scroll.
- **Input**: `ratatui-ptk` prompt with the SQL lexer and the metadata completer. Keys the prompt ignores become actions: Ctrl-C interrupt, Ctrl-D quit, F2 grid, F5 refresh the catalog.
- **Grid**: full-screen view of the latest result (F2). Scrolls in both directions; reaching the end fetches the next page.
- **Status line**: connection, transaction state, running/idle.

Every style lives in `src/theme.rs`: SQL token classes, the prompt and menu, the log, the grid and the status line, in ANSI colors so the terminal palette decides the shades. `NO_COLOR` switches to a monochrome theme of bold, dim and reverse.

### Input and execution

SQLcl rules decide when Enter submits:

- The input ends with `;`, or with a line containing only `/` (needed after PL/SQL blocks): execute.
- Otherwise: insert a newline.
- Alt-Enter: always execute.

One submission is one statement. Non-SQL commands, matched case-insensitively like SQLcl:

- `EXIT` / `QUIT`
- `CONNECT user[/pass][@connect]`
- `DESC [schema.]object`: always queried from `ALL_TAB_COLUMNS`, so it never shows stale columns.

`CONNECT` needs the password inline; there is no masked prompt inside the TUI. Exiting with uncommitted DML (tracked by statement keyword) warns once and needs a second exit.

### Cancellation

`oracle-rs` has no break or call timeout. Ctrl-C while a statement runs aborts the DB task's future, drops the connection (its protocol state is unknown), and reconnects. Show a warning that any uncommitted transaction was lost.

### Metadata completion

- Right after connecting, load the user list and the current schema's tables, views and columns in the background.
- Synonyms (PUBLIC and the current schema's) load with it and complete where a table name goes; columns resolve through the synonym's target, loading its schema on demand (`dual.` loads SYS).
- A schema that finishes loading refreshes the open completion, so `SCHEMA.` shows its candidates without another key.
- When the user types `SCHEMA.`, load that schema from `ALL_*` views on demand and cache it.
- The cache is not invalidated automatically after DDL; F5 reloads the current schema.
- Keywords come from a static list in `oracli`.
- Candidates follow the parse tree. A placeholder identifier is inserted at the cursor so partial input still parses; where it lands decides the candidates: a table name gets tables and schemas, an alias gets keywords, a column gets columns and keywords. Columns come from every table referenced in the statement, and `alias.` resolves through the tree.

### tree-sitter-plsql

- `grammar.js` is the source; `src/parser.c`, `src/grammar.json` and `src/node-types.json` are generated by `tree-sitter generate` and committed, so building needs only a C compiler.
- Keywords are `keyword_*` nodes, case-insensitive and contextual: where the grammar expects no keyword they lex as identifiers.
- `src/scanner.c` lexes what a regular expression cannot: `q'[...]'` strings and the `/` line that ends a statement.
- The grammar is deliberately tolerant: the last statement may lack a terminator and an unterminated string runs to the end, because oracli parses input while it is typed.
- oracli uses the tree for everything SQL-shaped: highlighting by node kind, completion, and the SQLcl terminator rules (a statement is complete when the last top-level node is `;` or a `/` line). A statement the grammar does not know yet is misjudged until the grammar learns it; fix the grammar, not oracli.

### oracle-rs 0.1.7 limitations

Verified against Oracle Free 23ai; each workaround is marked `shortcut:` in `src/db.rs`.

- The first page of a query never sets `has_more_rows`, and `fetch_more` on a query cursor returns nothing. Paging re-runs the query with `OFFSET n ROWS FETCH NEXT 100 ROWS ONLY`, so pages can shift without a stable `ORDER BY`.
- Any SQL error closes the session and loses the ORA code. The worker pings after an error and reconnects; uncommitted work is lost.
- A REF CURSOR (`OPEN :c FOR ...`) pages correctly, but an error inside it also kills the session, so user queries do not use it. oracli's own dictionary queries do (`fetch_all`), so loading a large schema like SYS runs the query once instead of once per page. Fetches stay at 100 rows; larger fetch sizes fail to parse, and the cursor is never closed.

## Connecting

Match SQLcl / SQL*Plus:

```
oracli user/pass@host:port/service
oracli user@host:port/service     # prompts for the password
oracli user                       # connect string from $TWO_TASK
oracli /nolog                     # start unconnected; use CONNECT (also the default with no arguments)
```

Only Easy Connect strings. No `tnsnames.ora`/`TNS_ADMIN` and no config file in the MVP (`oracle-rs` cannot resolve TNS aliases).

## Nix

Based on numtide/blueprint's `treefmt-and-git-hooks` template (`prefix = "nix"`).

- `nix/package.nix` (or `nix/packages/`): crane build of the workspace, Rust from nixpkgs, linked with mold.
- `nix/devshell.nix`: Rust toolchain, rust-analyzer, mold, sccache. Sets `RUSTC_WRAPPER=sccache` and the mold linker. sccache is devshell-only; it cannot help inside the Nix sandbox.
- `nix/treefmt.nix`: nixfmt, rustfmt.
- `nix/checks/`: clippy (`-D warnings`), tests, and pre-commit (nil, statix, treefmt) via git-hooks.nix.

## Commands

```
nix develop                 # or direnv
cargo build
cargo test                  # unit tests
(cd crates/tree-sitter-plsql && tree-sitter generate && tree-sitter test)   # after editing grammar.js
cargo test -- --ignored     # DB integration tests, needs Oracle (see below); target via ORACLI_TEST_LOGON
cargo clippy --all-targets -- -D warnings
nix fmt
nix flake check
```

## Testing

- `ratatui-ptk` and parsers (statement terminator, argument parsing): plain unit tests.
- Grammar: corpus tests in `crates/tree-sitter-plsql/test/corpus/`, run by `tree-sitter test`. Not part of `nix flake check`.
- Rendering: ratatui `TestBackend`.
- Database: integration tests against the Oracle Free container (`gvenzl/oracle-free`), marked `#[ignore]` and excluded from `nix flake check`. Default target `scott/tiger@localhost:1521/FREEPDB1`:

  ```
  docker run -d -p 1521:1521 -e ORACLE_PASSWORD=oracle -e APP_USER=scott -e APP_USER_PASSWORD=tiger gvenzl/oracle-free:slim-faststart
  ```

## Out of scope for the MVP

Vi key bindings, key remapping, tnsnames.ora, saved connections, schema browser, export, EXPLAIN PLAN, SQLcl `SET`/`@script`/`SPOOL`, multiple result tabs.
