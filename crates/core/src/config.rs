//! Configuración del usuario: ~/.config/csgo-legacy-server/config.toml (reemplaza config.env).
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
    /// Amigos por VPN: sv_lan 1 + puente UDP en PUERTO+1.
    Vpn,
    /// Por internet con el puerto abierto (sv_lan 0).
    Internet,
    /// Internet + token GSLT: sale en el navegador de servidores de Steam.
    Community,
}

impl Network {
    pub fn sv_lan(self) -> u8 {
        matches!(self, Network::Lan | Network::Vpn) as u8
    }
}

/// Tipo de partida de Valve para los modos que no fijan game_type/game_mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Competitive,
    Casual,
    Wingman,
}

impl Kind {
    pub fn game_type_mode(self) -> (u8, u8) {
        match self {
            Kind::Competitive => (0, 1),
            Kind::Casual => (0, 0),
            Kind::Wingman => (0, 2),
        }
    }
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "competitive" | "competitivo" => Some(Kind::Competitive),
            "casual" => Some(Kind::Casual),
            "wingman" => Some(Kind::Wingman),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    /// Carpeta del servidor dedicado (app 740). Vacía = data_dir/server.
    pub install_dir: PathBuf,
    pub port: u16,
    pub rcon_password: String,
    /// IP donde escucha rcon. Vacía = autodetectar (IP del hostname, 127.0.0.1, IP local).
    pub rcon_host: String,
    pub tickrate: u32,
    pub max_players: u32,
    pub hostname: String,
    pub password: String,
    pub region: u8,
    pub tags: String,
    pub network: Network,
    /// Game Server Login Token (perfil comunitario): https://steamcommunity.com/dev/managegameservers
    pub gslt: String,
    /// Clave de la Steam Web API para mapas del workshop: https://steamcommunity.com/dev/apikey
    pub workshop_authkey: String,
    /// Plugin de skins/cuchillos. En servidores públicos puede costar el GSLT.
    pub skins: bool,
    /// Perfiles Internet/Comunitario: abrir el puerto en el router por UPnP al arrancar.
    pub upnp: bool,
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
        Defaults { mode: "awp".into(), humans: 2, map: "de_dust2".into(), kind: Kind::Competitive, difficulty: 3 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub server: Server,
    pub defaults: Defaults,
}

pub fn config_path() -> PathBuf {
    paths::config_dir().join("config.toml")
}

fn random_password() -> String {
    use std::io::Read;
    const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut buf = [0u8; 20];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).is_err() {
        // shortcut: sin /dev/urandom se usa la hora; solo pasa en sistemas muy raros
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (t >> (i % 16 * 8)) as u8 ^ i as u8;
        }
    }
    buf.iter().map(|b| A[*b as usize % A.len()] as char).collect()
}

impl Config {
    /// Carga la config; si no existe la crea (migrando config.env del repo antiguo si lo hay).
    pub fn load() -> crate::Result<Config> {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &Path) -> crate::Result<Config> {
        if !path.exists() {
            let mut c = migrate_legacy().unwrap_or_default();
            if c.server.rcon_password.is_empty() || c.server.rcon_password == "cambia_esto" {
                c.server.rcon_password = random_password();
            }
            if c.server.install_dir.as_os_str().is_empty() {
                c.server.install_dir = find_install().unwrap_or_else(|| paths::data_dir().join("server"));
            }
            c.save_to(path)?;
            return Ok(c);
        }
        let txt = std::fs::read_to_string(path).with_context(|| format!("leyendo {}", path.display()))?;
        let mut c: Config = toml::from_str(&txt).with_context(|| format!("config inválida en {}", path.display()))?;
        if c.server.install_dir.as_os_str().is_empty() {
            c.server.install_dir = paths::data_dir().join("server");
        }
        c.validate()?;
        Ok(c)
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
        // lleva la contraseña rcon: solo legible por el usuario
        let tmp = path.with_extension("toml.tmp");
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(txt.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn validate(&self) -> crate::Result<()> {
        let s = &self.server;
        for (name, v) in [("hostname", &s.hostname), ("password", &s.password), ("tags", &s.tags), ("rcon_password", &s.rcon_password)] {
            if v.contains(['"', ';', '\n', '\r']) {
                bail!("{name} no puede llevar comillas, ; ni saltos de línea");
            }
        }
        if s.rcon_password.contains(' ') {
            bail!("rcon_password no puede llevar espacios");
        }
        if s.port == 0 || s.port == u16::MAX {
            bail!("puerto inválido: {}", s.port);
        }
        if !(1..=64).contains(&s.max_players) {
            bail!("max_players va de 1 a 64");
        }
        if self.defaults.difficulty > 3 {
            bail!("la dificultad va de 0 a 3");
        }
        Ok(())
    }

    pub fn install_dir(&self) -> &Path {
        &self.server.install_dir
    }
}

/// Primera carpeta con un servidor ya instalado (evita volver a bajar 33 GB).
pub fn find_install() -> Option<PathBuf> {
    let mut cand = vec![paths::data_dir().join("server")];
    cand.extend(paths::legacy_dirs().into_iter().map(|d| d.join("server")));
    cand.into_iter().find(|d| d.join("srcds_run").is_file())
}

/// Lee un config.env (formato KEY=valor) del proyecto bash antiguo.
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
            // en el proyecto antiguo SV_LAN=1 solo se usaba para amigos por VPN con puente.py
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
    fn migra_config_env() {
        let c = parse_env("PUERTO=27031\nRCON_PASSWORD=abc # comentario\nSV_LAN=1\nTIPO=casual\nDIFICULTAD=2\n");
        assert_eq!(c.server.port, 27031);
        assert_eq!(c.server.rcon_password, "abc");
        assert_eq!(c.server.network, Network::Vpn);
        assert_eq!(c.defaults.kind, Kind::Casual);
        assert_eq!(c.defaults.difficulty, 2);
    }

    #[test]
    fn guarda_y_carga() {
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
    fn rechaza_comillas() {
        let mut c = Config::default();
        c.server.hostname = "a\";quit".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn contrasena_aleatoria() {
        let a = random_password();
        assert_eq!(a.len(), 20);
        assert_ne!(a, random_password());
    }
}
