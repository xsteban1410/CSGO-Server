//! Security check run when the server starts and by `doctor`. It only advises: what to accept is the user's call.
use crate::config::{config_path, Config};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Below this an rcon password can be guessed, and rcon gives full control of the server.
const MIN_RCON_PASSWORD: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Worth showing at every start; otherwise only `doctor` lists it.
    pub warn: bool,
    pub text: String,
}

fn warn(text: impl Into<String>) -> Finding {
    Finding { warn: true, text: text.into() }
}

fn too_open(path: &Path) -> Option<u32> {
    let mode = std::fs::metadata(path).ok()?.permissions().mode() & 0o777;
    (mode & 0o077 != 0).then_some(mode)
}

fn hides_processes(mountinfo: &str) -> bool {
    mountinfo.lines().any(|l| l.contains(" - proc ") && l.contains("hidepid=") && !l.contains("hidepid=0"))
}

fn check_passwords(rcon: &str, join: &str) -> Vec<Finding> {
    let mut v = Vec::new();
    if !rcon.is_empty() && rcon == join {
        v.push(warn(
            "The rcon password and the server password are the same: anyone who can join could control the server. Use different ones",
        ));
    }
    if rcon.chars().count() < MIN_RCON_PASSWORD {
        v.push(warn(format!("The rcon password is short: use at least {MIN_RCON_PASSWORD} characters")));
    }
    v
}

pub fn preflight(cfg: &Config) -> Vec<Finding> {
    let mut v = check_passwords(&cfg.server.rcon_password, &cfg.server.password);
    let csgo = cfg.install_dir().join("csgo");
    let backups = crate::backup::list().unwrap_or_default();
    let secret_files = [config_path(), csgo.join("cfg/csls_secrets.cfg"), csgo.join("cfg/server.cfg"), csgo.join("webapi_authkey.txt")];
    for p in secret_files.iter().chain(&backups) {
        if let Some(mode) = too_open(p) {
            v.push(warn(format!("Other users of this PC can read {} (mode {mode:o}): chmod 600 it", p.display())));
        }
    }
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    if !mounts.is_empty() && !hides_processes(&mounts) {
        v.push(Finding {
            warn: false,
            text: "Other users of this PC can see which processes run (no hidepid on /proc). No secret is in the server's arguments".into(),
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_different_passwords() {
        let same = check_passwords("a-long-enough-password", "a-long-enough-password");
        assert!(same[0].text.contains("different"), "{same:?}");
        assert!(check_passwords("a-long-enough-password", "other").is_empty());
        assert!(check_passwords("short", "").iter().any(|f| f.text.contains("short")));
    }

    #[test]
    fn notices_open_files() {
        let p = std::env::temp_dir().join(format!("csls-open-{}", std::process::id()));
        std::fs::write(&p, "x").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(too_open(&p), Some(0o644));
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(too_open(&p), None);
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn reads_hidepid() {
        let line = |o: &str| format!("22 1 0:21 / /proc rw,nosuid - proc proc rw{o}");
        assert!(!hides_processes(&line("")));
        assert!(!hides_processes(&line(",hidepid=0")));
        assert!(hides_processes(&line(",hidepid=2")));
    }
}
