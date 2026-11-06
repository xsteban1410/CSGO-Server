//! Core of csgo-legacy-server: everything that is not UI (used by the CLI and the GTK app).
pub mod bridge;
pub mod config;
pub mod deploy;
pub mod install;
pub mod kv;
pub mod launch;
pub mod modes;
pub mod net;
pub mod paths;
pub mod players;
pub mod presets;
pub mod rcon;
pub mod server;
pub mod skins;
pub mod skins_gen;
pub mod steam_client;
pub mod workshop;

pub use anyhow::{Error, Result};

/// UI language: "es" if LANG/LC_ALL starts with es, otherwise "en".
pub fn lang() -> &'static str {
    for v in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(l) = std::env::var(v) {
            if !l.is_empty() {
                return if l.starts_with("es") { "es" } else { "en" };
            }
        }
    }
    "en"
}
