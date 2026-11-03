# tree-sitter-plsql

An Oracle SQL and PL/SQL grammar for [tree-sitter](https://tree-sitter.github.io). It drives highlighting, completion and statement termination in [oracli](../../README.md), and is meant to be usable from editors too.

## Coverage

- Queries: `WITH`, joins, set operations, subqueries, `CONNECT BY`, `GROUP BY`/`HAVING`, `ORDER BY`, `OFFSET`/`FETCH`, `FOR UPDATE`
- DML: `INSERT`, `UPDATE`, `DELETE`, `MERGE`
- DDL: `CREATE TABLE`, `CREATE VIEW`, `CREATE INDEX`, `CREATE PROCEDURE`/`FUNCTION`, `DROP`, `TRUNCATE`
- Transactions: `COMMIT`, `ROLLBACK`, `SAVEPOINT`
- PL/SQL anonymous blocks: declarations, cursors, `IF`, loops, `EXECUTE IMMEDIATE`, `OPEN`/`FETCH`/`CLOSE`, exception handlers
- Expressions: `CASE`, `CAST`, analytic `OVER (...)`, bind variables, `q'[...]'` strings

Other statements, such as `ALTER`, `GRANT` and `CREATE PACKAGE`, are not parsed yet.

## Design

- **Contextual keywords.** Keywords are `keyword_*` nodes, matched case-insensitively. Where the grammar expects no keyword they lex as identifiers, so non-reserved words still work as names.
- **Tolerant of unfinished input.** The last statement may lack a terminator, and an unterminated string runs to the end of the input, so a tree is available while the user is still typing.
- **Statement terminators.** Statements end with `;` or with a line holding only `/`, the `slash` node, as in SQL*Plus and SQLcl.
- **External scanner.** `src/scanner.c` lexes what a regular expression cannot: `q'[...]'` strings and the `/` line.

## Rust

```toml
[dependencies]
tree-sitter = "0.26"
tree-sitter-plsql = { git = "https://github.com/sei40kr/oracli" }
```

```rust
let mut parser = tree_sitter::Parser::new();
parser.set_language(&tree_sitter_plsql::LANGUAGE.into())?;
let tree = parser.parse("select ename from emp where deptno = :d;\n", None).unwrap();
println!("{}", tree.root_node().to_sexp());
```

`NODE_TYPES` holds the contents of `src/node-types.json`.

## Development

`grammar.js` is the source. `src/parser.c`, `src/grammar.json` and `src/node-types.json` are generated from it and committed, so building needs only a C compiler. After editing the grammar:

```
tree-sitter generate
tree-sitter test
```

Corpus tests live in `test/corpus/`.

## License

[MIT](../../LICENSE)
