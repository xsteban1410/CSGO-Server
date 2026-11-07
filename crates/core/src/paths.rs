//! XDG and resource paths. Nothing depends on where the repo is cloned.
use std::path::{Path, PathBuf};

pub const APP: &str = "csgo-legacy-server";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home().join(fallback),
    }
    .join(APP)
}

/// ~/.config/csgo-legacy-server
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// ~/.local/share/csgo-legacy-server
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// Runtime directory (pid, sockets). Falls back to data_dir without XDG_RUNTIME_DIR.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p).join(APP),
        _ => data_dir().join("run"),
    }
}

/// User's own modes (TOML + cfg).
pub fn user_modes_dir() -> PathBuf {
    config_dir().join("modes")
}

/// User's own plugins (.sp, compiled automatically).
pub fn user_plugins_dir() -> PathBuf {
    config_dir().join("plugins")
}

fn is_resources(p: &Path) -> bool {
    p.join("modes").is_dir() && p.join("cfg").is_dir() && p.join("plugin").is_dir()
}

/// Folder with cfg/, modes/, plugin/ and data/: $CSLS_RESOURCES, the package's
/// (<prefix>/share/csgo-legacy-server) or, in development, the repo's game/.
pub fn resources_dir() -> crate::Result<PathBuf> {
    let mut cand: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("CSLS_RESOURCES") {
        cand.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(prefix) = exe.parent().and_then(Path::parent) {
            cand.push(prefix.join("share").join(APP));
            // built in the repo: <repo>/target/{debug,release}/<exe>
            if let Some(repo) = prefix.parent() {
                cand.push(repo.join("game"));
            }
        }
    }
    cand.push(PathBuf::from("/usr/share").join(APP));
    // in development (cargo run / tests) the repo's game/; not embedded in release binaries
    #[cfg(debug_assertions)]
    cand.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game"));
    cand.into_iter()
        .find(|p| is_resources(p))
        .map(|p| p.canonicalize().unwrap_or(p))
        .ok_or_else(|| anyhow::anyhow!("resources not found (cfg/, modes/, plugin/); set CSLS_RESOURCES"))
}

/// Folders that may hold a previous install (old repo ~/CSGO-Server).
pub fn legacy_dirs() -> Vec<PathBuf> {
    let mut v = vec![home().join("CSGO-Server")];
    if let Ok(r) = resources_dir() {
        v.push(r);
    }
    v.dedup();
    v
}
