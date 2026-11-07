//! Minimal parser for Valve KeyValues ("key" "value" / "key" { ... }).
#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Str(String),
    Obj(Vec<(String, Kv)>),
}

impl Kv {
    pub fn get(&self, k: &str) -> Option<&Kv> {
        match self {
            Kv::Obj(v) => v.iter().find(|(kk, _)| kk.eq_ignore_ascii_case(k)).map(|(_, v)| v),
            Kv::Str(_) => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Kv::Str(s) => Some(s),
            Kv::Obj(_) => None,
        }
    }
    pub fn entries(&self) -> &[(String, Kv)] {
        match self {
            Kv::Obj(v) => v,
            Kv::Str(_) => &[],
        }
    }
}

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut t = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = it.next() {
                                t.push(match n {
                                    'n' => '\n',
                                    't' => '\t',
                                    o => o,
                                });
                            }
                        }
                        '"' => break,
                        o => t.push(o),
                    }
                }
                out.push(t);
            }
            '{' | '}' => out.push(c.to_string() + "\u{0}"),
            '/' if it.peek() == Some(&'/') => {
                for c in it.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                // unquoted word
                let mut t = c.to_string();
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() || n == '"' || n == '{' || n == '}' {
                        break;
                    }
                    t.push(n);
                    it.next();
                }
                // platform conditions like [$WIN32] are neither keys nor values
                if !(t.starts_with('[') && t.ends_with(']')) {
                    out.push(t);
                }
            }
        }
    }
    out
}

/// Parses a document; the root is an object with the top-level keys.
pub fn parse(s: &str) -> Kv {
    let toks = tokens(s);
    let mut stack: Vec<Vec<(String, Kv)>> = vec![Vec::new()];
    let mut keys: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for t in toks {
        if t == "{\u{0}" {
            keys.push(pending.take().unwrap_or_default());
            stack.push(Vec::new());
        } else if t == "}\u{0}" {
            if stack.len() > 1 {
                let obj = stack.pop().unwrap();
                let k = keys.pop().unwrap_or_default();
                stack.last_mut().unwrap().push((k, Kv::Obj(obj)));
            }
        } else if let Some(k) = pending.take() {
            stack.last_mut().unwrap().push((k, Kv::Str(t)));
        } else {
            pending = Some(t);
        }
    }
    while stack.len() > 1 {
        let obj = stack.pop().unwrap();
        let k = keys.pop().unwrap_or_default();
        stack.last_mut().unwrap().push((k, Kv::Obj(obj)));
    }
    Kv::Obj(stack.pop().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basico() {
        let k = parse("\"skins\"\n{\n // c\n \"a\" \"1\"\n \"b\" { \"c\" \"x \\\"y\\\"\" }\n}\n");
        let s = k.get("skins").unwrap();
        assert_eq!(s.get("a").unwrap().str(), Some("1"));
        assert_eq!(s.get("b").unwrap().get("c").unwrap().str(), Some("x \"y\""));
        let c = parse("\"t\" { \"a\" \"1\" [$X360] \"b\" \"2\" }");
        assert_eq!(c.get("t").unwrap().get("b").unwrap().str(), Some("2"));
    }
}
