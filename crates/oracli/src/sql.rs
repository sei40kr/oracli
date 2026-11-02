//! SQL text handling: parsing, SQLcl statement termination, and highlighting.

use std::cell::RefCell;
use std::ops::Range;

use ratatui_ptk::Lexer;
use tree_sitter::{Node, Parser, Tree};

pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$' | '#')
}

/// The statement's leading word, upper-cased, after any comments.
pub fn first_keyword(s: &str) -> Option<String> {
    let tree = parse(s);
    let first = statements(&tree).into_iter().next()?;
    let word: String = s[first.start_byte()..]
        .chars()
        .take_while(|&c| is_ident_char(c))
        .collect();
    (!word.is_empty()).then(|| word.to_ascii_uppercase())
}

/// A client command handled by oracli instead of the database.
#[derive(Debug, PartialEq, Eq)]
pub enum ClientCommand<'a> {
    Exit,
    Connect(&'a str),
    Describe(&'a str),
}

pub fn client_command(s: &str) -> Option<ClientCommand<'_>> {
    let s = s.trim().trim_end_matches(';').trim_end();
    let (head, rest) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
    let rest = rest.trim();
    match head.to_ascii_uppercase().as_str() {
        "EXIT" | "QUIT" => Some(ClientCommand::Exit),
        "CONN" | "CONNECT" => Some(ClientCommand::Connect(rest)),
        "DESC" | "DESCRIBE" => Some(ClientCommand::Describe(rest)),
        _ => None,
    }
}

/// The top-level nodes other than comments: statements, `;`, `/` lines, and ERROR nodes.
fn statements(tree: &Tree) -> Vec<Node<'_>> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    root.children(&mut cursor)
        .filter(|n| n.kind() != "comment")
        .collect()
}

/// SQLcl's rule for Enter: client commands run at once; SQL runs when ended by `;` or a `/` line.
/// A PL/SQL block's own `end;` belongs to the block, so blocks still wait for the `/` line.
pub fn is_complete(s: &str) -> bool {
    if s.trim().is_empty() || client_command(s).is_some() {
        return true;
    }
    let tree = parse(s);
    statements(&tree)
        .last()
        .is_some_and(|n| matches!(n.kind(), ";" | "slash"))
}

/// The statement to send to the server: drops the `/` line, and the trailing `;` outside PL/SQL.
pub fn prepare(s: &str) -> String {
    let tree = parse(s);
    let mut nodes = statements(&tree);
    if nodes.last().is_some_and(|n| n.kind() == "slash") {
        nodes.pop();
    }
    if nodes.last().is_some_and(|n| n.kind() == ";") {
        nodes.pop();
    }
    match (nodes.first(), nodes.last()) {
        (Some(first), Some(last)) => s[first.start_byte()..last.end_byte()].to_owned(),
        _ => String::new(),
    }
}

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut p = Parser::new();
        p.set_language(&tree_sitter_plsql::LANGUAGE.into())
            .expect("the grammar is generated for this tree-sitter version");
        p
    });
}

/// Parses Oracle SQL or PL/SQL. Incomplete input yields a tree with ERROR nodes, never a failure.
pub fn parse(text: &str) -> Tree {
    PARSER
        .with_borrow_mut(|p| p.parse(text, None))
        .expect("parsing fails only on timeout or cancellation, and neither is set")
}

pub struct SqlLexer;

impl Lexer for SqlLexer {
    fn lex(&self, text: &str) -> Vec<(Range<usize>, &'static str)> {
        let mut out = Vec::new();
        highlight(parse(text).root_node(), text, &mut out);
        out
    }
}

/// Tags the tokens under `node` with a class by node kind.
fn highlight(node: Node, text: &str, out: &mut Vec<(Range<usize>, &'static str)>) {
    let kind = node.kind();
    let class = match kind {
        _ if kind.starts_with("keyword_") => Some("keyword"),
        "string" | "q_string" => Some("string"),
        "number" => Some("number"),
        "identifier" if text[node.byte_range()].starts_with('"') => Some("quoted_identifier"),
        "comment" => Some("comment"),
        _ => None,
    };
    if let Some(class) = class {
        if !node.byte_range().is_empty() {
            out.push((node.byte_range(), class));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        highlight(child, text, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_rules_match_sqlcl() {
        assert!(is_complete("select 1 from dual;"));
        assert!(is_complete("select 1 from dual; -- done"));
        assert!(!is_complete("select ';'"));
        assert!(!is_complete("select 1\nfrom dual"));
        assert!(is_complete("desc emp"));
        assert!(is_complete("exit"));
        assert!(!is_complete("begin\n  null;\nend;"));
        assert!(is_complete("begin\n  null;\nend;\n/"));
        assert!(!is_complete(
            "create or replace procedure p is begin null; end;"
        ));
        assert!(is_complete("create table t (x number);"));
        assert!(is_complete("select q'[it's;]' from dual;"));
        assert!(!is_complete("select q'[a;]'"));
    }

    #[test]
    fn prepare_strips_terminators() {
        assert_eq!(prepare("select 1 from dual;  "), "select 1 from dual");
        assert_eq!(prepare("select 1 from dual\n/"), "select 1 from dual");
        assert_eq!(prepare("begin\n  null;\nend;\n/\n"), "begin\n  null;\nend;");
        assert_eq!(
            prepare("-- c\nselect 1 from dual; -- d"),
            "select 1 from dual"
        );
    }

    #[test]
    fn first_keyword_skips_comments() {
        assert_eq!(
            first_keyword("/* c */ insert into t values (1)").as_deref(),
            Some("INSERT")
        );
        assert_eq!(
            first_keyword("alter table t add x number").as_deref(),
            Some("ALTER")
        );
        assert_eq!(first_keyword("-- only"), None);
    }

    #[test]
    fn highlights_keywords_by_position() {
        let text = "select type, 'a' from \"T\" -- c";
        let spans: Vec<_> = SqlLexer
            .lex(text)
            .into_iter()
            .map(|(r, class)| (&text[r], class))
            .collect();
        assert_eq!(
            spans,
            [
                ("select", "keyword"),
                ("'a'", "string"),
                ("from", "keyword"),
                ("\"T\"", "quoted_identifier"),
                ("-- c", "comment"),
            ]
        );
    }

    #[test]
    fn recognizes_client_commands() {
        assert_eq!(client_command("quit"), Some(ClientCommand::Exit));
        assert_eq!(
            client_command("DESC hr.emp;"),
            Some(ClientCommand::Describe("hr.emp"))
        );
        assert_eq!(
            client_command("conn scott/tiger@db/svc"),
            Some(ClientCommand::Connect("scott/tiger@db/svc"))
        );
        assert_eq!(client_command("select 1"), None);
    }
}
