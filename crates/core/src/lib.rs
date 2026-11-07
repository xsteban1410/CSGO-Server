//! Core of csgo-legacy-server: everything that is not UI (used by the CLI and the desktop app).
pub mod agent;
pub mod aliases;
pub mod backup;
pub mod bridge;
pub mod config;
pub mod console_store;
pub mod deploy;
pub mod doctor;
pub mod install;
pub mod kv;
pub mod launch;
pub mod modes;
pub mod net;
pub mod paths;
pub mod players;
pub mod presets;
pub mod rcon;
pub mod rules;
pub mod security;
pub mod server;
pub mod skins;
pub mod skins_gen;
pub mod steam_client;
pub mod workshop;

pub use anyhow::{Error, Result};

/// UI language, from Settings ("en" unless chosen). The system locale does not count.
/// Read once: changing it applies on the next start.
pub fn lang() -> &'static str {
    static LANG: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    LANG.get_or_init(|| config::saved_language().as_str())
}
