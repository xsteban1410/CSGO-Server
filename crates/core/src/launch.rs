//! Match options (what lib.sh's parse_args used to read) and map choice (pick_map).
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

/// Where humans start in the competitive modes. They can switch team afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Teams {
    Together,
    Versus,
    Free,
}

impl Teams {
    pub fn parse(s: &str) -> Option<Teams> {
        match s.to_ascii_lowercase().as_str() {
            "together" | "juntos" => Some(Teams::Together),
            "versus" | "contra" => Some(Teams::Versus),
            "free" | "libre" => Some(Teams::Free),
            _ => None,
        }
    }
    /// Value of modo_equipos.
    fn as_str(self) -> &'static str {
        match self {
            Teams::Together => "juntos",
            Teams::Versus => "contra",
            Teams::Free => "libre",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub mode: String,
    /// Requested map; None = the current one or the one from the mode/config.
    pub map: Option<String>,
    pub workshop: Option<u64>,
    /// Humans needed to end warmup; 0 = Valve's timed warmup.
    pub humans: u32,
    pub bots: Option<u32>,
    pub difficulty: u8,
    pub rounds: Option<u32>,
    pub kind: Kind,
    pub site: Option<Site>,
    /// Modes with the "teams" option; None = what the mode says.
    pub teams: Option<Teams>,
    /// No spread or recoil for anyone.
    pub precision: bool,
    /// No automatic kicks or bans.
    pub anarchy: bool,
    /// Overtime until someone wins, in the modes with the "nodraw" option.
    #[serde(default)]
    pub no_draw: bool,
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
            teams: None,
            precision: false,
            anarchy: false,
            no_draw: false,
            cvars: Vec::new(),
        }
    }

    pub fn validate(&self, modes: &Modes, cfg: &Config) -> crate::Result<()> {
        let mode = modes.get(&self.mode)?;
        if !mode.allows_kind(self.kind) {
            bail!("mode {} does not support match type {}", mode.id, self.kind.label());
        }
        if self.teams.is_some() && !mode.has_option("teams") {
            bail!("mode {} has no team placement: players pick their team, or its rules fix the sides", mode.id);
        }
        if self.no_draw && !mode.has_option("nodraw") {
            bail!("mode {} has no no-draw rule", mode.id);
        }
        if self.difficulty > 3 {
            bail!("difficulty must be 0 to 3");
        }
        if let Some(m) = &self.map {
            if !valid_map_name(m) {
                bail!("invalid map name '{m}'");
            }
        }
        if self.workshop.is_some() && cfg.server.workshop_authkey.is_empty() {
            bail!("workshop maps need workshop_authkey in the config (key at https://steamcommunity.com/dev/apikey)");
        }
        for (k, v) in &self.cvars {
            check_cvar(k, v)?;
        }
        Ok(())
    }

    /// Commands run after the mode cfg (modo_extra convar).
    pub fn extra(&self, mode: &Mode) -> String {
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
        if let Some(t) = self.teams {
            e += &format!("; modo_equipos {}", t.as_str());
        }
        if self.no_draw {
            e += "; mp_overtime_enable 1";
        }
        // choosing a map is already the decision the map bans would make
        if mode.has_option("veto") && (self.map.is_some() || self.workshop.is_some()) {
            e += "; modo_veto 0";
        }
        for (k, v) in &self.cvars {
            e += &format!("; {k} {v}");
        }
        e
    }

    /// Value of modo_tipo. Modes that set their own game_type ignore the chosen type.
    pub fn tipo_cfg(&self, mode: &Mode) -> &'static str {
        if mode.meta.game_type.is_some() {
            ""
        } else {
            self.kind.tipo_cfg()
        }
    }

    pub fn game_type_mode(&self, mode: &Mode) -> (u8, u8) {
        match (mode.meta.game_type, mode.meta.game_mode) {
            (Some(t), Some(g)) => (t, g),
            _ => self.kind.game_type_mode(),
        }
    }

    /// Final map. `current` = map loaded now (None at startup).
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
            bail!("mode {} needs a {} map", mode.id, mode.meta.map_prefixes.join(" or "));
        }
        if let Some((prefix, _)) = kind_maps {
            if !m.starts_with(prefix) {
                bail!("match type {} needs a {prefix} map", self.kind.label());
            }
        }
        if !maps_dir.join(format!("{m}.bsp")).is_file() {
            bail!("map '{m}' does not exist in {}", maps_dir.display());
        }
        Ok(m)
    }
}

pub fn valid_map_name(m: &str) -> bool {
    !m.is_empty() && !m.contains("..") && m.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-/.".contains(&b))
}

/// cvar=value with nothing that allows chaining commands.
pub fn check_cvar(k: &str, v: &str) -> crate::Result<()> {
    if k.is_empty() || !k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        bail!("invalid cvar '{k}'");
    }
    if v.is_empty() || v.contains([';', '"', '\n', '\r']) {
        bail!("invalid value for {k}: no ; or quotes");
    }
    Ok(())
}

/// "cvar=valor" -> (cvar, valor)
pub fn parse_cvar(s: &str) -> crate::Result<(String, String)> {
    let Some((k, v)) = s.split_once('=') else { bail!("--set espera cvar=valor") };
    check_cvar(k, v)?;
    Ok((k.into(), v.into()))
}

/// List of installed maps (without .bsp), official and from the workshop.
pub fn installed_maps(maps_dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(maps_dir).into_iter().flatten().flatten() {
        if let Some(n) = e.file_name().to_str().and_then(|n| n.strip_suffix(".bsp")) {
            v.push(n.to_string());
        }
    }
    // workshop: maps/<sub>/<id>/<name>.bsp
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game");
        let m = Modes::load_from(&root.join("modes"), &root.join("cfg"), Path::new("/nonexistent")).unwrap();
        // tests run at once and each removes its folder at the end
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let maps = std::env::temp_dir().join(format!("csls-maps-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&maps).unwrap();
        for n in ["de_dust2", "de_nuke", "ar_shoots", "gd_cbble", "cs_office"] {
            std::fs::write(maps.join(format!("{n}.bsp")), "").unwrap();
        }
        (m, Config::default(), maps)
    }

    #[test]
    fn map_choice() {
        let (m, c, maps) = setup();
        let mut o = LaunchOptions::from_config(&c);
        assert_eq!(o.pick_map(&m, &c, None, &maps).unwrap(), "de_dust2");
        assert_eq!(o.pick_map(&m, &c, Some("de_nuke"), &maps).unwrap(), "de_nuke");
        // from an Arms Race map to a normal mode: back to the config's
        assert_eq!(o.pick_map(&m, &c, Some("ar_shoots"), &maps).unwrap(), "de_dust2");
        o.mode = "guardian".into();
        assert_eq!(o.pick_map(&m, &c, Some("de_nuke"), &maps).unwrap(), "gd_cbble");
        o.mode = "retake".into();
        assert_eq!(o.pick_map(&m, &c, Some("cs_office"), &maps).unwrap(), "de_dust2");
        o.map = Some("cs_office".into());
        assert!(o.pick_map(&m, &c, None, &maps).is_err());
        o.map = Some("de_mirage".into());
        assert!(o.pick_map(&m, &c, None, &maps).is_err(), "not installed");
        std::fs::remove_dir_all(maps).ok();
    }

    #[test]
    fn match_types() {
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
        // from a Danger Zone map to a normal type: back to the config's
        o.kind = Kind::Competitive;
        std::fs::write(maps.join("dz_blacksite.bsp"), "").unwrap();
        assert_eq!(o.pick_map(&m, &c, Some("dz_blacksite"), &maps).unwrap(), "de_dust2");
        // fixed weapons or their own rules: not every type fits
        assert!(m.get("awp").unwrap().allows_kind(Kind::Deathmatch));
        assert!(!m.get("awp").unwrap().allows_kind(Kind::ArmsRace));
        assert!(!m.get("zombies").unwrap().allows_kind(Kind::Deathmatch));
        assert!(!m.get("retake").unwrap().allows_kind(Kind::Deathmatch));
        assert!(m.get("retake").unwrap().allows_kind(Kind::Wingman));
        assert!(m.get("dm").unwrap().allows_kind(Kind::ArmsRace), "dm ignores the type");
        for k in Kind::ALL {
            assert_eq!(Kind::parse(&format!("{k:?}").to_lowercase()), Some(k));
            let cfg = k.tipo_cfg();
            assert!(
                cfg.is_empty() || Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../game/cfg/modo_tipo_{cfg}.cfg")).is_file(),
                "{cfg}"
            );
        }
        std::fs::remove_dir_all(maps).ok();
    }

    #[test]
    fn extra_and_type() {
        let (m, c, _) = setup();
        let mut o = LaunchOptions::from_config(&c);
        o.precision = true;
        o.bots = Some(5);
        o.site = Some(Site::A);
        o.cvars.push(parse_cvar("sv_gravity=300").unwrap());
        assert_eq!(
            o.extra(m.get("awp").unwrap()),
            "bot_difficulty 3; weapon_accuracy_nospread 1; weapon_recoil_scale 0; bot_quota 5; modo_sitio a; sv_gravity 300"
        );
        assert_eq!(o.game_type_mode(m.get("awp").unwrap()), (0, 1));
        assert_eq!(o.game_type_mode(m.get("dm").unwrap()), (1, 2));
        assert!(parse_cvar("sv_gravity=1;quit").is_err());
        assert!(parse_cvar("SV=1").is_err());
    }

    #[test]
    fn competitive_options() {
        let (m, c, maps) = setup();
        let mut o = LaunchOptions::from_config(&c);
        o.mode = "premier".into();
        let premier = m.get("premier").unwrap();
        assert_eq!(o.game_type_mode(premier), (0, 1));
        assert_eq!(o.tipo_cfg(premier), "", "the match type does not touch it");
        assert_eq!(o.extra(premier), "bot_difficulty 3");
        o.teams = Some(Teams::Versus);
        o.map = Some("de_dust2".into());
        assert_eq!(o.extra(premier), "bot_difficulty 3; modo_equipos contra; modo_veto 0", "a chosen map skips the bans");
        assert!(o.validate(&m, &c).is_ok());
        o.mode = "awp".into();
        assert!(o.validate(&m, &c).is_err(), "awp has no team placement");
        o.teams = None;
        o.map = None;
        o.no_draw = true;
        assert!(o.validate(&m, &c).is_err(), "awp has no no-draw rule");
        o.mode = "competitivo".into();
        assert!(o.validate(&m, &c).is_ok());
        assert_eq!(o.extra(m.get("competitivo").unwrap()), "bot_difficulty 3; mp_overtime_enable 1");
        o.humans = 0;
        assert!(o.validate(&m, &c).is_ok(), "no humans are needed: warmup runs out by itself");
        assert_eq!(Teams::parse("JUNTOS"), Some(Teams::Together));
        assert_eq!(Teams::parse("x"), None);
        // presets saved before the option existed still load
        let old =
            "[p]\nmode = \"awp\"\nhumans = 2\ndifficulty = 3\nkind = \"competitive\"\nprecision = false\nanarchy = false\ncvars = []\n";
        let p: crate::presets::Presets = toml::from_str(old).unwrap();
        assert_eq!(p.0["p"].teams, None);
        std::fs::remove_dir_all(maps).ok();
    }
}
