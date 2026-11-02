use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

/// Submitted entries, oldest first, optionally persisted to a file.
///
/// The file holds one entry per line with `\` and newlines escaped, so multi-line entries survive.
#[derive(Debug, Default)]
pub struct History {
    entries: Vec<String>,
    file: Option<File>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads the file if it exists and appends new entries to it.
    pub fn with_file(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let entries = match fs::read_to_string(&path) {
            Ok(s) => s.lines().map(unescape).collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            entries,
            file: Some(file),
        })
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Adds an entry unless it is blank or repeats the last one.
    pub fn push(&mut self, entry: &str) -> io::Result<()> {
        if entry.trim().is_empty() || self.entries.last().is_some_and(|l| l == entry) {
            return Ok(());
        }
        self.entries.push(entry.to_owned());
        if let Some(f) = &mut self.file {
            writeln!(f, "{}", escape(entry))?;
        }
        Ok(())
    }

    /// Newest entry that starts with `prefix` and is longer than it.
    pub fn suggest(&self, prefix: &str) -> Option<&str> {
        if prefix.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .rev()
            .find(|e| e.len() > prefix.len() && e.starts_with(prefix))
            .map(String::as_str)
    }

    /// Index of the newest entry before `before` that contains `query`.
    pub fn search(&self, query: &str, before: usize) -> Option<usize> {
        self.entries[..before.min(self.entries.len())]
            .iter()
            .rposition(|e| e.contains(query))
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_round_trips() {
        for s in ["select 1;", "begin\n  null;\nend;\n/", r"a\nb\\", "\\"] {
            assert_eq!(unescape(&escape(s)), s);
        }
    }

    #[test]
    fn file_persists_entries() {
        let path = std::env::temp_dir().join(format!("ratatui-ptk-history-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut h = History::with_file(&path).unwrap();
        h.push("select 1\nfrom dual;").unwrap();
        h.push("select 1\nfrom dual;").unwrap();
        h.push("  ").unwrap();
        drop(h);
        let h = History::with_file(&path).unwrap();
        assert_eq!(h.entries(), ["select 1\nfrom dual;"]);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn suggest_and_search_prefer_newest() {
        let mut h = History::new();
        for e in ["select 1;", "select 2;", "drop t;"] {
            h.push(e).unwrap();
        }
        assert_eq!(h.suggest("sel"), Some("select 2;"));
        assert_eq!(h.suggest("select 2;"), None);
        assert_eq!(h.search("select", 3), Some(1));
        assert_eq!(h.search("select", 1), Some(0));
        assert_eq!(h.search("nope", 3), None);
    }
}
