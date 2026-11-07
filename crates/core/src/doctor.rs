//! Health checks shared by `doctor` and the app, and a report that is safe to paste in an issue.
use crate::config::Config;
use crate::server::{self, Server, State};
use crate::{deploy, install, net, steam_client};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Ok,
    Problem,
    Info,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub status: Status,
    pub name: &'static str,
    pub detail: String,
}

fn check(ok: bool, name: &'static str, detail: impl Into<String>) -> Check {
    Check { status: if ok { Status::Ok } else { Status::Problem }, name, detail: detail.into() }
}

fn info(name: &'static str, detail: impl Into<String>) -> Check {
    Check { status: Status::Info, name, detail: detail.into() }
}

/// Newest SourceMod error log.
fn latest_error_log(cfg: &Config) -> Option<PathBuf> {
    let logs = cfg.install_dir().join("csgo/addons/sourcemod/logs");
    let mut v: Vec<PathBuf> = std::fs::read_dir(logs)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("errors_")))
        .collect();
    v.sort();
    v.pop()
}

/// The errors of the last server run in a log: older runs may be fixed already.
fn last_run_errors(log: &str) -> Vec<&str> {
    let start = log.rfind("SourceMod error session started").unwrap_or(0);
    log[start..]
        .lines()
        .filter(|l| {
            l.contains("Exception reported")
                || l.contains("signature not found")
                || l.contains("Failed to load plugin")
                || l.contains("[SM] Unable to load plugin")
        })
        .collect()
}

/// Newest error of the last session in the log, with its date.
pub fn last_error(cfg: &Config) -> Option<String> {
    let log = std::fs::read_to_string(latest_error_log(cfg)?).ok()?;
    last_run_errors(&log).last().map(|l| l.to_string())
}

/// "yes"/"no" from systemd; None if it cannot tell.
fn linger() -> Option<bool> {
    let user = std::env::var("USER").ok()?;
    let o = std::process::Command::new("loginctl").args(["show-user", &user, "--property=Linger", "--value"]).output().ok()?;
    match String::from_utf8_lossy(&o.stdout).trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

fn config_private(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o077 == 0)
}

/// Every check. It only reads local files and asks local services, so it is quick enough for the app.
pub fn run(cfg: &Config) -> Vec<Check> {
    let srv = Server::new(cfg);
    let running = srv.state() == State::Running;
    let mut v = Vec::new();

    let missing = install::missing_deps();
    v.push(if missing.is_empty() {
        check(true, "Dependencies", "installed")
    } else {
        check(false, "Dependencies", format!("missing: {} ({})", missing.join(", "), install::deps_command()))
    });

    let dir = cfg.install_dir();
    v.push(if srv.is_installed() {
        check(true, "Server install", dir.display().to_string())
    } else if dir.join(install::INCOMPLETE_MARKER).exists() {
        check(false, "Server install", "the last install did not finish: run csgo-legacy-server install")
    } else {
        check(false, "Server install", format!("not installed in {}", dir.display()))
    });
    if !srv.is_installed() {
        let free = install::free_bytes(dir).unwrap_or(0);
        v.push(check(free >= install::NEEDED_BYTES, "Free space", format!("{} GB (needs ~40)", free >> 30)));
    }

    let systemd = server::systemd_available();
    v.push(check(
        systemd,
        "systemd --user",
        if systemd { "the server keeps running after closing the app" } else { "missing: the server stops with the app" },
    ));
    if systemd {
        match linger() {
            Some(false) => {
                v.push(check(false, "After logging out", "the server stops when you log out; to keep it: loginctl enable-linger"))
            }
            Some(true) => v.push(check(true, "After logging out", "the server keeps running")),
            None => {}
        }
    }

    let ip = net::local_ip();
    v.push(check(ip.is_some(), "Local IP", ip.map_or("none".to_string(), |i| i.to_string())));
    let port = cfg.server.port;
    let port_ok = running || net::udp_port_free(port);
    v.push(check(port_ok, "UDP port", format!("{port} {}", if port_ok { "free" } else { "busy" })));
    if running {
        let exposed = net::tcp_exposed(port);
        let ips: Vec<String> = exposed.iter().map(ToString::to_string).collect();
        v.push(if exposed.is_empty() {
            check(true, "Rcon", "only on this PC")
        } else {
            check(false, "Rcon", format!("other machines can reach it on {}", ips.join(", ")))
        });
    }
    if let Some(h) = net::firewall_hint(port) {
        v.push(info("Firewall", format!("active; for internet players: {h}")));
    }

    if srv.is_installed() {
        match deploy::stale_plugins(cfg) {
            Ok(p) if p.is_empty() => v.push(check(true, "Plugins", "up to date")),
            Ok(p) => v.push(check(false, "Plugins", format!("out of date: {} (csgo-legacy-server deploy)", p.join(", ")))),
            Err(e) => v.push(check(false, "Plugins", format!("{e:#}"))),
        }
        if let Some(log) = latest_error_log(cfg).and_then(|p| std::fs::read_to_string(p).ok()) {
            let errors = last_run_errors(&log);
            v.push(match errors.last() {
                // the log has no end of run mark, so these may be old: the date in the line tells
                Some(last) => info("SourceMod errors", format!("{} in its last session, the newest: {last}", errors.len())),
                None => check(true, "SourceMod errors", "none in its last session"),
            });
        }
    }

    let path = crate::config::config_path();
    v.push(check(config_private(&path), "Config", path.display().to_string()));
    match steam_client::find_steam() {
        Some(s) => v.push(check(
            s.game_dir().is_some(),
            "CS:GO Legacy client",
            s.game_dir().map_or("not installed".to_string(), |d| d.display().to_string()),
        )),
        None => v.push(info("CS:GO Legacy client", "Steam not found (only needed to play on this PC)")),
    }
    v
}

/// Hides what should not reach a public issue: secrets, IPs, SteamIDs and the user name in paths.
pub fn redact(text: &str, secrets: &[&str]) -> String {
    let mut s = text.to_string();
    for secret in secrets.iter().filter(|x| !x.is_empty()) {
        s = s.replace(secret, "[secret]");
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(c) = rest.chars().next() {
        if let Some(n) = id_len(rest) {
            out.push_str("[hidden]");
            rest = &rest[n..];
        } else if c.is_ascii_digit() || c.is_ascii_alphabetic() {
            // a whole word at a time: an ID only counts from its start
            let n = rest.find(|x: char| !x.is_ascii_alphanumeric()).unwrap_or(rest.len());
            out.push_str(&rest[..n]);
            rest = &rest[n..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => out.replace(&home, "~"),
        _ => out,
    }
}

/// Length of the IPv4, SteamID or SteamID64 that `s` starts with.
fn id_len(s: &str) -> Option<usize> {
    let digits = |x: &str| x.bytes().take_while(u8::is_ascii_digit).count();
    if let Some(r) = s.strip_prefix("STEAM_") {
        let n = r.find(|c: char| !c.is_ascii_digit() && c != ':').unwrap_or(r.len());
        return (n > 0).then_some(6 + n);
    }
    if s.starts_with("[U:") {
        return s.find(']').map(|n| n + 1);
    }
    let first = digits(s);
    if first == 17 && s.starts_with("7656") {
        return Some(17);
    }
    // four groups of digits
    let (mut at, mut groups) = (0, 0);
    loop {
        let n = digits(&s[at..]);
        if n == 0 || n > 3 {
            return None;
        }
        at += n;
        groups += 1;
        if groups == 4 {
            return Some(at);
        }
        if !s[at..].starts_with('.') {
            return None;
        }
        at += 1;
    }
}

/// Text to paste in an issue. Read it first anyway: it is meant to be safe, not proven safe.
pub fn report(cfg: &Config) -> String {
    let (os, version) = install::os_release();
    let mut t = format!(
        "csgo-legacy-server {}\n{os} {version}\nlanguage: {}\nnetwork: {:?}\n\n",
        env!("CARGO_PKG_VERSION"),
        cfg.language.as_str(),
        cfg.server.network
    );
    for c in run(cfg) {
        let mark = match c.status {
            Status::Ok => "ok",
            Status::Problem => "!!",
            Status::Info => "--",
        };
        t.push_str(&format!("{mark} {}: {}\n", c.name, c.detail));
    }
    if let Some(log) = latest_error_log(cfg).and_then(|p| std::fs::read_to_string(p).ok()) {
        let lines: Vec<&str> = log.lines().collect();
        t.push_str("\nLast SourceMod errors:\n");
        for l in &lines[lines.len().saturating_sub(30)..] {
            t.push_str(l);
            t.push('\n');
        }
    }
    let s = &cfg.server;
    redact(&t, &[&s.rcon_password, &s.password, &s.gslt, &s.workshop_authkey])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_hides_private_data() {
        let t = redact("rcon abc123 at 192.168.1.12:27030 STEAM_1:0:123 [U:1:999] 76561198000000000 from x", &["abc123", ""]);
        assert_eq!(t, "rcon [secret] at [hidden]:27030 [hidden] [hidden] [hidden] from x");
    }

    #[test]
    fn redact_keeps_the_rest() {
        let t = "CS:GO 1.38.8.1 version 1.12.0 port 27030 de_dust2 sm_1.2.3.4 ☃";
        // a four-part version looks like an IP: better hidden than the other way round
        assert_eq!(redact(t, &[]), "CS:GO [hidden] version 1.12.0 port 27030 de_dust2 sm_[hidden] ☃");
    }

    #[test]
    fn only_errors_of_the_last_run_count() {
        let log = "L SourceMod error session started\nL [SM] Exception reported: old\nL SourceMod error session started\nL Info\nL signature not found: x\n";
        assert_eq!(last_run_errors(log), vec!["L signature not found: x"]);
    }
}
