//! Translations. The `.po` catalog is the one source: Slint bundles it for `@tr`, and `t` reads the same file for text
//! built in Rust and messages from the core.
use std::collections::HashMap;
use std::sync::OnceLock;

const ES: &str = include_str!("../lang/es/LC_MESSAGES/csgo-legacy-server-app.po");

/// Reads `msgid`/`msgstr` pairs. Only what the catalog uses: one line per string, no plurals.
fn parse(po: &str) -> HashMap<String, String> {
    let unquote = |s: &str| s.trim().trim_matches('"').replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\");
    let mut out = HashMap::new();
    let mut id = None;
    for l in po.lines() {
        if let Some(v) = l.strip_prefix("msgid ") {
            id = Some(unquote(v));
        } else if let Some(v) = l.strip_prefix("msgstr ") {
            if let Some(k) = id.take().filter(|k: &String| !k.is_empty()) {
                out.insert(k, unquote(v));
            }
        }
    }
    out
}

fn table() -> &'static HashMap<String, String> {
    static T: OnceLock<HashMap<String, String>> = OnceLock::new();
    T.get_or_init(|| if csls_core::lang() == "es" { parse(ES) } else { HashMap::new() })
}

pub fn t(s: &str) -> String {
    table().get(s).cloned().unwrap_or_else(|| s.to_string())
}

/// Translates a message from the core: whole, or its fixed part before ": " ("Step: details").
/// Messages with data in the middle (paths, SteamCMD output) stay in English.
pub fn t_msg(s: &str) -> String {
    match s.split_once(": ") {
        Some((head, rest)) if !table().contains_key(s) => format!("{}: {rest}", t(head)),
        _ => t(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_keep_their_details() {
        // in tests the language is English: the message comes back whole
        assert_eq!(t_msg("CS:GO server (app 740, ~33 GB): downloading"), "CS:GO server (app 740, ~33 GB): downloading");
        assert_eq!(t_msg("no colon here"), "no colon here");
    }

    #[test]
    fn reads_the_catalog() {
        let m = parse(ES);
        assert_eq!(m.get("Cancel").map(String::as_str), Some("Cancelar"));
        assert!(m.len() > 100);
        assert!(!m.contains_key(""));
    }

    #[test]
    fn no_duplicates() {
        let ids: Vec<&str> = ES.lines().filter(|l| l.starts_with("msgid ")).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
    }

    fn literals(text: &str, open: &str, out: &mut Vec<String>) {
        for (i, _) in text.match_indices(open) {
            // only the function, not get(, set(…
            if open.starts_with('t') && text[..i].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                continue;
            }
            let part = &text[i + open.len()..];
            if let Some(end) = part.find("\")") {
                let k = &part[..end];
                if !k.contains('"') {
                    out.push(k.to_string());
                }
            }
        }
    }

    /// Every literal in `t("…")` and `@tr("…")` has a translation.
    #[test]
    fn everything_translated() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let m = parse(ES);
        let mut seen = Vec::new();
        for (dir, open) in [("src", "t(\""), ("ui", "@tr(\"")] {
            for e in std::fs::read_dir(root.join(dir)).unwrap().flatten() {
                // icons and other assets live in subfolders
                if !e.path().extension().is_some_and(|x| x == "rs" || x == "slint") {
                    continue;
                }
                let txt = std::fs::read_to_string(e.path()).unwrap();
                // the tests above are not user text
                let txt = txt.split("#[cfg(test)]").next().unwrap();
                literals(txt, open, &mut seen);
            }
        }
        let mut missing: Vec<String> = seen.into_iter().filter(|k| !m.contains_key(k)).collect();
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "untranslated: {missing:#?}");
    }
}
