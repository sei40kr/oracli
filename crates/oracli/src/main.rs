mod app;
mod catalog;
mod components;
mod db;
mod logon;
mod sql;
mod theme;

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::{execute, terminal};
use ratatui_ptk::History;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::app::{App, Event};
use crate::logon::Logon;

const USAGE: &str = "\
Usage: oracli [user[/password][@connect] | /nolog]

  connect   Easy Connect string, e.g. host:1521/service. Defaults to $TWO_TASK.
  /nolog    Start without connecting (the default with no arguments).";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("oracli: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> io::Result<()> {
    let arg = std::env::args().nth(1);
    let logon = match arg.as_deref() {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return Ok(());
        }
        None | Some("/nolog") => None,
        Some(s) => {
            let two_task = std::env::var("TWO_TASK").ok();
            let mut l = Logon::parse(s, two_task.as_deref()).map_err(io::Error::other)?;
            if l.password.is_none() {
                l.password = Some(read_password()?);
            }
            Some(l)
        }
    };
    let (history, history_err) = match History::with_file(history_path()) {
        Ok(h) => (h, None),
        Err(e) => (History::new(), Some(e)),
    };

    let (tx, mut rx) = unbounded_channel();
    spawn_input(tx.clone());
    spawn_tick(tx.clone());

    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableBracketedPaste)?;
    let mut app = App::new(tx, history, logon, theme::Theme::from_env());
    if let Some(e) = history_err {
        app.warn(format!("History is not saved: {e}"));
    }
    let result = app::run(&mut terminal, &mut app, &mut rx).await;
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    result
}

fn history_path() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_default();
    data.join("oracli/history")
}

/// Reads a password from the terminal without echoing it.
fn read_password() -> io::Result<String> {
    eprint!("Password: ");
    io::stderr().flush()?;
    terminal::enable_raw_mode()?;
    let read = || -> io::Result<String> {
        let mut pw = String::new();
        loop {
            let event::Event::Key(k) = event::read()? else {
                continue;
            };
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match k.code {
                KeyCode::Enter => return Ok(pw),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
                }
                KeyCode::Backspace => {
                    pw.pop();
                }
                KeyCode::Char(c) => pw.push(c),
                _ => {}
            }
        }
    };
    let result = read();
    terminal::disable_raw_mode()?;
    eprintln!();
    result
}

/// Blocking terminal reads live on their own thread so the event loop never waits on them.
fn spawn_input(tx: UnboundedSender<Event>) {
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            let ev = match ev {
                event::Event::Key(k) => Event::Key(k),
                event::Event::Paste(s) => Event::Paste(s),
                event::Event::Resize(..) => Event::Resize,
                _ => continue,
            };
            if tx.send(ev).is_err() {
                break;
            }
        }
    });
}

/// Redraws the elapsed time while a request runs.
fn spawn_tick(tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            if tx.send(Event::Tick).is_err() {
                break;
            }
        }
    });
}
