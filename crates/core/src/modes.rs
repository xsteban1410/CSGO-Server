//! Modos de juego: metadatos en modes/<id>.toml + reglas en cfg/modo_<id>.cfg.
//! Los del usuario viven en ~/.config/csgo-legacy-server/modes/<id>.toml + <id>.cfg y
//! pisan a los oficiales con el mismo id. Un .cfg sin .toml también es un modo válido.
use crate::paths;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Localized {
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub es: String,
}

impl Localized {
    pub fn get(&self, lang: &str) -> &str {
        let (a, b) = if lang == "es" { (&self.es, &self.en) } else { (&self.en, &self.es) };
        if a.is_empty() { b } else { a }
    }
    pub fn same(s: &str) -> Localized {
        Localized { en: s.into(), es: s.into() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ModeMeta {
    /// classic | weapons | deathmatch | fun | objective | pve | custom
    pub category: String,
    pub name: Localized,
    pub description: Localized,
    /// game_type/game_mode de Valve fijos; si faltan, salen del tipo elegido (competitivo/casual/wingman).
    pub game_type: Option<u8>,
    pub game_mode: Option<u8>,
    /// Prefijos de mapa válidos (vacío = cualquiera).
    pub map_prefixes: Vec<String>,
    pub default_map: Option<String>,
    /// Sus mapas (ar_, gd_) no sirven para otros modos.
    pub exclusive_maps: bool,
    /// Opciones extra del asistente: "site".
    pub options: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Mode {
    pub id: String,
    pub meta: ModeMeta,
    pub cfg: PathBuf,
    pub custom: bool,
}

impl Mode {
    pub fn name(&self, lang: &str) -> String {
        let n = self.meta.name.get(lang);
        if n.is_empty() { self.id.clone() } else { n.to_string() }
    }

    pub fn has_option(&self, o: &str) -> bool {
        self.meta.options.iter().any(|x| x == o)
    }

    pub fn accepts_map(&self, map: &str) -> bool {
        self.meta.map_prefixes.is_empty() || self.meta.map_prefixes.iter().any(|p| map.starts_with(p.as_str()))
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 31 && id != "base" && id != "inicio"
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

#[derive(Debug, Clone, Default)]
pub struct Modes(pub BTreeMap<String, Mode>);

impl Modes {
    /// Oficiales (recursos) + del usuario.
    pub fn load() -> crate::Result<Modes> {
        let r = paths::resources_dir()?;
        Self::load_from(&r.join("modes"), &r.join("cfg"), &paths::user_modes_dir())
    }

    pub fn load_from(official_meta: &Path, official_cfg: &Path, user: &Path) -> crate::Result<Modes> {
        let mut m = BTreeMap::new();
        // oficiales: modes/<id>.toml + cfg/modo_<id>.cfg
        for e in read_dir_sorted(official_cfg) {
            let Some(id) = file_id(&e, "modo_", ".cfg") else { continue };
            if !valid_id(&id) { continue }
            let meta = read_meta(&official_meta.join(format!("{id}.toml")))?;
            m.insert(id.clone(), Mode { id, meta, cfg: e, custom: false });
        }
        // del usuario: <id>.toml + <id>.cfg
        for e in read_dir_sorted(user) {
            let Some(id) = file_id(&e, "", ".cfg") else { continue };
            if !valid_id(&id) { continue }
            let mut meta = read_meta(&user.join(format!("{id}.toml")))?;
            if meta.category.is_empty() {
                meta.category = "custom".into();
            }
            m.insert(id.clone(), Mode { id, meta, cfg: e, custom: true });
        }
        Ok(Modes(m))
    }

    pub fn get(&self, id: &str) -> crate::Result<&Mode> {
        self.0.get(id).ok_or_else(|| anyhow::anyhow!("modo desconocido '{id}'. Modos: {}", self.ids().join(" ")))
    }

    pub fn ids(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Prefijos que solo valen para su modo (ar_, gd_).
    pub fn exclusive_prefixes(&self) -> Vec<String> {
        self.0.values().filter(|m| m.meta.exclusive_maps).flat_map(|m| m.meta.map_prefixes.clone()).collect()
    }
}

fn read_dir_sorted(d: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).collect();
    v.sort();
    v
}

fn file_id(p: &Path, prefix: &str, suffix: &str) -> Option<String> {
    let n = p.file_name()?.to_str()?;
    n.strip_prefix(prefix)?.strip_suffix(suffix).map(str::to_string)
}

fn read_meta(p: &Path) -> crate::Result<ModeMeta> {
    match std::fs::read_to_string(p) {
        Ok(t) => toml::from_str(&t).with_context(|| format!("modo inválido en {}", p.display())),
        Err(_) => Ok(ModeMeta::default()),
    }
}

/// Guarda un modo del usuario (TOML + cfg). Devuelve la ruta del cfg.
pub fn save_user_mode(dir: &Path, id: &str, meta: &ModeMeta, cfg: &str) -> crate::Result<PathBuf> {
    if !valid_id(id) {
        bail!("id de modo inválido '{id}': minúsculas, números y _ (máx. 31)");
    }
    CfgDoc::parse(cfg).validate()?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(format!("{id}.toml")), toml::to_string_pretty(meta)?)?;
    let p = dir.join(format!("{id}.cfg"));
    std::fs::write(&p, cfg)?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oficiales_tienen_toml_y_cfg() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let m = Modes::load_from(&root.join("modes"), &root.join("cfg"), Path::new("/nonexistent")).unwrap();
        assert!(m.0.len() >= 30);
        for mode in m.0.values() {
            assert!(!mode.meta.name.en.is_empty(), "{} sin modes/{}.toml", mode.id, mode.id);
            assert!(!mode.meta.category.is_empty(), "{} sin categoría", mode.id);
        }
        // y cada toml tiene su cfg
        for e in read_dir_sorted(&root.join("modes")) {
            let id = file_id(&e, "", ".toml").unwrap();
            assert!(m.0.contains_key(&id), "modes/{id}.toml sin cfg/modo_{id}.cfg");
        }
        assert_eq!(m.get("armsrace").unwrap().meta.game_type, Some(1));
        assert!(m.exclusive_prefixes().contains(&"gd_".to_string()));
    }

    #[test]
    fn usuario_pisa_y_cfg_suelto() {
        let d = std::env::temp_dir().join(format!("csls-modes-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("mio.cfg"), "sv_gravity 200\n").unwrap();
        save_user_mode(&d, "awp", &ModeMeta { name: Localized::same("Mi AWP"), ..Default::default() }, "x").unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let m = Modes::load_from(&root.join("modes"), &root.join("cfg"), &d).unwrap();
        assert!(m.get("mio").unwrap().custom);
        assert_eq!(m.get("mio").unwrap().name("es"), "mio");
        assert_eq!(m.get("awp").unwrap().name("en"), "Mi AWP");
        assert!(save_user_mode(&d, "Mal Id", &ModeMeta::default(), "").is_err());
        std::fs::remove_dir_all(d).ok();
    }
}

// ---------- editor: cfg como lista de "cvar valor" ----------

/// Contenido de un cfg de modo: comentario de cabecera + pares cvar/valor en orden.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CfgDoc {
    pub comment: String,
    pub pairs: Vec<(String, String)>,
}

impl CfgDoc {
    pub fn parse(txt: &str) -> CfgDoc {
        let mut d = CfgDoc::default();
        for l in txt.lines() {
            let l = l.trim();
            if let Some(c) = l.strip_prefix("//") {
                if d.pairs.is_empty() && d.comment.is_empty() {
                    d.comment = c.trim().to_string();
                }
                continue;
            }
            let mut it = l.splitn(2, char::is_whitespace);
            let Some(k) = it.next().filter(|k| !k.is_empty()) else { continue };
            let v = it.next().unwrap_or("").trim().trim_matches('"').to_string();
            d.set(k, &v);
        }
        d
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.pairs.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, k: &str, v: &str) {
        match self.pairs.iter_mut().find(|(kk, _)| kk == k) {
            Some(p) => p.1 = v.to_string(),
            None => self.pairs.push((k.to_string(), v.to_string())),
        }
    }

    pub fn remove(&mut self, k: &str) {
        self.pairs.retain(|(kk, _)| kk != k);
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        if !self.comment.is_empty() {
            s += &format!("// {}\n", self.comment.replace('\n', " "));
        }
        for (k, v) in &self.pairs {
            // los valores con espacios, comas o vacíos van entre comillas
            if v.is_empty() || v.contains([' ', ',']) || k.starts_with("modo_") && v.parse::<f64>().is_err() {
                s += &format!("{k} \"{v}\"\n");
            } else {
                s += &format!("{k} {v}\n");
            }
        }
        s
    }

    /// Valida todos los pares (sin ; ni comillas que permitan encadenar comandos, ni comandos peligrosos:
    /// un modo importado viene de otra persona).
    pub fn validate(&self) -> crate::Result<()> {
        for (k, v) in &self.pairs {
            if BLOCKED.contains(&k.to_ascii_lowercase().as_str()) || k.starts_with("sm_") {
                bail!("comando no permitido en un modo: {k}");
            }
            if k.is_empty() || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                bail!("cvar inválida '{k}'");
            }
            if v.contains([';', '"', '\n', '\r']) {
                bail!("valor inválido para {k}: sin ; ni comillas");
            }
        }
        Ok(())
    }
}

const BLOCKED: &[&str] = &[
    "quit", "exit", "exec", "alias", "rcon_password", "sv_password", "sv_setsteamaccount", "sm", "meta",
    "plugin_load", "plugin_unload", "changelevel", "map", "host_workshop_map", "kick", "kickid", "banid", "banip",
    "addip", "writeid", "writeip", "killserver", "restart", "_restart", "host_writeconfig", "log", "logaddress_add",
    "sv_downloadurl", "con_logfile", "rcon",
];

/// Archivo para compartir un modo: metadatos + id + cfg en un solo TOML.
#[derive(Debug, Serialize, Deserialize)]
struct Exported {
    id: String,
    #[serde(flatten)]
    meta: ModeMeta,
    cfg: String,
}

pub fn export_mode(m: &Mode) -> crate::Result<String> {
    let cfg = std::fs::read_to_string(&m.cfg)?;
    Ok(toml::to_string_pretty(&Exported { id: m.id.clone(), meta: m.meta.clone(), cfg })?)
}

/// Importa un modo exportado al directorio del usuario. Devuelve su id.
pub fn import_mode(dir: &Path, txt: &str) -> crate::Result<String> {
    let e: Exported = toml::from_str(txt).context("no es un modo exportado válido")?;
    CfgDoc::parse(&e.cfg).validate()?;
    let mut meta = e.meta;
    if meta.category.is_empty() {
        meta.category = "custom".into();
    }
    // al importar se guarda el cfg ya limpio (solo pares cvar valor)
    save_user_mode(dir, &e.id, &meta, &CfgDoc::parse(&e.cfg).render())?;
    Ok(e.id)
}

pub fn delete_user_mode(dir: &Path, id: &str) -> crate::Result<()> {
    if !valid_id(id) {
        bail!("id inválido");
    }
    for ext in ["toml", "cfg"] {
        let p = dir.join(format!("{id}.{ext}"));
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod editor_tests {
    use super::*;

    #[test]
    fn cfg_ida_y_vuelta() {
        let t = "// awp: AWP + Deagle\nmodo_t_quitar 1\nmodo_t_arma \"weapon_awp\"\nmodo_t_objeto \"\"\nsv_gravity 300\n";
        let d = CfgDoc::parse(t);
        assert_eq!(d.comment, "awp: AWP + Deagle");
        assert_eq!(d.get("modo_t_arma"), Some("weapon_awp"));
        assert_eq!(d.render(), t);
        let mut bad = d.clone();
        bad.set("sv_gravity", "1;quit");
        assert!(bad.validate().is_err());
        assert!(CfgDoc::parse("rcon_password x\n").validate().is_err());
        assert!(CfgDoc::parse("sm_rcon quit\n").validate().is_err());
        assert!(CfgDoc::parse("sv_gravity 300\nmodo_vampiro 0.5\n").validate().is_ok());
    }

    #[test]
    fn exportar_importar() {
        let d = std::env::temp_dir().join(format!("csls-exp-{}", std::process::id()));
        let cfg = save_user_mode(&d, "mio", &ModeMeta { name: Localized::same("Mío"), ..Default::default() }, "sv_gravity 200\n").unwrap();
        let m = Mode { id: "mio".into(), meta: ModeMeta { name: Localized::same("Mío"), ..Default::default() }, cfg, custom: true };
        let txt = export_mode(&m).unwrap();
        let d2 = d.join("otro");
        assert_eq!(import_mode(&d2, &txt).unwrap(), "mio");
        assert_eq!(std::fs::read_to_string(d2.join("mio.cfg")).unwrap(), "sv_gravity 200\n");
        assert!(import_mode(&d2, "id = \"x\"\ncfg = \"quit;rm\"\n").is_err() || CfgDoc::parse("quit;rm").validate().is_err());
        delete_user_mode(&d2, "mio").unwrap();
        assert!(!d2.join("mio.cfg").exists());
        std::fs::remove_dir_all(d).ok();
    }
}
