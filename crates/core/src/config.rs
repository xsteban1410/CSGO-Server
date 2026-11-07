//! User configuration: ~/.config/csgo-legacy-server/config.toml (replaces config.env).
use crate::paths;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Solo red local (sv_lan 1).
    #[default]
    Lan,
    /// Friends over VPN: sv_lan 1 + UDP bridge on PORT+1.
    Vpn,
    /// Over the internet with the port open (sv_lan 0).
    Internet,
    /// Internet + GSLT token: listed in the Steam server browser.
    Community,
}

impl Network {
    pub fn sv_lan(self) -> u8 {
        matches!(self, Network::Lan | Network::Vpn) as u8
    }
}

/// Valve match type for modes that do not set game_type/game_mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Competitive,
    CompetitiveShort,
    Casual,
    Wingman,
    Deathmatch,
    FreeForAll,
    TeamDeathmatch,
    ArmsRace,
    Demolition,
    Guardian,
    DangerZone,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Competitive,
        Kind::CompetitiveShort,
        Kind::Casual,
        Kind::Wingman,
        Kind::Deathmatch,
        Kind::FreeForAll,
        Kind::TeamDeathmatch,
        Kind::ArmsRace,
        Kind::Demolition,
        Kind::Guardian,
        Kind::DangerZone,
    ];

    pub fn game_type_mode(self) -> (u8, u8) {
        match self {
            Kind::Competitive | Kind::CompetitiveShort => (0, 1),
            Kind::Casual => (0, 0),
            Kind::Wingman => (0, 2),
            Kind::Deathmatch | Kind::FreeForAll | Kind::TeamDeathmatch => (1, 2),
            Kind::ArmsRace => (1, 0),
            Kind::Demolition => (1, 1),
            Kind::Guardian => (4, 0),
            Kind::DangerZone => (6, 0),
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "competitive" | "competitivo" => Some(Kind::Competitive),
            "short" | "competitiveshort" | "corto" => Some(Kind::CompetitiveShort),
            "casual" => Some(Kind::Casual),
            "wingman" => Some(Kind::Wingman),
            "deathmatch" | "dm" => Some(Kind::Deathmatch),
            "ffa" | "freeforall" => Some(Kind::FreeForAll),
            "tdm" | "teamdeathmatch" => Some(Kind::TeamDeathmatch),
            "armsrace" | "carrera" => Some(Kind::ArmsRace),
            "demolition" | "demolicion" => Some(Kind::Demolition),
            "guardian" => Some(Kind::Guardian),
            "dangerzone" | "dz" => Some(Kind::DangerZone),
            _ => None,
        }
    }

    /// Name for the UI (translated with t()).
    pub fn label(self) -> &'static str {
        match self {
            Kind::Competitive => "Competitive",
            Kind::CompetitiveShort => "Short competitive",
            Kind::Casual => "Casual",
            Kind::Wingman => "Wingman",
            Kind::Deathmatch => "Deathmatch",
            Kind::FreeForAll => "Free-for-all deathmatch",
            Kind::TeamDeathmatch => "Team deathmatch",
            Kind::ArmsRace => "Arms Race",
            Kind::Demolition => "Demolition",
            Kind::Guardian => "Guardian",
            Kind::DangerZone => "Danger Zone",
        }
    }

    /// cfg/modo_tipo_<x>.cfg, run before the mode cfg.
    pub fn tipo_cfg(self) -> &'static str {
        match self {
            Kind::CompetitiveShort => "corto",
            Kind::Deathmatch => "dm",
            Kind::FreeForAll => "ffa",
            Kind::TeamDeathmatch => "tdm",
            Kind::ArmsRace => "carrera",
            Kind::Demolition => "demolicion",
            Kind::Guardian => "guardian",
            Kind::DangerZone => "dz",
            Kind::Wingman => "wingman",
            Kind::Competitive | Kind::Casual => "",
        }
    }

    /// (prefix, default map) of the types that only work on their own maps.
    pub fn own_maps(self) -> Option<(&'static str, &'static str)> {
        match self {
            Kind::ArmsRace => Some(("ar_", "ar_shoots")),
            Kind::Guardian => Some(("gd_", "gd_cbble")),
            Kind::DangerZone => Some(("dz_", "dz_blacksite")),
            _ => None,
        }
    }

    pub fn is_deathmatch(self) -> bool {
        matches!(self, Kind::Deathmatch | Kind::FreeForAll | Kind::TeamDeathmatch)
    }

    /// The game hands out the weapons, or the bots follow their own script.
    pub fn is_scripted(self) -> bool {
        matches!(self, Kind::ArmsRace | Kind::Demolition | Kind::Guardian | Kind::DangerZone)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    /// Dedicated server folder (app 740). Empty = data_dir/server.
    pub install_dir: PathBuf,
    pub port: u16,
    pub rcon_password: String,
    /// IP rcon listens on. Empty = autodetect (hostname IP, 127.0.0.1, local IP).
    pub rcon_host: String,
    pub tickrate: u32,
    pub max_players: u32,
    pub hostname: String,
    pub password: String,
    pub region: u8,
    pub tags: String,
    pub network: Network,
    /// Game Server Login Token (community profile): https://steamcommunity.com/dev/managegameservers
    pub gslt: String,
    /// Steam Web API key for workshop maps: https://steamcommunity.com/dev/apikey
    pub workshop_authkey: String,
    /// Skins/knives plugin. On public servers it can cost you the GSLT.
    pub skins: bool,
    /// Internet/Community profiles: open the port on the router via UPnP at startup.
    pub upnp: bool,
    /// VPN profile: most friends the bridge serves at once.
    pub bridge_max_clients: u32,
    /// VPN profile: networks allowed through the bridge, as IP/bits. Empty = the VPN range (100.64.0.0/10) and this PC.
    pub bridge_allow: Vec<String>,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            install_dir: PathBuf::new(),
            port: 27030,
            rcon_password: String::new(),
            rcon_host: String::new(),
            tickrate: 128,
            max_players: 20,
            hostname: "CS:GO Legacy".into(),
            password: String::new(),
            region: 255,
            tags: String::new(),
            network: Network::Lan,
            gslt: String::new(),
            workshop_authkey: String::new(),
            skins: true,
            upnp: false,
            bridge_max_clients: 64,
            bridge_allow: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    pub mode: String,
    pub humans: u32,
    pub map: String,
    pub kind: Kind,
    pub difficulty: u8,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults { mode: "awp".into(), humans: 0, map: "de_dust2".into(), kind: Kind::Competitive, difficulty: 3 }
    }
}

/// Language of the app, the CLI and in-game messages. English unless chosen in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Es,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Es => "es",
        }
    }
}

const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Format of this file; see `migrate`.
    #[serde(default)]
    pub version: u32,
    pub language: Language,
    pub server: Server,
    pub defaults: Defaults,
}

pub fn config_path() -> PathBuf {
    paths::config_dir().join("config.toml")
}

/// The saved language, without creating the config if it is missing. A broken config means English.
pub fn saved_language() -> Language {
    std::fs::read_to_string(config_path()).ok().and_then(|t| toml::from_str::<Config>(&t).ok()).map(|c| c.language).unwrap_or_default()
}

fn random_password() -> String {
    use std::io::Read;
    const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut buf = [0u8; 20];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).is_err() {
        // shortcut: without /dev/urandom it uses the time; only happens on very odd systems
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (t >> (i % 16 * 8)) as u8 ^ i as u8;
        }
    }
    buf.iter().map(|b| A[*b as usize % A.len()] as char).collect()
}

impl Config {
    /// Loads the config; creates it if missing (migrating config.env from the old repo if there is one).
    pub fn load() -> crate::Result<Config> {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &Path) -> crate::Result<Config> {
        if !path.exists() {
            let mut c = migrate_legacy().unwrap_or_default();
            c.migrate();
            if c.server.rcon_password.is_empty() || c.server.rcon_password == "cambia_esto" {
                c.server.rcon_password = random_password();
            }
            if c.server.install_dir.as_os_str().is_empty() {
                c.server.install_dir = find_install().unwrap_or_else(|| paths::data_dir().join("server"));
            }
            c.save_to(path)?;
            return Ok(c);
        }
        let txt = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut c: Config = toml::from_str(&txt).with_context(|| format!("invalid config in {}", path.display()))?;
        if c.server.install_dir.as_os_str().is_empty() {
            c.server.install_dir = paths::data_dir().join("server");
        }
        c.migrate();
        c.validate()?;
        // an empty rcon password turns rcon off, and the app cannot do anything without it
        if c.server.rcon_password.is_empty() {
            c.server.rcon_password = random_password();
            c.save_to(path)?;
        }
        Ok(c)
    }

    /// Brings a config written by an older version up to date.
    fn migrate(&mut self) {
        // Version 0 saved humans = 2 as a plain default, which made every warmup wait for two players.
        if self.version < 1 {
            self.defaults.humans = 0;
        }
        self.version = CONFIG_VERSION;
    }

    pub fn save(&self) -> crate::Result<()> {
        self.save_to(&config_path())
    }

    pub fn save_to(&self, path: &Path) -> crate::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        self.validate()?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let txt = toml::to_string_pretty(self)?;
        // holds the rcon password: readable by the user only
        let tmp = path.with_extension("toml.tmp");
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(txt.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn validate(&self) -> crate::Result<()> {
        let s = &self.server;
        for (name, v) in [
            ("hostname", &s.hostname),
            ("password", &s.password),
            ("tags", &s.tags),
            ("rcon_password", &s.rcon_password),
            ("gslt", &s.gslt),
            ("workshop_authkey", &s.workshop_authkey),
        ] {
            if v.contains(['"', ';', '\n', '\r']) {
                bail!("{name} cannot contain quotes, ; or line breaks");
            }
        }
        if s.rcon_password.contains(' ') {
            bail!("rcon_password cannot contain spaces");
        }
        if s.port == 0 || s.port == u16::MAX {
            bail!("invalid port: {}", s.port);
        }
        if !(1..=64).contains(&s.max_players) {
            bail!("max_players must be 1 to 64");
        }
        if self.defaults.difficulty > 3 {
            bail!("difficulty must be 0 to 3");
        }
        if !(1..=1024).contains(&s.bridge_max_clients) {
            bail!("bridge_max_clients must be 1 to 1024");
        }
        for n in &s.bridge_allow {
            crate::bridge::Cidr::parse(n)?;
        }
        Ok(())
    }

    pub fn install_dir(&self) -> &Path {
        &self.server.install_dir
    }
}

/// First folder with a server already installed (avoids downloading 33 GB again).
pub fn find_install() -> Option<PathBuf> {
    let mut cand = vec![paths::data_dir().join("server")];
    cand.extend(paths::legacy_dirs().into_iter().map(|d| d.join("server")));
    cand.into_iter().find(|d| d.join("srcds_run").is_file())
}

/// Reads a config.env (KEY=value format) from the old bash project.
pub fn parse_env(txt: &str) -> Config {
    let mut c = Config::default();
    for line in txt.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches('"').to_string();
        let s = &mut c.server;
        let d = &mut c.defaults;
        match k.trim() {
            "PUERTO" => s.port = v.parse().unwrap_or(s.port),
            "RCON_HOST" => s.rcon_host = v,
            "RCON_PASSWORD" => s.rcon_password = v,
            "TICKRATE" => s.tickrate = v.parse().unwrap_or(s.tickrate),
            "MAX_JUGADORES" => s.max_players = v.parse().unwrap_or(s.max_players),
            "WORKSHOP_AUTHKEY" => s.workshop_authkey = v,
            // in the old project SV_LAN=1 was only used for friends over VPN with puente.py
            "SV_LAN" => s.network = if v == "1" { Network::Vpn } else { Network::Internet },
            "MODO" => d.mode = v,
            "HUMANOS" => d.humans = v.parse().unwrap_or(d.humans),
            "MAPA" => d.map = v,
            "TIPO" => d.kind = Kind::parse(&v).unwrap_or(d.kind),
            "DIFICULTAD" => d.difficulty = v.parse().unwrap_or(d.difficulty),
            _ => {}
        }
    }
    c
}

fn migrate_legacy() -> Option<Config> {
    paths::legacy_dirs().into_iter().find_map(|d| {
        let txt = std::fs::read_to_string(d.join("config.env")).ok()?;
        let mut c = parse_env(&txt);
        if d.join("server/srcds_run").is_file() {
            c.server.install_dir = d.join("server");
        }
        Some(c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_values_are_refused() {
        let check = |f: &dyn Fn(&mut Config)| {
            let mut c = Config::default();
            f(&mut c);
            c.validate()
        };
        assert!(check(&|_| {}).is_ok());
        for bad in ["a\"b", "a;quit", "a\nb", "a\rb"] {
            assert!(check(&|c| c.server.password = bad.into()).is_err(), "{bad:?}");
            assert!(check(&|c| c.server.rcon_password = bad.into()).is_err(), "{bad:?}");
            assert!(check(&|c| c.server.workshop_authkey = bad.into()).is_err(), "{bad:?}");
            assert!(check(&|c| c.server.hostname = bad.into()).is_err(), "{bad:?}");
        }
        assert!(check(&|c| c.server.rcon_password = "with space".into()).is_err());
        assert!(check(&|c| c.server.port = 0).is_err());
        assert!(check(&|c| c.server.port = u16::MAX).is_err());
        assert!(check(&|c| c.server.max_players = 0).is_err());
        assert!(check(&|c| c.server.max_players = 65).is_err());
        assert!(check(&|c| c.defaults.difficulty = 4).is_err());
        assert!(check(&|c| c.server.bridge_max_clients = 0).is_err());
        assert!(check(&|c| c.server.bridge_allow = vec!["nonsense".into()]).is_err());
        assert!(check(&|c| c.server.bridge_allow = vec!["10.0.0.0/8".into()]).is_ok());
    }

    #[test]
    fn an_empty_rcon_password_gets_one() {
        let p = std::env::temp_dir().join(format!("csls-rcon-{}.toml", std::process::id()));
        std::fs::write(&p, "[server]\nrcon_password = \"\"\n").unwrap();
        let made = Config::load_from(&p).unwrap().server.rcon_password;
        assert!(!made.is_empty());
        assert_eq!(Config::load_from(&p).unwrap().server.rcon_password, made, "kept");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn old_configs_stop_waiting_for_two_humans() {
        let p = std::env::temp_dir().join(format!("csls-migrate-{}.toml", std::process::id()));
        std::fs::write(&p, "[defaults]\nhumans = 2\n").unwrap();
        let c = Config::load_from(&p).unwrap();
        assert_eq!((c.defaults.humans, c.version), (0, CONFIG_VERSION));
        // from this version on it is the user's choice
        std::fs::write(&p, format!("version = {CONFIG_VERSION}\n[defaults]\nhumans = 2\n")).unwrap();
        assert_eq!(Config::load_from(&p).unwrap().defaults.humans, 2);
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn migrates_config_env() {
        let c = parse_env("PUERTO=27031\nRCON_PASSWORD=abc # comentario\nSV_LAN=1\nTIPO=casual\nDIFICULTAD=2\n");
        assert_eq!(c.server.port, 27031);
        assert_eq!(c.server.rcon_password, "abc");
        assert_eq!(c.server.network, Network::Vpn);
        assert_eq!(c.defaults.kind, Kind::Casual);
        assert_eq!(c.defaults.difficulty, 2);
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("csls-test-{}", std::process::id()));
        let p = dir.join("config.toml");
        let mut c = Config::default();
        c.server.rcon_password = "x1".into();
        c.server.install_dir = "/srv/x".into();
        c.save_to(&p).unwrap();
        let l = Config::load_from(&p).unwrap();
        assert_eq!(l.server.rcon_password, "x1");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_quotes() {
        let mut c = Config::default();
        c.server.hostname = "a\";quit".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn language_is_english_unless_chosen() {
        assert_eq!(Config::default().language, Language::En);
        assert_eq!(toml::from_str::<Config>("[server]\nport = 1").unwrap().language, Language::En);
        assert_eq!(toml::from_str::<Config>("language = \"es\"").unwrap().language, Language::Es);
        assert!(toml::from_str::<Config>("language = \"fr\"").is_err());
        // a saved config keeps it
        let txt = toml::to_string(&Config { language: Language::Es, ..Default::default() }).unwrap();
        assert_eq!(toml::from_str::<Config>(&txt).unwrap().language, Language::Es);
    }

    #[test]
    fn random_password_is_long() {
        let a = random_password();
        assert_eq!(a.len(), 20);
        assert_ne!(a, random_password());
    }
}
