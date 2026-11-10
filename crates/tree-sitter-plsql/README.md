# tree-sitter-plsql

An Oracle SQL and PL/SQL grammar for [tree-sitter](https://tree-sitter.github.io). It drives highlighting, completion and statement termination in [oracli](../../README.md), and is meant to be usable from editors too.

## Coverage

| Area | Parsed | Not yet |
|---|---|---|
| Queries | `WITH`, joins, `UNION` / `INTERSECT` / `MINUS` / `EXCEPT`, subqueries, `CONNECT BY`, `GROUP BY` / `HAVING`, `ORDER BY`, `OFFSET` / `FETCH`, `FOR UPDATE` | `PIVOT` / `UNPIVOT`, `MODEL`, flashback, `SAMPLE`, `WITHIN GROUP`, `KEEP` |
| DML | `INSERT`, `UPDATE`, `DELETE`, `MERGE` | `RETURNING INTO`, multi-table `INSERT` |
| DDL | `CREATE TABLE` (constraints, `AS` query), `CREATE VIEW`, `CREATE INDEX`, `CREATE PROCEDURE` / `FUNCTION`, `DROP`, `TRUNCATE` | `ALTER`, `GRANT` / `REVOKE`, `COMMENT ON`, `CREATE SEQUENCE` / `SYNONYM` / `USER`, `CREATE PACKAGE` / `TRIGGER` / `TYPE` ([#1](https://github.com/sei40kr/oracli/issues/1)) |
| Transactions | `COMMIT`, `ROLLBACK [TO SAVEPOINT]`, `SAVEPOINT` | `SET TRANSACTION` |
| PL/SQL | anonymous blocks, variable / cursor / exception declarations, `%TYPE` / `%ROWTYPE`, assignment, calls, `IF`, `LOOP` / `WHILE` / `FOR` (range and cursor), `EXIT`, `RETURN`, `RAISE`, `EXECUTE IMMEDIATE`, `OPEN` / `FETCH` / `CLOSE`, `BULK COLLECT INTO`, exception handlers | `CASE` statement, `GOTO`, `CONTINUE`, `FORALL`, records and collection types, `PRAGMA`, packages, triggers |
| Expressions | `CASE`, `CAST`, analytic `OVER (...)`, `EXISTS`, `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `ANY` / `ALL`, `PRIOR`, bind variables, `q'[...]'` strings, `DATE` literals | `INTERVAL` / `TIMESTAMP` literals, `MULTISET` |
| Terminators | `;` and a line holding only `/` | SQL*Plus client commands such as `SET`, `@` and `SPOOL` |

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
