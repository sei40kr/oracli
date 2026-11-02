use std::io;
use std::time::Instant;

use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Layout};
use ratatui::{DefaultTerminal, Frame};
use ratatui_ptk::History;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::catalog::{SharedCatalog, lock};
use crate::components::{Component, Entry, Grid, Input, Log, Status};
use crate::db::{Db, DbEvent, Request};
use crate::logon::Logon;
use crate::sql::{ClientCommand, client_command, first_keyword, prepare};
use crate::theme::Theme;

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Paste(String),
    Resize,
    Tick,
    /// The completion thread has candidates for the input.
    Completions,
    Db(DbEvent),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Submit(String),
    Quit,
    /// Ctrl-C: cancel the running request, or clear the input when idle.
    Interrupt,
    /// Scroll the log by this many half pages; positive scrolls back.
    ScrollLog(i32),
    OpenGrid,
    CloseGrid,
    FetchMore,
    RefreshCatalog,
}

#[derive(PartialEq, Eq)]
enum Focus {
    Input,
    Grid,
}

pub struct App {
    events: UnboundedSender<Event>,
    db: Db,
    logon: Option<Logon>,
    catalog: SharedCatalog,
    input: Input,
    log: Log,
    grid: Grid,
    status: Status,
    theme: Theme,
    focus: Focus,
    pending: usize,
    exit_warned: bool,
    pub quit: bool,
}

impl App {
    pub fn new(
        events: UnboundedSender<Event>,
        history: History,
        logon: Option<Logon>,
        theme: Theme,
    ) -> Self {
        let catalog = SharedCatalog::default();
        let wake = {
            let events = events.clone();
            move || {
                let _ = events.send(Event::Completions);
            }
        };
        let mut app = Self {
            db: Db::spawn(events.clone()),
            events,
            logon: None,
            input: Input::new(history, catalog.clone(), wake),
            catalog,
            log: Log::default(),
            grid: Grid::default(),
            status: Status::default(),
            theme,
            focus: Focus::Input,
            pending: 0,
            exit_warned: false,
            quit: false,
        };
        match logon {
            Some(l) => app.connect(l),
            None => app.log.push(Entry::Info(
                "Not connected. Use CONNECT user/password@host:port/service.".into(),
            )),
        }
        app
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.log.push(Entry::Warn(msg.into()));
    }

    pub fn update(&mut self, ev: Event) {
        for action in self.handle_event(ev) {
            self.dispatch(action);
        }
        let wanted = std::mem::take(&mut lock(&self.catalog).wanted);
        for schema in wanted {
            self.send(Request::LoadSchema(Some(schema)));
        }
    }

    fn handle_event(&mut self, ev: Event) -> Vec<Action> {
        match ev {
            Event::Key(key) => match self.focus {
                Focus::Input => self.input.handle_key(key),
                Focus::Grid => self.grid.handle_key(key),
            },
            Event::Paste(s) => {
                if self.focus == Focus::Input {
                    self.input.paste(&s);
                }
                vec![]
            }
            Event::Completions => {
                self.input.poll_completions();
                vec![]
            }
            Event::Resize | Event::Tick => vec![],
            Event::Db(ev) => {
                match &ev {
                    DbEvent::Idle => {
                        self.pending = self.pending.saturating_sub(1);
                        if self.pending == 0 {
                            self.status.busy_since = None;
                        }
                    }
                    DbEvent::Connected(_) => self.set_dirty(false),
                    DbEvent::Users(users) => lock(&self.catalog).users = users.clone(),
                    DbEvent::Synonyms(synonyms) => lock(&self.catalog).synonyms = synonyms.clone(),
                    DbEvent::Schema { owner, tables, own } => {
                        lock(&self.catalog).set_schema(owner.clone(), tables.clone(), *own)
                    }
                    _ => {}
                }
                self.input.on_db(&ev);
                self.log.on_db(&ev);
                self.grid.on_db(&ev);
                self.status.on_db(&ev);
                vec![]
            }
        }
    }

    fn dispatch(&mut self, action: Action) {
        match action {
            Action::Submit(text) => self.submit(&text),
            Action::Quit => self.exit(),
            Action::Interrupt if self.pending > 0 => self.cancel(),
            Action::Interrupt => self.input.clear(),
            Action::ScrollLog(n) => self.log.scroll(n),
            Action::OpenGrid if self.grid.has_result() => self.focus = Focus::Grid,
            Action::OpenGrid => {}
            Action::CloseGrid => self.focus = Focus::Input,
            Action::FetchMore => self.send(Request::FetchMore),
            Action::RefreshCatalog => self.send(Request::LoadSchema(None)),
        }
    }

    fn send(&mut self, req: Request) {
        self.db.send(req);
        self.pending += 1;
        self.status.busy_since.get_or_insert_with(Instant::now);
    }

    fn set_dirty(&mut self, dirty: bool) {
        self.status.dirty = dirty;
        self.exit_warned = false;
    }

    fn connect(&mut self, logon: Logon) {
        self.logon = Some(logon.clone());
        self.send(Request::Connect(logon));
    }

    fn submit(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.log.push(Entry::Input(text.to_owned()));
        match client_command(text) {
            Some(ClientCommand::Exit) => self.exit(),
            Some(ClientCommand::Connect(arg)) => {
                let two_task = std::env::var("TWO_TASK").ok();
                match Logon::parse(arg, two_task.as_deref()) {
                    // shortcut: no masked password prompt inside the TUI; add one if CONNECT without a password is common.
                    Ok(l) if l.password.is_none() => self.log.push(Entry::Error(
                        "Password required: CONNECT user/password@connect".into(),
                    )),
                    Ok(l) => self.connect(l),
                    Err(e) => self.log.push(Entry::Error(e)),
                }
            }
            Some(ClientCommand::Describe("")) => self
                .log
                .push(Entry::Error("Usage: DESC [schema.]object".into())),
            Some(ClientCommand::Describe(name)) => self.send(Request::Describe(name.to_owned())),
            None => {
                let sql = prepare(text);
                if sql.is_empty() {
                    return;
                }
                match first_keyword(&sql).as_deref() {
                    Some("INSERT" | "UPDATE" | "DELETE" | "MERGE") => self.set_dirty(true),
                    // DDL commits implicitly.
                    Some(
                        "COMMIT" | "ROLLBACK" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "GRANT"
                        | "REVOKE" | "RENAME" | "COMMENT",
                    ) => self.set_dirty(false),
                    _ => {}
                }
                self.send(Request::Execute(sql));
            }
        }
    }

    fn exit(&mut self) {
        if self.status.dirty && !self.exit_warned {
            self.exit_warned = true;
            self.warn("Uncommitted changes will be rolled back. COMMIT first, or exit again to discard them.");
            return;
        }
        self.quit = true;
    }

    fn cancel(&mut self) {
        self.db.abort();
        self.db = Db::spawn(self.events.clone());
        self.pending = 0;
        self.status.busy_since = None;
        self.grid.cancel_fetch();
        let msg = if self.status.dirty {
            "Cancelled. Reconnecting; uncommitted changes were rolled back."
        } else {
            "Cancelled. Reconnecting."
        };
        self.warn(msg);
        self.set_dirty(false);
        if let Some(l) = self.logon.clone() {
            self.send(Request::Connect(l));
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let [main, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
        if self.focus == Focus::Grid {
            self.grid.draw(f, main, &self.theme);
        } else {
            let input_height = self.input.height().min(main.height / 2).max(1);
            let [log, input] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(input_height)])
                    .areas(main);
            self.log.draw(f, log, &self.theme);
            self.input.draw(f, input, &self.theme);
        }
        self.status.draw(f, status, &self.theme);
    }
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &mut UnboundedReceiver<Event>,
) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|f| app.draw(f))?;
        let Some(ev) = rx.recv().await else { break };
        app.update(ev);
        // Apply queued events before redrawing so a paste or key burst draws once.
        while let Ok(ev) = rx.try_recv() {
            app.update(ev);
        }
    }
    Ok(())
}
