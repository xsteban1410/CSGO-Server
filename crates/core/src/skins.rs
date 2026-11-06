//! Skins saved per player (skins_prefs table in the SourceMod SQLite database) with names from skins.cfg.
use crate::kv::{self, Kv};
use anyhow::Context;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Catalog {
    /// weapon_x -> (nombre, pintura -> nombre)
    pub weapons: HashMap<String, (String, HashMap<String, String>)>,
    pub knives: HashMap<String, String>,
}

impl Catalog {
    pub fn load(skins_cfg: &Path) -> crate::Result<Catalog> {
        let k = kv::parse(&std::fs::read_to_string(skins_cfg).with_context(|| format!("leyendo {}", skins_cfg.display()))?);
        let root = k.get("skins").cloned().unwrap_or(Kv::Obj(vec![]));
        let mut c = Catalog::default();
        if let Some(a) = root.get("armas") {
            for (w, v) in a.entries() {
                let name = v.get("nombre").and_then(Kv::str).unwrap_or(w).to_string();
                let paints = v
                    .get("pintura")
                    .map(|p| p.entries().iter().filter_map(|(i, n)| Some((i.clone(), n.str()?.to_string()))).collect())
                    .unwrap_or_default();
                c.weapons.insert(w.clone(), (name, paints));
            }
        }
        if let Some(a) = root.get("cuchillos") {
            for (w, v) in a.entries() {
                if let Some(n) = v.str() {
                    c.knives.insert(w.clone(), n.to_string());
                }
            }
        }
        Ok(c)
    }

    pub fn weapon_name<'a>(&'a self, w: &'a str) -> &'a str {
        self.weapons.get(w).map(|x| x.0.as_str()).or(self.knives.get(w).map(String::as_str)).unwrap_or(w)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Weapon (or "Cuchillo" for the chosen model).
    pub weapon: String,
    /// Skin or model.
    pub value: String,
}

pub fn db_path(csgo_dir: &Path) -> PathBuf {
    csgo_dir.join("addons/sourcemod/data/sqlite/sourcemod-local.sq3")
}

fn open(db: &Path) -> crate::Result<rusqlite::Connection> {
    // read-only: the server may be writing
    Ok(rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?)
}

/// SteamID64 of every player with something saved.
pub fn players(db: &Path) -> crate::Result<Vec<String>> {
    let c = open(db)?;
    let mut st = c.prepare("SELECT DISTINCT steamid FROM skins_prefs ORDER BY steamid")?;
    let v = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(v)
}

pub fn player_skins(db: &Path, cat: &Catalog, steamid64: &str) -> crate::Result<Vec<Entry>> {
    let c = open(db)?;
    let mut st = c.prepare("SELECT clave, valor FROM skins_prefs WHERE steamid = ?1 ORDER BY clave")?;
    let rows = st.query_map([steamid64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut v = Vec::new();
    for row in rows {
        let (k, val) = row?;
        if k == "cuchillo" {
            v.push(Entry { weapon: "Cuchillo / Knife".into(), value: cat.weapon_name(&val).to_string() });
        } else {
            let paint = cat.weapons.get(&k).and_then(|w| w.1.get(&val)).cloned().unwrap_or_else(|| format!("#{val}"));
            v.push(Entry { weapon: cat.weapon_name(&k).to_string(), value: paint });
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_and_database() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins.cfg");
        let cat = Catalog::load(&root).unwrap();
        assert_eq!(cat.weapon_name("weapon_ak47"), "AK-47");
        assert_eq!(cat.weapons["weapon_ak47"].1["801"], "Asiimov");
        assert!(cat.knives.contains_key("weapon_knife_butterfly"));

        let db = std::env::temp_dir().join(format!("csls-skins-{}.sq3", std::process::id()));
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE skins_prefs (steamid TEXT, clave TEXT, valor TEXT); INSERT INTO skins_prefs VALUES ('1','weapon_ak47','801'),('1','cuchillo','weapon_knife_butterfly');").unwrap();
        let e = player_skins(&db, &cat, "1").unwrap();
        assert!(e.contains(&Entry { weapon: "AK-47".into(), value: "Asiimov".into() }));
        assert_eq!(players(&db).unwrap(), vec!["1".to_string()]);
        std::fs::remove_file(db).ok();
    }
}
