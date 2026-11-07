//! Plugin manager: what is on the server, and turning it on, off, installing and rolling back.
use crate::config::Config;
use crate::deploy::{self, sourcemod_dir};
use crate::paths;
use anyhow::{bail, Context};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Without it no mode works, so it cannot be turned off.
const ESSENTIAL: &[&str] = &["modos"];
const HISTORY_KEEP: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Info {
    pub name: String,
    pub enabled: bool,
    /// "builtin", "user", or "server" for a compiled plugin with no sources here.
    pub origin: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub includes: Vec<String>,
    pub missing: Vec<String>,
    /// Another plugin with the same title.
    pub conflict: Option<String>,
}

/// Same rule as a mode id: this name ends up in file paths and in a console command.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn field(block: &str, key: &str) -> String {
    let Some(i) = block.find(key) else { return String::new() };
    let rest = &block[i + key.len()..];
    let Some(eq) = rest.find('=') else { return String::new() };
    let value = rest[eq + 1..].trim_start();
    match value.strip_prefix('"') {
        Some(v) => v.split('"').next().unwrap_or("").to_string(),
        // a #define: all that can be said without compiling
        None => value.split([',', '\n', '}']).next().unwrap_or("").trim().to_string(),
    }
}

/// (title, author, version, description) from the `myinfo` block.
pub fn parse_myinfo(src: &str) -> (String, String, String, String) {
    let Some(i) = src.find("myinfo") else { return Default::default() };
    let block = &src[i..];
    let block = &block[..block.find("};").unwrap_or(block.len())];
    (field(block, "name"), field(block, "author"), field(block, "version"), field(block, "description"))
}

pub fn parse_includes(src: &str) -> Vec<String> {
    let mut v: Vec<String> = src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("#include"))
        .map(|r| r.trim().trim_matches(['<', '>', '"']).trim_end_matches(".inc").to_string())
        .filter(|n| !n.is_empty())
        .collect();
    v.sort();
    v.dedup();
    v
}

fn has_include(name: &str, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|d| ["", ".inc", ".sp"].iter().any(|ext| d.join(format!("{name}{ext}")).is_file()))
}

pub fn list(cfg: &Config) -> crate::Result<Vec<Info>> {
    let sm = sourcemod_dir(cfg);
    let res = paths::resources_dir()?;
    let mut out = Vec::new();
    let mut add = |p: &deploy::Plugin, origin: &str| {
        let src = std::fs::read_to_string(&p.main).unwrap_or_default();
        let (title, author, version, description) = parse_myinfo(&src);
        let includes = parse_includes(&src);
        let dirs = [sm.join("scripting/include"), p.dir.clone(), p.main.parent().unwrap_or(&p.dir).to_path_buf()];
        let missing = includes.iter().filter(|n| !has_include(n, &dirs)).cloned().collect();
        let off = sm.join("plugins/disabled").join(format!("{}.smx", p.name)).is_file();
        out.push(Info {
            name: p.name.clone(),
            enabled: !off,
            origin: origin.into(),
            title,
            author,
            version,
            description,
            includes,
            missing,
            conflict: None,
        });
    };
    let builtin = deploy::find_plugins(&res.join("plugin"));
    builtin.iter().for_each(|p| add(p, "builtin"));
    deploy::find_plugins(&paths::user_plugins_dir()).iter().for_each(|p| add(p, "user"));

    // compiled plugins that came with SourceMod or were copied by hand
    for (dir, enabled) in [(sm.join("plugins"), true), (sm.join("plugins/disabled"), false)] {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let Some(name) = p.file_stem().and_then(|s| s.to_str()).filter(|_| p.extension().is_some_and(|x| x == "smx")) else { continue };
            if !out.iter().any(|i| i.name == name) {
                out.push(Info { name: name.into(), enabled, origin: "server".into(), ..Default::default() });
            }
        }
    }
    let titles: Vec<(String, String)> = out.iter().filter(|i| !i.title.is_empty()).map(|i| (i.name.clone(), i.title.clone())).collect();
    for i in &mut out {
        i.conflict = titles.iter().find(|(n, t)| *n != i.name && *t == i.title).map(|(n, _)| n.clone());
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Turns a plugin on or off by moving it, the way SourceMod expects. A running server notices with `reload`.
pub fn set_enabled(cfg: &Config, name: &str, on: bool) -> crate::Result<()> {
    if !valid_name(name) {
        bail!("invalid plugin name '{name}'");
    }
    if !on && ESSENTIAL.contains(&name) {
        bail!("{name} is needed for the modes to work");
    }
    let sm = sourcemod_dir(cfg);
    let (active, off) = (sm.join("plugins").join(format!("{name}.smx")), sm.join("plugins/disabled").join(format!("{name}.smx")));
    let (from, to) = if on { (&off, &active) } else { (&active, &off) };
    if !from.is_file() {
        bail!("{} is not there: deploy the plugins first", from.display());
    }
    std::fs::create_dir_all(sm.join("plugins/disabled"))?;
    std::fs::rename(from, to)?;
    Ok(())
}

/// Only tells versions apart in the history, as with the plugin stamps of `deploy`.
fn hash(b: &[u8]) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    b.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn history_dir(name: &str) -> PathBuf {
    paths::data_dir().join("plugin-history").join(name)
}

/// Keeps a copy of the source by content, for going back. A bounded number of versions.
fn archive(name: &str, source: &[u8]) -> crate::Result<()> {
    let dir = history_dir(name);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{}.sp", hash(source))), source)?;
    let mut files: Vec<(std::time::SystemTime, PathBuf)> =
        std::fs::read_dir(&dir)?.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    files.sort();
    let extra = files.len().saturating_sub(HISTORY_KEEP);
    for (_, p) in files.into_iter().take(extra) {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

/// The saved versions of a plugin, newest first.
pub fn history(name: &str) -> crate::Result<Vec<PathBuf>> {
    if !valid_name(name) {
        bail!("invalid plugin name '{name}'");
    }
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(history_dir(name))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort();
    Ok(files.into_iter().rev().map(|(_, p)| p).collect())
}

/// Installs a `.sp` (the deploy compiles it and says what is wrong) or a `.smx` (checked to be one).
/// Returns the plugin's name.
pub fn install(cfg: &Config, file: &Path) -> crate::Result<String> {
    let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    if !valid_name(&name) {
        bail!("invalid plugin name '{name}': letters, numbers, - and _");
    }
    let data = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    match file.extension().and_then(|e| e.to_str()) {
        Some("sp") => {
            let text = String::from_utf8_lossy(&data);
            if !text.contains("myinfo") {
                bail!("{name}.sp has no myinfo block: it does not look like a plugin");
            }
            let dir = paths::user_plugins_dir();
            std::fs::create_dir_all(&dir)?;
            let dest = dir.join(format!("{name}.sp"));
            if let Ok(old) = std::fs::read(&dest) {
                archive(&name, &old)?;
            }
            archive(&name, &data)?;
            std::fs::write(dest, &data)?;
        }
        Some("smx") => {
            // the "FFPS" magic of a SourcePawn file
            if data.get(..4) != Some(&[0x46, 0x46, 0x50, 0x53]) {
                bail!("{name}.smx is not a compiled SourceMod plugin");
            }
            let dir = sourcemod_dir(cfg).join("plugins");
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join(format!("{name}.smx")), &data)?;
        }
        _ => bail!("a plugin is a .sp or .smx file"),
    }
    Ok(name)
}

/// Puts a saved version back as the plugin's source. It must be one from `history`.
pub fn rollback(name: &str, version: &Path) -> crate::Result<()> {
    if !history(name)?.iter().any(|p| p == version) {
        bail!("that is not a saved version of {name}");
    }
    let data = std::fs::read(version)?;
    std::fs::write(paths::user_plugins_dir().join(format!("{name}.sp")), data)?;
    Ok(())
}

pub fn reload_command(name: &str) -> crate::Result<String> {
    if !valid_name(name) {
        bail!("invalid plugin name '{name}'");
    }
    Ok(format!("sm plugins reload {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"#include <sourcemod>
#include "mine.inc"
#define V "1.2"
public Plugin myinfo = {
    name = "My plugin",
    author = "Someone",
    description = "Does things",
    version = V,
    url = ""
};
"#;

    #[test]
    fn reads_the_source() {
        let (t, a, v, d) = parse_myinfo(SRC);
        assert_eq!((t.as_str(), a.as_str(), v.as_str(), d.as_str()), ("My plugin", "Someone", "V", "Does things"));
        assert_eq!(parse_includes(SRC), ["mine", "sourcemod"]);
        assert_eq!(parse_myinfo("no block"), Default::default());
    }

    #[test]
    fn names_cannot_escape_a_folder_or_a_command() {
        for bad in ["", "../x", "a b", "a;quit", "a/b", "a\"b"] {
            assert!(!valid_name(bad), "{bad}");
            assert!(reload_command(bad).is_err());
        }
        assert_eq!(reload_command("my-plugin_2").unwrap(), "sm plugins reload my-plugin_2");
    }

    #[test]
    fn enabling_moves_the_file_and_the_essential_one_stays() {
        let tmp = std::env::temp_dir().join(format!("csls-pl-{}", std::process::id()));
        let mut cfg = Config::default();
        cfg.server.install_dir = tmp.clone();
        let sm = sourcemod_dir(&cfg);
        std::fs::create_dir_all(sm.join("plugins")).unwrap();
        std::fs::write(sm.join("plugins/x.smx"), "x").unwrap();
        std::fs::write(sm.join("plugins/modos.smx"), "x").unwrap();
        set_enabled(&cfg, "x", false).unwrap();
        assert!(sm.join("plugins/disabled/x.smx").is_file() && !sm.join("plugins/x.smx").exists());
        assert!(set_enabled(&cfg, "x", false).is_err(), "already off");
        set_enabled(&cfg, "x", true).unwrap();
        assert!(sm.join("plugins/x.smx").is_file());
        assert!(set_enabled(&cfg, "modos", false).is_err());
        assert!(set_enabled(&cfg, "../x", true).is_err());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn a_smx_must_be_one() {
        let tmp = std::env::temp_dir().join(format!("csls-pl2-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut cfg = Config::default();
        cfg.server.install_dir = tmp.join("srv");
        let bad = tmp.join("bad.smx");
        std::fs::write(&bad, "text").unwrap();
        assert!(install(&cfg, &bad).is_err());
        let good = tmp.join("good.smx");
        std::fs::write(&good, b"FFPSrest").unwrap();
        assert_eq!(install(&cfg, &good).unwrap(), "good");
        assert!(sourcemod_dir(&cfg).join("plugins/good.smx").is_file());
        let other = tmp.join("x.txt");
        std::fs::write(&other, "x").unwrap();
        assert!(install(&cfg, &other).is_err());
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
