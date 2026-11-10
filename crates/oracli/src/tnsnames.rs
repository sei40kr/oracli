//! TNS alias resolution through `tnsnames.ora`, as SQL*Plus does it.

use std::path::PathBuf;

/// Turns a connect identifier into an Easy Connect string. Anything with `:` or `/` already is
/// one; everything else is looked up as an alias in `tnsnames.ora`.
pub fn resolve(connect: &str) -> Result<String, String> {
    if connect.contains([':', '/']) {
        return Ok(connect.to_owned());
    }
    let Some(path) = tnsnames_path() else {
        return Err(format!(
            "TNS alias {connect}: no tnsnames.ora found; set TNS_ADMIN or ORACLE_HOME"
        ));
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    lookup(&text, connect).map_err(|e| format!("{}: {e}", path.display()))
}

/// `$TNS_ADMIN/tnsnames.ora`, then `$ORACLE_HOME/network/admin/tnsnames.ora`.
fn tnsnames_path() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("TNS_ADMIN").map(PathBuf::from),
        std::env::var_os("ORACLE_HOME").map(|h| PathBuf::from(h).join("network/admin")),
    ];
    candidates
        .into_iter()
        .flatten()
        .map(|dir| dir.join("tnsnames.ora"))
        .find(|p| p.is_file())
}

/// Finds `alias` (case-insensitively) in the text of a `tnsnames.ora` and converts its entry.
pub fn lookup(text: &str, alias: &str) -> Result<String, String> {
    let entries = parse(text)?;
    let entry = entries
        .iter()
        .find(|(names, _)| names.iter().any(|n| n.eq_ignore_ascii_case(alias)))
        .ok_or_else(|| format!("TNS alias {alias} not found"))?;
    easy_connect(&entry.1).map_err(|e| format!("TNS alias {alias}: {e}"))
}

#[derive(Debug, PartialEq, Eq)]
enum Value {
    Scalar(String),
    List(Vec<(String, Value)>),
}

impl Value {
    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::List(items) => items
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Value::Scalar(_) => None,
        }
    }

    fn scalar(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Scalar(s) => Some(s),
            Value::List(_) => None,
        }
    }

    /// Every `ADDRESS` under this value, however deeply it sits in `ADDRESS_LIST`s.
    fn addresses<'a>(&'a self, out: &mut Vec<&'a Value>) {
        if let Value::List(items) = self {
            for (k, v) in items {
                if k.eq_ignore_ascii_case("ADDRESS") {
                    out.push(v);
                } else {
                    v.addresses(out);
                }
            }
        }
    }
}

/// `host:port/service` or `host:port:sid` from an entry's value, or why oracle-rs cannot
/// express it.
fn easy_connect(entry: &Value) -> Result<String, String> {
    let desc = entry
        .get("DESCRIPTION")
        .ok_or("entry has no DESCRIPTION (DESCRIPTION_LIST is not supported)")?;
    let mut addresses = Vec::new();
    desc.addresses(&mut addresses);
    let address = match addresses.as_slice() {
        [a] => *a,
        [] => return Err("no ADDRESS".into()),
        _ => {
            return Err(
                "more than one ADDRESS (failover and load balancing are not supported)".into(),
            );
        }
    };
    let protocol = address.scalar("PROTOCOL").unwrap_or("TCP");
    if !protocol.eq_ignore_ascii_case("TCP") {
        return Err(format!("PROTOCOL={protocol} is not supported, only TCP"));
    }
    let host = address.scalar("HOST").ok_or("ADDRESS has no HOST")?;
    let port = address.scalar("PORT").unwrap_or("1521");
    let data = desc.get("CONNECT_DATA").ok_or("no CONNECT_DATA")?;
    if let Some(service) = data.scalar("SERVICE_NAME") {
        Ok(format!("{host}:{port}/{service}"))
    } else if let Some(sid) = data.scalar("SID") {
        Ok(format!("{host}:{port}:{sid}"))
    } else {
        Err("CONNECT_DATA has neither SERVICE_NAME nor SID".into())
    }
}

/// Parses a whole `tnsnames.ora`: `ALIAS[, ALIAS...] = (KEY = VALUE)...` entries, `#` comments.
// shortcut: IFILE includes are not followed; add them when someone's tnsnames.ora uses them.
fn parse(text: &str) -> Result<Vec<(Vec<String>, Value)>, String> {
    let mut p = Parser { s: text, pos: 0 };
    let mut entries = Vec::new();
    loop {
        p.skip_space();
        if p.rest().is_empty() {
            return Ok(entries);
        }
        let names = p
            .take_until(&['='])
            .ok_or("expected '=' after alias name")?;
        p.pos += 1;
        let names = names.split(',').map(|n| n.trim().to_owned()).collect();
        entries.push((names, p.value()?));
    }
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.pos..]
    }

    fn skip_space(&mut self) {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            if trimmed.starts_with('#') {
                self.pos += trimmed.find('\n').unwrap_or(trimmed.len());
            } else {
                return;
            }
        }
    }

    /// Text up to (not including) the first of `stops`; the cursor lands on the stop.
    fn take_until(&mut self, stops: &[char]) -> Option<&'a str> {
        let rest = self.rest();
        let end = rest.find(stops)?;
        self.pos += end;
        Some(rest[..end].trim())
    }

    /// Either a run of `(KEY = VALUE)` groups or a scalar ending at `)` or a newline.
    fn value(&mut self) -> Result<Value, String> {
        self.skip_space();
        if !self.rest().starts_with('(') {
            let v = match self.take_until(&[')', '\n']) {
                Some(v) => v,
                None => {
                    let r = self.rest();
                    self.pos = self.s.len();
                    r
                }
            };
            return Ok(Value::Scalar(v.trim().to_owned()));
        }
        let mut items = Vec::new();
        while self.rest().starts_with('(') {
            self.pos += 1;
            self.skip_space();
            let key = self
                .take_until(&['='])
                .ok_or("expected '=' inside parentheses")?
                .to_owned();
            self.pos += 1;
            let v = self.value()?;
            self.skip_space();
            if !self.rest().starts_with(')') {
                return Err(format!("expected ')' after {key}"));
            }
            self.pos += 1;
            items.push((key, v));
            self.skip_space();
        }
        Ok(Value::List(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# tnsnames.ora Network Configuration File
FREE =
  (DESCRIPTION =
    (ADDRESS = (PROTOCOL = TCP)(HOST = db.example.com)(PORT = 1522))
    (CONNECT_DATA =
      (SERVER = DEDICATED)
      (SERVICE_NAME = FREEPDB1)   # the pluggable database
    )
  )

legacy, LEGACY2 = (DESCRIPTION=(ADDRESS=(PROTOCOL=tcp)(HOST=old))(CONNECT_DATA=(SID=ORCL)))

RAC =
  (DESCRIPTION =
    (ADDRESS_LIST =
      (LOAD_BALANCE = on)
      (ADDRESS = (PROTOCOL = TCP)(HOST = rac1)(PORT = 1521))
      (ADDRESS = (PROTOCOL = TCP)(HOST = rac2)(PORT = 1521))
    )
    (CONNECT_DATA = (SERVICE_NAME = rac))
  )

SECURE = (DESCRIPTION=(ADDRESS=(PROTOCOL=TCPS)(HOST=h)(PORT=2484))(CONNECT_DATA=(SERVICE_NAME=s)))
"#;

    #[test]
    fn resolves_aliases() {
        assert_eq!(
            lookup(SAMPLE, "free").unwrap(),
            "db.example.com:1522/FREEPDB1"
        );
        assert_eq!(lookup(SAMPLE, "LEGACY").unwrap(), "old:1521:ORCL");
        assert_eq!(lookup(SAMPLE, "legacy2").unwrap(), "old:1521:ORCL");
    }

    #[test]
    fn rejects_what_oracle_rs_cannot_express() {
        assert!(
            lookup(SAMPLE, "RAC")
                .unwrap_err()
                .contains("more than one ADDRESS")
        );
        assert!(lookup(SAMPLE, "SECURE").unwrap_err().contains("TCPS"));
        assert!(lookup(SAMPLE, "NOPE").unwrap_err().contains("not found"));
        assert!(lookup("X = (DESCRIPTION=(ADDRESS=(HOST=h)))", "X").is_err());
        assert!(lookup("X = (DESCRIPTION=(ADDRESS=(HOST=h)", "X").is_err());
    }

    #[test]
    fn passes_easy_connect_through() {
        assert_eq!(resolve("db:1521/svc").unwrap(), "db:1521/svc");
        assert_eq!(resolve("db/svc").unwrap(), "db/svc");
    }
}
