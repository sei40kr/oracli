/// Credentials and target parsed from SQLcl-style `user[/password][@connect]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logon {
    pub user: String,
    pub password: Option<String>,
    /// Easy Connect string such as `host:1521/service`; a TNS alias is resolved at parse time.
    pub connect: String,
}

impl Logon {
    /// Parses `user[/password][@connect]`. Without `@connect`, `two_task` (the value of
    /// `$TWO_TASK`) is the connect string, as in SQL*Plus. Either may be a `tnsnames.ora` alias.
    pub fn parse(s: &str, two_task: Option<&str>) -> Result<Self, String> {
        let s = s.trim();
        let (cred, connect) = match s.rsplit_once('@') {
            Some((cred, connect)) => (cred, connect.to_owned()),
            None => match two_task {
                Some(t) if !t.is_empty() => (s, t.to_owned()),
                _ => {
                    return Err(
                        "no connect identifier: use user@host:port/service, user@alias or set TWO_TASK".into(),
                    );
                }
            },
        };
        let (user, password) = match cred.split_once('/') {
            Some((u, p)) => (u, Some(p.to_owned())),
            None => (cred, None),
        };
        if user.is_empty() {
            return Err("missing user name".into());
        }
        Ok(Self {
            user: user.to_owned(),
            password,
            connect: crate::tnsnames::resolve(&connect)?,
        })
    }

    /// `user@connect`, without the password.
    pub fn label(&self) -> String {
        format!("{}@{}", self.user, self.connect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sqlcl_forms() {
        assert_eq!(
            Logon::parse("scott/ti@ger@//db:1521/FREEPDB1", None).unwrap(),
            Logon {
                user: "scott".into(),
                password: Some("ti@ger".into()),
                connect: "//db:1521/FREEPDB1".into(),
            }
        );
        let l = Logon::parse("scott@db/svc", None).unwrap();
        assert_eq!(l.password, None);
        assert_eq!(l.label(), "scott@db/svc");
    }

    #[test]
    fn falls_back_to_two_task() {
        assert_eq!(
            Logon::parse("scott/tiger", Some("db/svc")).unwrap().connect,
            "db/svc"
        );
        assert!(Logon::parse("scott/tiger", None).is_err());
        assert!(Logon::parse("scott", Some("")).is_err());
        assert!(Logon::parse("/pw@db/svc", None).is_err());
    }
}
