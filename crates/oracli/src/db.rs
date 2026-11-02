//! The DB task: owns the connection and turns requests into events.

use oracle_rs::{BindParam, Connection, QueryResult, Value};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;

use crate::app::Event;
use crate::catalog::{Synonyms, Table, Tables};
use crate::logon::Logon;

/// Rows oracle-rs returns with the first page of a query; later pages use the same size.
const PAGE: usize = 100;

#[derive(Debug)]
pub enum Request {
    /// Connect, then load the own schema for completion.
    Connect(Logon),
    Execute(String),
    FetchMore,
    /// Load a schema's tables and columns; `None` loads the session's own schema and the user list.
    LoadSchema(Option<String>),
    Describe(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbEvent {
    Connected(String),
    /// Column names and rows of a new result set.
    Rows {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        more: bool,
    },
    /// Further rows of the latest result set.
    MoreRows {
        rows: Vec<Vec<String>>,
        more: bool,
    },
    Message(String),
    Error(String),
    Schema {
        owner: String,
        tables: Tables,
        own: bool,
    },
    Users(Vec<String>),
    Synonyms(Synonyms),
    /// The request finished, successfully or not.
    Idle,
}

/// Handle to the running DB task.
pub struct Db {
    tx: UnboundedSender<Request>,
    task: JoinHandle<()>,
}

impl Db {
    pub fn spawn(events: UnboundedSender<Event>) -> Self {
        let (tx, rx) = unbounded_channel();
        let task = tokio::spawn(run(rx, events));
        Self { tx, task }
    }

    pub fn send(&self, req: Request) {
        // The task only stops when aborted, and abort replaces the handle.
        let _ = self.tx.send(req);
    }

    /// Kills the task mid-request. Its connection is dropped with it, since the wire protocol
    /// state after an interrupted call is unknown.
    pub fn abort(&self) {
        self.task.abort();
    }
}

struct Worker {
    conn: Option<Connection>,
    logon: Option<Logon>,
    /// The latest query and how many of its rows were sent, for fetching the next page.
    cursor: Option<(String, usize)>,
    events: UnboundedSender<Event>,
}

async fn run(mut rx: UnboundedReceiver<Request>, events: UnboundedSender<Event>) {
    let mut w = Worker {
        conn: None,
        logon: None,
        cursor: None,
        events,
    };
    while let Some(req) = rx.recv().await {
        if let Err(e) = w.handle(req).await {
            w.emit(DbEvent::Error(e));
            w.recover().await;
        }
        w.emit(DbEvent::Idle);
    }
}

impl Worker {
    fn emit(&self, ev: DbEvent) {
        let _ = self.events.send(Event::Db(ev));
    }

    fn conn(&self) -> Result<&Connection, String> {
        self.conn
            .as_ref()
            .ok_or_else(|| "Not connected.".to_owned())
    }

    async fn open(&mut self, logon: &Logon) -> Result<(), String> {
        if let Some(old) = self.conn.take() {
            let _ = old.close().await;
        }
        self.cursor = None;
        let password = logon.password.as_deref().unwrap_or_default();
        let conn = Connection::connect(&logon.connect, &logon.user, password)
            .await
            .map_err(|e| e.to_string())?;
        self.conn = Some(conn);
        self.emit(DbEvent::Connected(logon.label()));
        Ok(())
    }

    /// Reopens the session if the failed request killed it.
    // shortcut: oracle-rs 0.1.7 drops the session on any SQL error (and loses the ORA code), so
    // this runs after most mistakes; remove once errors keep the session upstream.
    async fn recover(&mut self) {
        let Some(conn) = &self.conn else { return };
        if conn.ping().await.is_ok() {
            return;
        }
        self.conn = None;
        let Some(logon) = self.logon.clone() else {
            return;
        };
        match self.open(&logon).await {
            Ok(()) => self.emit(DbEvent::Error(
                "The server closed the session; reconnected. Uncommitted changes were rolled back."
                    .into(),
            )),
            Err(e) => self.emit(DbEvent::Error(format!("Reconnect failed: {e}"))),
        }
    }

    async fn handle(&mut self, req: Request) -> Result<(), String> {
        match req {
            Request::Connect(logon) => {
                self.open(&logon).await?;
                self.logon = Some(logon);
                self.load_schema(None).await?;
            }
            Request::Execute(sql) => {
                self.cursor = None;
                let r = self
                    .conn()?
                    .execute(&sql, &[])
                    .await
                    .map_err(|e| e.to_string())?;
                if r.columns.is_empty() {
                    let msg = match crate::sql::first_keyword(&sql).as_deref() {
                        Some("INSERT" | "UPDATE" | "DELETE" | "MERGE") => {
                            format!("{} row(s) affected.", r.rows_affected)
                        }
                        _ => "Done.".to_owned(),
                    };
                    self.emit(DbEvent::Message(msg));
                } else {
                    let rows = strings(&r);
                    let more = rows.len() >= PAGE;
                    if more {
                        self.cursor = Some((sql, rows.len()));
                    }
                    self.emit(DbEvent::Rows {
                        columns: r.columns.iter().map(|c| c.name.clone()).collect(),
                        rows,
                        more,
                    });
                }
            }
            Request::FetchMore => {
                let Some((sql, offset)) = self.cursor.take() else {
                    return Ok(());
                };
                let rows = self.page(&sql, &[], offset).await?;
                let more = rows.len() >= PAGE;
                if more {
                    self.cursor = Some((sql, offset + rows.len()));
                }
                self.emit(DbEvent::MoreRows { rows, more });
            }
            Request::LoadSchema(owner) => self.load_schema(owner).await?,
            Request::Describe(name) => self.describe(&name).await?,
        }
        Ok(())
    }

    async fn load_schema(&mut self, owner: Option<String>) -> Result<(), String> {
        let own = owner.is_none();
        let owner = match owner {
            Some(o) => o,
            None => {
                let users = self
                    .fetch_all("SELECT username FROM all_users ORDER BY 1", &[])
                    .await?;
                self.emit(DbEvent::Users(
                    users.into_iter().map(|mut r| r.swap_remove(0)).collect(),
                ));
                let row = self
                    .fetch_all(
                        "SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') FROM dual",
                        &[],
                    )
                    .await?;
                row.into_iter()
                    .next()
                    .ok_or("no current schema")?
                    .swap_remove(0)
            }
        };
        let rows = self
            .fetch_all(
                "SELECT c.table_name, o.object_type, c.column_name \
                 FROM all_tab_columns c \
                 JOIN all_objects o ON o.owner = c.owner AND o.object_name = c.table_name \
                   AND o.object_type IN ('TABLE', 'VIEW') \
                 WHERE c.owner = :1 ORDER BY c.table_name, c.column_id",
                &[Value::String(owner.clone())],
            )
            .await?;
        let mut tables = Tables::new();
        for mut r in rows {
            let column = r.pop().unwrap_or_default();
            let kind = r.pop().unwrap_or_default();
            let name = r.pop().unwrap_or_default();
            tables
                .entry(name)
                .or_insert_with(|| Table {
                    kind,
                    columns: vec![],
                })
                .columns
                .push(column);
        }
        if own {
            // PUBLIC sorts first so the own schema's synonyms overwrite public ones.
            let rows = self
                .fetch_all(
                    "SELECT synonym_name, table_owner, table_name FROM all_synonyms \
                     WHERE owner IN ('PUBLIC', :1) AND db_link IS NULL \
                     ORDER BY DECODE(owner, 'PUBLIC', 0, 1)",
                    &[Value::String(owner.clone())],
                )
                .await?;
            self.emit(DbEvent::Synonyms(
                rows.into_iter()
                    .map(|mut r| {
                        let object = r.pop().unwrap_or_default();
                        let target = r.pop().unwrap_or_default();
                        (r.pop().unwrap_or_default(), (target, object))
                    })
                    .collect(),
            ));
        }
        self.emit(DbEvent::Schema { owner, tables, own });
        Ok(())
    }

    async fn describe(&mut self, name: &str) -> Result<(), String> {
        // shortcut: names are upper-cased, so quoted mixed-case names cannot be described.
        let name = name.trim().to_ascii_uppercase();
        let (owner, binds) = match name.split_once('.') {
            Some((o, t)) => (
                ":2",
                vec![Value::String(t.to_owned()), Value::String(o.to_owned())],
            ),
            None => (
                "SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')",
                vec![Value::String(name.clone())],
            ),
        };
        let rows = self
            .fetch_all(
                &format!("SELECT column_name, DECODE(nullable, 'N', 'NOT NULL'), \
                   data_type || CASE \
                     WHEN data_type IN ('VARCHAR2', 'NVARCHAR2', 'CHAR', 'NCHAR', 'RAW') \
                       THEN '(' || data_length || ')' \
                     WHEN data_type = 'NUMBER' AND data_precision IS NOT NULL \
                       THEN '(' || data_precision || NVL2(NULLIF(data_scale, 0), ',' || data_scale, '') || ')' \
                   END \
                 FROM all_tab_columns \
                 WHERE table_name = :1 AND owner = {owner} \
                 ORDER BY column_id"),
                &binds,
            )
            .await?;
        if rows.is_empty() {
            return Err(format!("Object {name} does not exist."));
        }
        self.cursor = None;
        self.emit(DbEvent::Rows {
            columns: vec!["Name".into(), "Null?".into(), "Type".into()],
            rows,
            more: false,
        });
        Ok(())
    }

    /// Every row of a query that cannot fail on user input. `:1`, `:2`, ... in `sql` bind
    /// `params`. It goes through a REF CURSOR so the query runs once, not once per page as
    /// [`Self::page`] does; an error inside a REF CURSOR kills the session, hence not for user SQL.
    /// Fetches stay at oracle-rs's default size, since 0.1.7 fails to parse larger ones
    /// ("buffer underflow").
    ///
    /// shortcut: the cursor is never closed (oracle-rs 0.1.7 can only close scrollable cursors),
    /// so every call holds one of the session's OPEN_CURSORS until reconnect; close it once
    /// oracle-rs can, or if calls become frequent.
    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<Vec<String>>, String> {
        let conn = self.conn()?;
        let mut binds = vec![BindParam::output_cursor()];
        binds.extend(params.iter().cloned().map(BindParam::input));
        let r = conn
            .execute_plsql(&format!("BEGIN OPEN :c FOR {sql}; END;"), &binds)
            .await
            .map_err(|e| e.to_string())?;
        let Some(Value::Cursor(cursor)) = r.out_values.first() else {
            return Err("no cursor returned".into());
        };
        let mut rows = Vec::new();
        loop {
            let page = conn.fetch_cursor(cursor).await.map_err(|e| e.to_string())?;
            let done = page.rows.len() < PAGE;
            rows.extend(strings(&page));
            if done {
                return Ok(rows);
            }
        }
    }

    /// Rows `offset..offset + PAGE` of a query.
    ///
    /// shortcut: oracle-rs 0.1.7 cannot page an open cursor (`fetch_more` returns nothing and the
    /// first page never sets `has_more_rows`), so each page re-runs the query with a row-limiting
    /// clause. Pages can repeat or skip rows without a stable ORDER BY, and queries that already
    /// limit rows fail past the first page. Switch to `fetch_more` once it works upstream.
    async fn page(
        &self,
        sql: &str,
        params: &[Value],
        offset: usize,
    ) -> Result<Vec<Vec<String>>, String> {
        let sql = if offset == 0 {
            sql.to_owned()
        } else {
            format!("{sql} OFFSET {offset} ROWS FETCH NEXT {PAGE} ROWS ONLY")
        };
        let r = self
            .conn()?
            .query(&sql, params)
            .await
            .map_err(|e| e.to_string())?;
        Ok(strings(&r))
    }
}

/// Rows as display strings. NULL becomes empty, which Oracle cannot tell apart from `''` anyway.
fn strings(r: &QueryResult) -> Vec<Vec<String>> {
    r.rows
        .iter()
        .map(|row| {
            row.values()
                .iter()
                .map(|v| {
                    if v.is_null() {
                        String::new()
                    } else {
                        v.to_string()
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs requests in order and collects their events up to each `Idle`.
    async fn run_all(reqs: Vec<Request>) -> Vec<Vec<DbEvent>> {
        let (tx, mut rx) = unbounded_channel();
        let db = Db::spawn(tx);
        let mut out = Vec::new();
        for req in reqs {
            db.send(req);
            let mut events = Vec::new();
            loop {
                let Some(Event::Db(ev)) = rx.recv().await else {
                    panic!("worker stopped")
                };
                if ev == DbEvent::Idle {
                    break;
                }
                events.push(ev);
            }
            out.push(events);
        }
        out
    }

    /// Needs Oracle Free, e.g. `docker run -p 1521:1521 -e ORACLE_PASSWORD=oracle
    /// -e APP_USER=scott -e APP_USER_PASSWORD=tiger gvenzl/oracle-free:slim-faststart`.
    /// Override the target with `ORACLI_TEST_LOGON`.
    #[tokio::test]
    #[ignore]
    async fn round_trip_against_oracle() {
        let logon = std::env::var("ORACLI_TEST_LOGON")
            .unwrap_or_else(|_| "scott/tiger@localhost:1521/FREEPDB1".into());
        let logon = Logon::parse(&logon, None).unwrap();
        let ev = run_all(vec![
            Request::Connect(logon),
            Request::Execute("BEGIN EXECUTE IMMEDIATE 'DROP TABLE oracli_t'; EXCEPTION WHEN OTHERS THEN NULL; END;".into()),
            Request::Execute("CREATE TABLE oracli_t (id NUMBER(5) NOT NULL, name VARCHAR2(10))".into()),
            Request::Execute("INSERT INTO oracli_t SELECT level, 'n' || level FROM dual CONNECT BY level <= 250".into()),
            Request::Execute("SELECT id, name FROM oracli_t ORDER BY id".into()),
            Request::FetchMore,
            Request::FetchMore,
            Request::Describe("oracli_t".into()),
            Request::LoadSchema(None),
            Request::Execute("ROLLBACK".into()),
            Request::Execute("SELECT 1/0 FROM dual".into()),
            Request::Execute("SELECT 1 FROM dual".into()),
            Request::LoadSchema(Some("SYS".into())),
        ])
        .await;

        assert_eq!(
            ev[11],
            [DbEvent::Rows {
                columns: vec!["1".into()],
                rows: vec![vec!["1".into()]],
                more: false
            }]
        );
        assert!(matches!(&ev[0][0], DbEvent::Connected(l) if l.starts_with("scott@")));
        assert!(
            ev[0]
                .iter()
                .any(|e| matches!(e, DbEvent::Schema { own: true, .. }))
        );
        assert!(ev[0].iter().any(|e| matches!(e, DbEvent::Synonyms(s)
            if s.get("DUAL") == Some(&("SYS".into(), "DUAL".into())))));
        assert_eq!(ev[3], [DbEvent::Message("250 row(s) affected.".into())]);
        let DbEvent::Rows {
            columns,
            rows,
            more,
        } = &ev[4][0]
        else {
            panic!("{:?}", ev[4])
        };
        assert_eq!(columns, &["ID", "NAME"]);
        assert_eq!(rows[0], ["1", "n1"]);
        assert!(*more);
        let fetched: usize = ev[5..7]
            .iter()
            .flatten()
            .map(|e| match e {
                DbEvent::MoreRows { rows, .. } => rows.len(),
                e => panic!("{e:?}"),
            })
            .sum();
        assert_eq!(rows.len() + fetched, 250);
        assert_eq!(
            ev[7],
            [DbEvent::Rows {
                columns: vec!["Name".into(), "Null?".into(), "Type".into()],
                rows: vec![
                    vec!["ID".into(), "NOT NULL".into(), "NUMBER(5)".into()],
                    vec!["NAME".into(), "".into(), "VARCHAR2(10)".into()],
                ],
                more: false,
            }]
        );
        let schema = ev[8].iter().find_map(|e| match e {
            DbEvent::Schema {
                tables, own: true, ..
            } => Some(tables),
            _ => None,
        });
        assert_eq!(schema.unwrap()["ORACLI_T"].columns, ["ID", "NAME"]);
        let DbEvent::Schema { owner, tables, .. } = &ev[12][0] else {
            panic!("{:?}", ev[12])
        };
        assert_eq!(owner, "SYS");
        assert!(tables.len() > 1000, "{}", tables.len());
        assert_eq!(tables["DUAL"].columns, ["DUMMY"]);
        // oracle-rs loses the ORA code and the session on errors; the worker reconnects.
        assert!(
            matches!(&ev[10][..], [DbEvent::Error(_), DbEvent::Connected(_), DbEvent::Error(e)] if e.contains("reconnected")),
            "{:?}",
            ev[10]
        );
    }
}
