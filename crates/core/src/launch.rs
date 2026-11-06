//! Opciones de una partida (lo que antes leía parse_args de lib.sh) y elección de mapa (pick_map).
use crate::config::{Config, Kind};
use crate::modes::{Mode, Modes};
use anyhow::bail;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    A,
    B,
    Random,
}

impl Site {
    pub fn parse(s: &str) -> Option<Site> {
        match s.to_ascii_lowercase().as_str() {
            "a" => Some(Site::A),
            "b" => Some(Site::B),
            "random" => Some(Site::Random),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Site::A => "a",
            Site::B => "b",
            Site::Random => "random",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub mode: String,
    /// Mapa pedido; None = el actual o el del modo/config.
    pub map: Option<String>,
    pub workshop: Option<u64>,
    /// Humanos para acabar el calentamiento.
    pub humans: u32,
    pub bots: Option<u32>,
    pub difficulty: u8,
    pub rounds: Option<u32>,
    pub kind: Kind,
    pub site: Option<Site>,
    /// Sin dispersión ni retroceso para todos.
    pub precision: bool,
    /// Sin expulsiones ni baneos automáticos.
    pub anarchy: bool,
    pub cvars: Vec<(String, String)>,
}

impl LaunchOptions {
    pub fn from_config(c: &Config) -> LaunchOptions {
        let d = &c.defaults;
        LaunchOptions {
            mode: d.mode.clone(),
            map: None,
            workshop: None,
            humans: d.humans,
            bots: None,
            difficulty: d.difficulty,
            rounds: None,
            kind: d.kind,
            site: None,
            precision: false,
            anarchy: false,
            cvars: Vec::new(),
        }
    }

    pub fn validate(&self, modes: &Modes, cfg: &Config) -> crate::Result<()> {
        let mode = modes.get(&self.mode)?;
        if !mode.allows_kind(self.kind) {
            bail!("el modo {} no admite el tipo de partida {}", mode.id, self.kind.label());
        }
        if self.humans == 0 {
            bail!("hace falta al menos 1 humano");
        }
        if self.difficulty > 3 {
            bail!("la dificultad va de 0 a 3");
        }
        if let Some(m) = &self.map {
            if !valid_map_name(m) {
                bail!("nombre de mapa inválido '{m}'");
            }
        }
        if self.workshop.is_some() && cfg.server.workshop_authkey.is_empty() {
            bail!("los mapas del workshop necesitan workshop_authkey en la config (clave en https://steamcommunity.com/dev/apikey)");
        }
        for (k, v) in &self.cvars {
            check_cvar(k, v)?;
        }
        Ok(())
    }

    /// Comandos que se ejecutan después del cfg del modo (convar modo_extra).
    pub fn extra(&self) -> String {
        let mut e = format!("bot_difficulty {}", self.difficulty);
        if self.precision {
            e += "; weapon_accuracy_nospread 1; weapon_recoil_scale 0";
        }
        if self.anarchy {
            e += "; modo_anarquia 1";
        }
        if let Some(b) = self.bots {
            e += &format!("; bot_quota {b}");
        }
        if let Some(r) = self.rounds {
            e += &format!("; mp_maxrounds {r}");
        }
        if let Some(s) = self.site {
            e += &format!("; modo_sitio {}", s.as_str());
        }
        for (k, v) in &self.cvars {
            e += &format!("; {k} {v}");
        }
        e
    }

    /// Valor de modo_tipo. Los modos que fijan su game_type no usan el tipo elegido.
    pub fn tipo_cfg(&self, mode: &Mode) -> &'static str {
        if mode.meta.game_type.is_some() { "" } else { self.kind.tipo_cfg() }
    }

    pub fn game_type_mode(&self, mode: &Mode) -> (u8, u8) {
        match (mode.meta.game_type, mode.meta.game_mode) {
            (Some(t), Some(g)) => (t, g),
            _ => self.kind.game_type_mode(),
        }
    }

    /// Mapa final. `current` = mapa cargado ahora (None al arrancar).
    pub fn pick_map(&self, modes: &Modes, cfg: &Config, current: Option<&str>, maps_dir: &Path) -> crate::Result<String> {
        if let Some(id) = self.workshop {
            return Ok(format!("workshop/{id}"));
        }
        let mode = modes.get(&self.mode)?;
        let fallback = &cfg.defaults.map;
        let kind_maps = if mode.meta.game_type.is_none() { self.kind.own_maps() } else { None };
        let m = if let Some(m) = &self.map {
            m.clone()
        } else if let Some((prefix, default)) = kind_maps {
            match current {
                Some(c) if c.starts_with(prefix) => c.to_string(),
                _ => default.to_string(),
            }
        } else if !mode.meta.map_prefixes.is_empty() {
            match current {
                Some(c) if mode.accepts_map(c) => c.to_string(),
                _ => mode.meta.default_map.clone().unwrap_or_else(|| fallback.clone()),
            }
        } else {
            let excl = modes.exclusive_prefixes();
            match current {
                Some(c) if !c.starts_with("workshop/") && !excl.iter().any(|p| c.starts_with(p.as_str())) => c.to_string(),
                _ => fallback.clone(),
            }
        };
        if !mode.accepts_map(&m) {
            bail!("el modo {} necesita un mapa {}", mode.id, mode.meta.map_prefixes.join(" o "));
        }
        if let Some((prefix, _)) = kind_maps {
            if !m.starts_with(prefix) {
                bail!("el tipo de partida {} necesita un mapa {prefix}", self.kind.label());
            }
        }
        if !maps_dir.join(format!("{m}.bsp")).is_file() {
            bail!("no existe el mapa '{m}' en {}", maps_dir.display());
        }
        Ok(m)
    }
}

pub fn valid_map_name(m: &str) -> bool {
    !m.is_empty() && !m.contains("..") && m.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-/.".contains(&b))
}

/// cvar=valor sin nada que permita encadenar comandos.
pub fn check_cvar(k: &str, v: &str) -> crate::Result<()> {
    if k.is_empty() || !k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        bail!("cvar inválida '{k}'");
    }
    if v.is_empty() || v.contains([';', '"', '\n', '\r']) {
        bail!("valor inválido para {k}: sin ; ni comillas");
    }
    Ok(())
}

/// "cvar=valor" -> (cvar, valor)
pub fn parse_cvar(s: &str) -> crate::Result<(String, String)> {
    let Some((k, v)) = s.split_once('=') else { bail!("--set espera cvar=valor") };
    check_cvar(k, v)?;
    Ok((k.into(), v.into()))
}

/// Lista de mapas instalados (sin .bsp), oficiales y del workshop.
pub fn installed_maps(maps_dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(maps_dir).into_iter().flatten().flatten() {
        if let Some(n) = e.file_name().to_str().and_then(|n| n.strip_suffix(".bsp")) {
            v.push(n.to_string());
        }
    }
    // del workshop: maps/<sub>/<id>/<nombre>.bsp
    for sub in [crate::workshop::MAPS_SUBDIR, "workshop"] {
        for e in std::fs::read_dir(maps_dir.join(sub)).into_iter().flatten().flatten() {
            for f in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                if let Some(n) = f.file_name().to_str().and_then(|n| n.strip_suffix(".bsp")) {
                    v.push(format!("{sub}/{}/{n}", e.file_name().to_string_lossy()));
                }
            }
        }
    }
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Modes, Config, std::path::PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let m = Modes::load_from(&root.join("modes"), &root.join("cfg"), Path::new("/nonexistent")).unwrap();
        let maps = std::env::temp_dir().join(format!("csls-maps-{}", std::process::id()));
        std::fs::create_dir_all(&maps).unwrap();
        for n in ["de_dust2", "de_nuke", "ar_shoots", "gd_cbble", "cs_office"] {
            std::fs::write(maps.join(format!("{n}.bsp")), "").unwrap();
        }
        (m, Config::default(), maps)
    }

    #[test]
    fn eleccion_de_mapa() {
        let (m, c, maps) = setup();
        let mut o = LaunchOptions::from_config(&c);
        assert_eq!(o.pick_map(&m, &c, None, &maps).unwrap(), "de_dust2");
        assert_eq!(o.pick_map(&m, &c, Some("de_nuke"), &maps).unwrap(), "de_nuke");
        // de un mapa de carrera de armas a un modo normal: vuelve al de la config
        assert_eq!(o.pick_map(&m, &c, Some("ar_shoots"), &maps).unwrap(), "de_dust2");
        o.mode = "guardian".into();
        assert_eq!(o.pick_map(&m, &c, Some("de_nuke"), &maps).unwrap(), "gd_cbble");
        o.mode = "retake".into();
        assert_eq!(o.pick_map(&m, &c, Some("cs_office"), &maps).unwrap(), "de_dust2");
        o.map = Some("cs_office".into());
        assert!(o.pick_map(&m, &c, None, &maps).is_err());
        o.map = Some("de_mirage".into());
        assert!(o.pick_map(&m, &c, None, &maps).is_err(), "no instalado");
        std::fs::remove_dir_all(maps).ok();
    }

    #[test]
    fn tipos_de_partida() {
        use crate::config::Kind;
        let (m, c, maps) = setup();
        let mut o = LaunchOptions::from_config(&c);
        o.mode = "normal".into();
        o.kind = Kind::ArmsRace;
        assert_eq!(o.game_type_mode(m.get("normal").unwrap()), (1, 0));
        assert_eq!(o.pick_map(&m, &c, Some("de_nuke"), &maps).unwrap(), "ar_shoots");
        o.map = Some("de_nuke".into());
        assert!(o.pick_map(&m, &c, None, &maps).is_err());
        o.map = None;
        o.kind = Kind::FreeForAll;
        assert_eq!(o.tipo_cfg(m.get("normal").unwrap()), "ffa");
        assert_eq!(o.tipo_cfg(m.get("dm").unwrap()), "", "dm fija su game_type");
        // de un mapa de Danger Zone a un tipo normal: vuelve al de la config
        o.kind = Kind::Competitive;
        std::fs::write(maps.join("dz_blacksite.bsp"), "").unwrap();
        assert_eq!(o.pick_map(&m, &c, Some("dz_blacksite"), &maps).unwrap(), "de_dust2");
        // armas fijas o reglas propias: no todos los tipos encajan
        assert!(m.get("awp").unwrap().allows_kind(Kind::Deathmatch));
        assert!(!m.get("awp").unwrap().allows_kind(Kind::ArmsRace));
        assert!(!m.get("zombies").unwrap().allows_kind(Kind::Deathmatch));
        assert!(!m.get("retake").unwrap().allows_kind(Kind::Deathmatch));
        assert!(m.get("retake").unwrap().allows_kind(Kind::Wingman));
        assert!(m.get("dm").unwrap().allows_kind(Kind::ArmsRace), "dm ignora el tipo");
        for k in Kind::ALL {
            assert_eq!(Kind::parse(&format!("{k:?}").to_lowercase()), Some(k));
            let cfg = k.tipo_cfg();
            assert!(cfg.is_empty() || Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../cfg/modo_tipo_{cfg}.cfg")).is_file(), "{cfg}");
        }
        std::fs::remove_dir_all(maps).ok();
    }

    #[test]
    fn extra_y_tipo() {
        let (m, c, _) = setup();
        let mut o = LaunchOptions::from_config(&c);
        o.precision = true;
        o.bots = Some(5);
        o.site = Some(Site::A);
        o.cvars.push(parse_cvar("sv_gravity=300").unwrap());
        assert_eq!(o.extra(), "bot_difficulty 3; weapon_accuracy_nospread 1; weapon_recoil_scale 0; bot_quota 5; modo_sitio a; sv_gravity 300");
        assert_eq!(o.game_type_mode(m.get("awp").unwrap()), (0, 1));
        assert_eq!(o.game_type_mode(m.get("dm").unwrap()), (1, 2));
        assert!(parse_cvar("sv_gravity=1;quit").is_err());
        assert!(parse_cvar("SV=1").is_err());
    }
}
