//! Oracle SQL and PL/SQL grammar for tree-sitter.
//!
//! The parser is generated from `grammar.js`; regenerate `src/` with `tree-sitter generate`.

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_plsql() -> *const ();
}

pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_plsql) };

pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

#[cfg(test)]
mod tests {
    #[test]
    fn parses_without_errors() {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&super::LANGUAGE.into()).unwrap();
        let tree = parser.parse("select 1 from dual;\n/\n", None).unwrap();
        assert!(!tree.root_node().has_error());
    }
}
