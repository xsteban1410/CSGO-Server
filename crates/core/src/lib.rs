//! Núcleo de csgo-legacy-server: todo lo que no es interfaz (lo usan el CLI y la app GTK).
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

/// Idioma de la interfaz: "es" si LANG/LC_ALL empieza por es, si no "en".
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
