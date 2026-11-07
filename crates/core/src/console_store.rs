//! What the console keeps between sessions: command history, favorites and the cvars of this game build.
use crate::paths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const HISTORY_KEEP: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub history: Vec<String>,
    pub favorites: Vec<String>,
}

fn path() -> PathBuf {
    paths::config_dir().join("console.toml")
}

impl Store {
    pub fn load() -> Self {
        std::fs::read_to_string(path()).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> crate::Result<()> {
        std::fs::create_dir_all(paths::config_dir())?;
        std::fs::write(path(), toml::to_string(self)?)?;
        Ok(())
    }

    /// Commands with a password never get here.
    pub fn remember(&mut self, cmd: &str) {
        if crate::rcon::hides_secret(cmd) {
            return;
        }
        if self.history.last().map(String::as_str) != Some(cmd) {
            self.history.push(cmd.to_string());
        }
        let extra = self.history.len().saturating_sub(HISTORY_KEEP);
        self.history.drain(..extra);
    }

    pub fn toggle_favorite(&mut self, cmd: &str) {
        if crate::rcon::hides_secret(cmd) || cmd.trim().is_empty() {
            return;
        }
        match self.favorites.iter().position(|f| f == cmd) {
            Some(i) => {
                self.favorites.remove(i);
            }
            None => self.favorites.push(cmd.to_string()),
        }
    }
}

/// The names in the output of `cvarlist`: `name : value : flags : help`.
pub fn parse_cvarlist(out: &str) -> Vec<String> {
    let mut v: Vec<String> = out
        .lines()
        .filter_map(|l| l.split_once(" : ").map(|(n, _)| n.trim().to_string()))
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'+' || b == b'-'))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The game build from `steam.inf`, so the list is asked for again only after an update.
pub fn game_build(csgo: &Path) -> Option<String> {
    let t = std::fs::read_to_string(csgo.join("steam.inf")).ok()?;
    t.lines().find_map(|l| l.strip_prefix("PatchVersion=")).map(|v| v.trim().to_string())
}

fn cvars_path(build: &str) -> PathBuf {
    let safe: String = build.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
    paths::data_dir().join(format!("cvars-{safe}.txt"))
}

pub fn cached_cvars(build: &str) -> Option<Vec<String>> {
    Some(std::fs::read_to_string(cvars_path(build)).ok()?.lines().map(str::to_string).collect())
}

pub fn cache_cvars(build: &str, names: &[String]) -> crate::Result<()> {
    std::fs::create_dir_all(paths::data_dir())?;
    std::fs::write(cvars_path(build), names.join("\n"))?;
    Ok(())
}

/// A few names that start with what was typed, then those that contain it.
pub fn complete<'a>(names: &'a [String], typed: &str, limit: usize) -> Vec<&'a str> {
    let t = typed.trim().to_lowercase();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return Vec::new();
    }
    let start = names.iter().filter(|n| n.starts_with(&t));
    let inside = names.iter().filter(|n| !n.starts_with(&t) && n.contains(&t));
    start.chain(inside).take(limit).map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_stay_out() {
        let mut s = Store::default();
        s.remember("rcon_password x");
        s.remember("status");
        s.remember("status");
        s.toggle_favorite("sv_password y");
        assert_eq!(s.history, ["status"]);
        assert!(s.favorites.is_empty());
        s.toggle_favorite("status");
        assert_eq!(s.favorites, ["status"]);
        s.toggle_favorite("status");
        assert!(s.favorites.is_empty());
    }

    #[test]
    fn history_is_bounded() {
        let mut s = Store::default();
        for i in 0..HISTORY_KEEP + 5 {
            s.remember(&i.to_string());
        }
        assert_eq!(s.history.len(), HISTORY_KEEP);
        assert_eq!(s.history[0], "5");
    }

    #[test]
    fn reads_cvarlist_and_completes() {
        let out = "cvar list\n--------------\nbot_quota : 10 : , \"sv\" : Help\nmp_restartgame : 0 : , \"sv\" : x\n\n2 total convars/concommands\n";
        let names = parse_cvarlist(out);
        assert_eq!(names, ["bot_quota", "mp_restartgame"]);
        assert_eq!(complete(&names, "qu", 5), ["bot_quota"]);
        assert_eq!(complete(&names, "mp_", 5), ["mp_restartgame"]);
        assert!(complete(&names, "mp_restartgame 1", 5).is_empty());
        assert!(complete(&names, "", 5).is_empty());
    }
}
