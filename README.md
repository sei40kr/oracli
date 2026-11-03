# oracli

A terminal client for Oracle Database, in the spirit of SQLcl and SQL*Plus but with an editor-grade prompt: syntax highlighting, completion from the data dictionary, and a scrollable result grid. Written in Rust on [ratatui](https://ratatui.rs), talking to Oracle through [oracle-rs](https://crates.io/crates/oracle-rs), so no Oracle Client libraries are needed.

```
Connected to scott@localhost:1521/FREEPDB1.

SQL> select empno, ename, job from emp;

EMPNO ENAME      JOB
----- ---------- -----
1     employee 1 CLERK
2     employee 2 CLERK
3     employee 3
3 row(s) selected.

SQL>
 scott@localhost:1521/FREEPDB1  Ready · F2 grid · F5 refresh · Ctrl-D quit
```

## Features

- Multi-line SQL and PL/SQL input with SQLcl's rules for when Enter runs the statement
- Highlighting and completion driven by a tree-sitter grammar for Oracle SQL and PL/SQL, so candidates fit the cursor's place in the statement: tables after `FROM`, columns of the tables the statement uses, `alias.` columns, `SCHEMA.` objects, synonyms
- Emacs key bindings, undo, kill ring, file-backed history with Ctrl-R search and fish-style suggestions
- Full-screen result grid that fetches more rows as you scroll
- Ctrl-C cancels a running statement
- `NO_COLOR` support

## Install

With Nix:

```
nix run github:sei40kr/oracli -- scott/tiger@localhost:1521/FREEPDB1
nix profile install github:sei40kr/oracli
```

With Cargo (needs a C compiler and CMake):

```
cargo install --git https://github.com/sei40kr/oracli oracli
```

## Usage

```
oracli user/pass@host:port/service
oracli user@host:port/service     # prompts for the password
oracli user                       # connect string from $TWO_TASK
oracli /nolog                     # start unconnected (also the default)
```

The connect identifier is an Easy Connect string. TNS aliases are not supported yet ([#5](https://github.com/sei40kr/oracli/issues/5)).

Inside the prompt:

| Command | |
|---|---|
| `CONNECT user/pass@host:port/service` | Connect or reconnect; the password must be inline for now ([#6](https://github.com/sei40kr/oracli/issues/6)) |
| `DESC [schema.]object` | Describe a table or view |
| `EXIT`, `QUIT` | Leave; warns once if DML is uncommitted |

### Running statements

Enter runs the input when it ends with `;`, or with a line holding only `/`. Otherwise it inserts a newline. PL/SQL blocks end with their own `end;`, so they need the `/` line:

```
SQL> begin
       dbms_output.put_line('hi');
     end;
     /
```

Alt-Enter runs the input as it is. One input runs one statement.

### Keys

| Key | Action |
|---|---|
| Enter / Alt-Enter | Run (see above) / run now |
| Ctrl-O | Insert a newline |
| Tab, Shift-Tab | Complete, next / previous candidate |
| Esc | Close the completion menu and restore what was typed |
| Up, Down | Previous / next line, then history |
| Ctrl-R | Search history |
| → or Ctrl-E at the end | Accept the history suggestion |
| Ctrl-_ / Alt-_ | Undo / redo |
| Ctrl-K, Ctrl-U, Ctrl-W, Alt-D, Ctrl-Y | Kill and yank |
| PageUp, PageDown | Scroll the result log |
| F2 | Open the latest result in the grid (q or Esc closes it) |
| F5 | Reload the current schema for completion |
| Ctrl-C | Cancel the running statement, or clear the input |
| Ctrl-D | Quit on an empty input |

History is saved to `$XDG_DATA_HOME/oracli/history` (`~/.local/share/oracli/history`).

## Limitations

oracle-rs 0.1.7 has a few gaps that oracli works around:

- Results page by re-running the query with `OFFSET ... FETCH NEXT`, so pages can shift without a stable `ORDER BY`.
- A SQL error closes the session. oracli reconnects, but uncommitted work is lost and the `ORA-` code is not shown.
- Ctrl-C drops and reopens the connection, which also loses uncommitted work.

Statements the grammar does not know yet, such as `ALTER`, `GRANT` and `CREATE PACKAGE`, are not recognized as finished ([#1](https://github.com/sei40kr/oracli/issues/1)). Known gaps are tracked in [issues](https://github.com/sei40kr/oracli/issues).

## Development

The workspace holds three crates:

| Crate | |
|---|---|
| [`crates/oracli`](crates/oracli) | The application |
| [`crates/ratatui-ptk`](crates/ratatui-ptk) | A prompt_toolkit-like input library for ratatui, with nothing Oracle-specific |
| [`crates/tree-sitter-plsql`](crates/tree-sitter-plsql) | The tree-sitter grammar for Oracle SQL and PL/SQL |

```
nix develop                  # or direnv; Rust, tree-sitter CLI, cargo-insta, mold, sccache
cargo test                   # unit and snapshot tests
nix flake check              # clippy, tests, formatting
```

Integration tests run against Oracle Database Free in a container:

```
docker compose up -d --wait  # scott/tiger@localhost:1521/FREEPDB1
cargo test -- --ignored
docker compose down
```

After editing `crates/tree-sitter-plsql/grammar.js`, regenerate and test the parser:

```
cd crates/tree-sitter-plsql
tree-sitter generate
tree-sitter test
```

## License

[MIT](LICENSE)
