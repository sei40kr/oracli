//! Cached dictionary metadata and the completer built on it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use ratatui_ptk::{Completer, Completion};

use tree_sitter::Node;

use crate::sql::{first_keyword, is_ident_char, parse};

pub const KEYWORDS: &[&str] = &[
    "ALL",
    "ALTER",
    "AND",
    "AS",
    "ASC",
    "BEGIN",
    "BETWEEN",
    "BY",
    "CASE",
    "COMMENT",
    "COMMIT",
    "CONNECT",
    "CREATE",
    "CROSS",
    "DECLARE",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "EXCEPTION",
    "EXISTS",
    "FETCH",
    "FIRST",
    "FOR",
    "FROM",
    "FULL",
    "FUNCTION",
    "GRANT",
    "GROUP",
    "HAVING",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "LEFT",
    "LIKE",
    "LIMIT",
    "MERGE",
    "MINUS",
    "NEXT",
    "NOT",
    "NULL",
    "OFFSET",
    "ON",
    "ONLY",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PACKAGE",
    "PARTITION",
    "PRIMARY",
    "PROCEDURE",
    "REPLACE",
    "REVOKE",
    "RIGHT",
    "ROLLBACK",
    "ROWNUM",
    "ROWS",
    "SAVEPOINT",
    "SELECT",
    "SEQUENCE",
    "SET",
    "SYSDATE",
    "SYSTIMESTAMP",
    "TABLE",
    "THEN",
    "TRIGGER",
    "TRUNCATE",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USING",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WITH",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// `TABLE` or `VIEW`.
    pub kind: String,
    pub columns: Vec<String>,
}

pub type Tables = BTreeMap<String, Table>;

/// Synonym name to the `(owner, object)` it stands for.
pub type Synonyms = BTreeMap<String, (String, String)>;

#[derive(Debug, Default)]
pub struct Catalog {
    /// The schema loaded at connect time; unqualified names resolve here.
    pub own: Option<String>,
    pub users: Vec<String>,
    pub schemas: HashMap<String, Tables>,
    /// Public synonyms and the own schema's, the own ones winning.
    pub synonyms: Synonyms,
    /// Schemas the completer needs but has not loaded; the app drains this.
    pub wanted: Vec<String>,
    requested: HashSet<String>,
}

impl Catalog {
    pub fn set_schema(&mut self, owner: String, tables: Tables, own: bool) {
        if own {
            self.own = Some(owner.clone());
        }
        self.requested.insert(owner.clone());
        self.schemas.insert(owner, tables);
    }

    fn own_tables(&self) -> Option<&Tables> {
        self.own.as_ref().and_then(|o| self.schemas.get(o))
    }

    /// Looks up a table named in the statement; unqualified names resolve in the own schema,
    /// then through synonyms. A schema that is not loaded yet is queued in `wanted` and yields
    /// `None` until it arrives.
    fn table(&mut self, schema: Option<&str>, name: &str) -> Option<&Table> {
        let (schema, name) = match schema {
            Some(s) => (s.to_owned(), name),
            None if self.own_tables().is_some_and(|t| t.contains_key(name)) => {
                (self.own.clone()?, name)
            }
            None => match self.synonyms.get(name) {
                Some((owner, object)) => (owner.clone(), object.as_str()),
                None => (self.own.clone()?, name),
            },
        };
        let name = name.to_owned();
        if !self.schemas.contains_key(&schema)
            && self.users.contains(&schema)
            && self.requested.insert(schema.clone())
        {
            self.wanted.push(schema.clone());
        }
        self.schemas.get(&schema)?.get(&name)
    }

    fn complete(&mut self, text: &str, cursor: usize) -> Vec<Completion> {
        let start = text[..cursor]
            .char_indices()
            .rev()
            .take_while(|&(_, c)| is_ident_char(c) || c == '.')
            .last()
            .map_or(cursor, |(i, _)| i);
        let word = &text[start..cursor];
        let mut out = Vec::new();
        if word.is_empty() {
            return out;
        }
        let ctx = Context::at(text, cursor);

        if let Some((qual, part)) = word.rsplit_once('.') {
            let qual = qual.to_ascii_uppercase();
            let part_start = cursor - part.len();
            if let Some(tables) = self.schemas.get(&qual) {
                for (name, t) in tables {
                    push(&mut out, name, part, part_start, &t.kind);
                }
                return out;
            }
            let source = ctx.sources.iter().find(|s| {
                s.alias.as_deref() == Some(qual.as_str()) || (s.alias.is_none() && s.name == qual)
            });
            let (schema, name) = match source {
                Some(s) => (s.schema.clone(), s.name.clone()),
                None => (None, qual.clone()),
            };
            if let Some(t) = self.table(schema.as_deref(), &name) {
                for c in &t.columns {
                    push(&mut out, c, part, part_start, "COLUMN");
                }
            } else if self.users.contains(&qual) && self.requested.insert(qual.clone()) {
                self.wanted.push(qual);
            }
            return out;
        }

        let tables = matches!(ctx.expect, Expect::Table | Expect::Any);
        let columns = matches!(ctx.expect, Expect::Column | Expect::Any);
        if tables {
            if let Some(own) = self.own_tables() {
                for (name, t) in own {
                    push(&mut out, name, word, start, &t.kind);
                }
            }
            for name in self.synonyms.keys() {
                push(&mut out, name, word, start, "SYNONYM");
            }
        }
        if columns {
            let mut cols = Vec::new();
            for s in &ctx.sources {
                if let Some(t) = self.table(s.schema.as_deref(), &s.name) {
                    cols.extend(t.columns.iter().cloned());
                }
            }
            for c in &cols {
                push(&mut out, c, word, start, "COLUMN");
            }
        }
        if ctx.expect != Expect::Table {
            for k in KEYWORDS {
                push(&mut out, k, word, start, "KEYWORD");
            }
        }
        if tables {
            for u in &self.users {
                push(&mut out, u, word, start, "SCHEMA");
            }
        }
        let mut seen = HashSet::new();
        out.retain(|c| seen.insert(c.text.clone()));
        out
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Expect {
    Keyword,
    Table,
    /// Columns and keywords, which include the functions and operators an expression may use.
    Column,
    /// The tree does not tell, as inside an ERROR node.
    Any,
}

/// A table the statement reads or writes, with names normalized as Oracle stores them.
#[derive(Debug)]
struct Source {
    schema: Option<String>,
    name: String,
    alias: Option<String>,
}

struct Context {
    expect: Expect,
    sources: Vec<Source>,
}

/// Inserted at the cursor before parsing, so a partial or empty word still parses as a name.
const PLACEHOLDER: &str = "x__";

impl Context {
    fn at(text: &str, cursor: usize) -> Self {
        let probe = format!("{}{PLACEHOLDER}{}", &text[..cursor], &text[cursor..]);
        let tree = parse(&probe);
        let root = tree.root_node();
        let mut sources = Vec::new();
        collect_sources(root, &probe, &mut sources);
        let expect = if matches!(first_keyword(text).as_deref(), Some("DESC" | "DESCRIBE")) {
            Expect::Table
        } else {
            root.named_descendant_for_byte_range(cursor, cursor + PLACEHOLDER.len())
                .map_or(Expect::Any, expect)
        };
        Self { expect, sources }
    }
}

/// What the grammar allows where `node`, the identifier being typed, sits.
fn expect(node: Node) -> Expect {
    let Some(parent) = node.parent() else {
        return Expect::Any;
    };
    let is_alias = parent.child_by_field_name("alias") == Some(node);
    match parent.kind() {
        "object_name" => Expect::Table,
        "table_reference" | "select_item" if is_alias => Expect::Keyword,
        "column_reference" | "function_call" | "column_list" => Expect::Column,
        _ => Expect::Any,
    }
}

/// Every table reference in the statement.
// shortcut: subqueries share one scope, so columns of outer and sibling queries are offered too;
// give each query its own scope if the extra candidates get in the way.
fn collect_sources(node: Node, text: &str, out: &mut Vec<Source>) {
    if node.kind() == "table_reference"
        && let Some(obj) = node
            .child_by_field_name("name")
            .or_else(|| node.named_child(0))
        && obj.kind() == "object_name"
        && let Some(name) = obj.child_by_field_name("name")
    {
        let ident = |n: Node| normalize(&text[n.byte_range()]);
        out.push(Source {
            schema: obj.child_by_field_name("schema").map(ident),
            name: ident(name),
            alias: node.child_by_field_name("alias").map(ident),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_sources(child, text, out);
    }
}

/// Oracle folds unquoted names to upper case and keeps quoted ones as written.
fn normalize(ident: &str) -> String {
    match ident.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(quoted) => quoted.to_owned(),
        None => ident.to_ascii_uppercase(),
    }
}

/// Adds `name` if it starts with `typed`, in lower case when the user is typing lower case.
fn push(out: &mut Vec<Completion>, name: &str, typed: &str, start: usize, meta: &str) {
    if !name
        .to_ascii_uppercase()
        .starts_with(&typed.to_ascii_uppercase())
    {
        return;
    }
    // shortcut: case follows the typed text, so mixed-case quoted names complete wrong; quote them if needed.
    let text = if typed.chars().any(|c| c.is_lowercase()) {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    out.push(Completion::new(text, start).with_meta(meta));
}

/// The catalog shared by the app and the completer, which runs on a thread of its own.
pub type SharedCatalog = Arc<Mutex<Catalog>>;

/// Locks the catalog even if a completion thread panicked while holding it; the cache is
/// still usable, and losing the session over it would be worse.
pub fn lock(catalog: &SharedCatalog) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct SqlCompleter(pub SharedCatalog);

impl Completer for SqlCompleter {
    fn complete(&self, text: &str, cursor: usize) -> Vec<Completion> {
        lock(&self.0).complete(text, cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog {
            users: vec!["HR".into(), "SCOTT".into(), "SYS".into()],
            synonyms: [("DUAL".to_string(), ("SYS".to_string(), "DUAL".to_string()))].into(),
            ..Default::default()
        };
        let table = |cols: &[&str]| Table {
            kind: "TABLE".into(),
            columns: cols.iter().map(|s| s.to_string()).collect(),
        };
        c.set_schema(
            "SCOTT".into(),
            [
                ("EMP".to_string(), table(&["EMPNO", "ENAME", "DEPTNO"])),
                ("DEPT".to_string(), table(&["DEPTNO", "DNAME"])),
            ]
            .into(),
            true,
        );
        c
    }

    /// Completes at `|`, or at the end without one.
    fn texts(c: &mut Catalog, text: &str) -> Vec<String> {
        let cursor = text.find('|').unwrap_or(text.len());
        let text = text.replacen('|', "", 1);
        c.complete(&text, cursor)
            .into_iter()
            .map(|c| c.text)
            .collect()
    }

    #[test]
    fn completes_what_the_clause_allows() {
        let mut c = catalog();
        assert_eq!(texts(&mut c, "select * from de"), ["dept"]);
        assert_eq!(texts(&mut c, "select * from emp, d"), ["dept", "dual"]);
        assert_eq!(texts(&mut c, "desc e"), ["emp"]);
        assert_eq!(texts(&mut c, "select 1 from du"), ["dual"]);
        assert_eq!(texts(&mut c, "select * from emp wh"), ["when", "where"]);
        assert_eq!(texts(&mut c, "select ename fr"), ["from"]);
        assert_eq!(texts(&mut c, "select EN"), ["END"]);
        assert_eq!(
            texts(&mut c, "select x from emp where EN"),
            ["ENAME", "END"]
        );
        assert_eq!(
            texts(&mut c, "select count(DEPT) from emp where (DEPT"),
            ["DEPTNO"]
        );
        assert!(texts(&mut c, "select DN| from emp").is_empty());
        assert_eq!(texts(&mut c, "select DN| from dept"), ["DNAME"]);
        assert_eq!(texts(&mut c, "insert into dept (dn"), ["dname"]);
        assert_eq!(texts(&mut c, "-- from\nse"), ["select", "sequence", "set"]);
        assert_eq!(texts(&mut c, ""), Vec::<String>::new());
    }

    #[test]
    fn completes_after_alias_and_table_qualifier() {
        let mut c = catalog();
        assert_eq!(texts(&mut c, "select e.en"), Vec::<String>::new());
        assert_eq!(texts(&mut c, "select * from emp e where e.en"), ["ename"]);
        assert_eq!(
            texts(&mut c, "select x.DEPT| from emp x join dept d on 1 = 1"),
            ["DEPTNO"]
        );
        assert_eq!(
            texts(
                &mut c,
                "select * from emp where deptno in (select d.dn| from dept d)"
            ),
            ["dname"]
        );
        assert_eq!(texts(&mut c, "select dept.d"), ["deptno", "dname"]);
        assert_eq!(texts(&mut c, "select * from scott.e"), ["emp"]);
    }

    #[test]
    fn unknown_schema_is_requested_once() {
        let mut c = catalog();
        assert!(texts(&mut c, "select * from hr.").is_empty());
        assert!(texts(&mut c, "select * from hr.").is_empty());
        assert!(texts(&mut c, "select * from nobody.").is_empty());
        assert_eq!(c.wanted, ["HR"]);
        c.wanted.clear();
        let start = "select * from hr.".len();
        c.set_schema(
            "HR".into(),
            [(
                "JOBS".to_string(),
                Table {
                    kind: "VIEW".into(),
                    columns: vec![],
                },
            )]
            .into(),
            false,
        );
        assert_eq!(
            c.complete("select * from hr.", start),
            [Completion::new("JOBS", start).with_meta("VIEW")]
        );
    }

    #[test]
    fn synonyms_complete_as_tables_and_resolve_their_columns() {
        let mut c = catalog();
        let start = "select * from ".len();
        assert_eq!(
            c.complete("select * from du", start + 2),
            [Completion::new("dual", start).with_meta("SYNONYM")]
        );
        assert!(!texts(&mut c, "select du| from dual").contains(&"dummy".to_string()));
        assert_eq!(c.wanted, ["SYS"]);
        c.set_schema(
            "SYS".into(),
            [(
                "DUAL".to_string(),
                Table {
                    kind: "TABLE".into(),
                    columns: vec!["DUMMY".into()],
                },
            )]
            .into(),
            false,
        );
        assert_eq!(texts(&mut c, "select du| from dual"), ["dummy"]);
        assert_eq!(texts(&mut c, "select dual.d"), ["dummy"]);
    }
}
