use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::Component;
use crate::db::DbEvent;
use crate::theme::Theme;

/// One-line summary of the session at the bottom of the screen.
#[derive(Default)]
pub struct Status {
    connection: Option<String>,
    /// When the oldest in-flight request started.
    pub busy_since: Option<Instant>,
    /// Whether DML may be uncommitted.
    pub dirty: bool,
}

impl Component for Status {
    fn on_db(&mut self, ev: &DbEvent) {
        if let DbEvent::Connected(label) = ev {
            self.connection = Some(label.clone());
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut spans = vec![Span::raw(format!(
            " {} ",
            self.connection.as_deref().unwrap_or("not connected")
        ))];
        if self.dirty {
            spans.push(Span::styled(" TX pending ", theme.status_dirty));
        }
        spans.push(match self.busy_since {
            Some(t) => Span::styled(
                format!(
                    " Running {:.1}s · Ctrl-C cancels ",
                    t.elapsed().as_secs_f32()
                ),
                theme.status_busy,
            ),
            None => Span::raw(" Ready "),
        });
        spans.push(Span::raw("· F2 grid · F5 refresh · Ctrl-D quit"));
        f.render_widget(Line::from(spans).style(theme.status), area);
    }
}
