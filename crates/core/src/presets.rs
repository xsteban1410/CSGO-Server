//! Presets: named saved match options (~/.config/csgo-legacy-server/presets.toml).
use crate::launch::LaunchOptions;
use crate::paths;
use anyhow::{bail, Context};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Presets(pub BTreeMap<String, LaunchOptions>);

pub fn path() -> PathBuf {
    paths::config_dir().join("presets.toml")
}

pub fn valid_name(n: &str) -> bool {
    !n.trim().is_empty() && n.len() <= 64 && !n.contains(['"', '\n', '\r', '[', ']'])
}

impl Presets {
    pub fn load() -> crate::Result<Presets> {
        Self::load_from(&path())
    }

    pub fn load_from(p: &Path) -> crate::Result<Presets> {
        match std::fs::read_to_string(p) {
            Ok(t) => toml::from_str(&t).with_context(|| format!("leyendo {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Presets::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_to(&self, p: &Path) -> crate::Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, toml::to_string(self)?)?;
        Ok(())
    }

    /// Saves (or replaces) a preset and writes the file.
    pub fn put(name: &str, o: &LaunchOptions) -> crate::Result<()> {
        if !valid_name(name) {
            bail!("invalid preset name '{name}'");
        }
        let mut p = Self::load()?;
        p.0.insert(name.trim().to_string(), o.clone());
        p.save_to(&path())
    }

    pub fn remove(name: &str) -> crate::Result<()> {
        let mut p = Self::load()?;
        if p.0.remove(name).is_none() {
            bail!("preset '{name}' does not exist");
        }
        p.save_to(&path())
    }

    pub fn get(&self, name: &str) -> crate::Result<&LaunchOptions> {
        self.0.get(name).with_context(|| format!("preset '{name}' does not exist"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::{parse_cvar, Site};

    #[test]
    fn round_trip() {
        let mut o = LaunchOptions::from_config(&crate::config::Config::default());
        o.mode = "retake".into();
        o.bots = Some(4);
        o.site = Some(Site::B);
        o.cvars.push(parse_cvar("sv_gravity=300").unwrap());
        let mut p = Presets::default();
        p.0.insert("Retake B".into(), o.clone());
        let f = std::env::temp_dir().join(format!("csls-presets-{}.toml", std::process::id()));
        p.save_to(&f).unwrap();
        let back = Presets::load_from(&f).unwrap();
        std::fs::remove_file(&f).ok();
        assert_eq!(back.get("Retake B").unwrap(), &o);
        assert!(!valid_name("a\"b") && valid_name("Mi preset"));
    }
}
