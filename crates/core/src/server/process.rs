//! Background processes: systemd --user units, or without systemd a process group with a PID file.
//! A PID alone proves nothing: Linux reuses them, so the file also holds the start time of the process.
use crate::paths;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn systemd_available() -> bool {
    if std::env::var_os("CSLS_NO_SYSTEMD").is_some() {
        return false;
    }
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn unit_active(unit: &str) -> bool {
    Command::new("systemctl").args(["--user", "is-active", "--quiet", unit]).status().is_ok_and(|s| s.success())
}

fn pid_file(name: &str) -> PathBuf {
    paths::runtime_dir().join(format!("{name}.pid"))
}

/// (state, start time in ticks) from the text of /proc/PID/stat.
fn parse_proc_identity(stat: &str) -> Option<(char, u64)> {
    // the name in parentheses may hold spaces; after it: state is field 0, starttime 19
    let f: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((f.first()?.chars().next()?, f.get(19)?.parse().ok()?))
}

fn proc_identity(pid: u32) -> Option<(char, u64)> {
    parse_proc_identity(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// "PID STARTTIME". The process is ours only if both match and it has not already died.
fn check_pid_file(text: &str, identity: impl Fn(u32) -> Option<(char, u64)>) -> Option<u32> {
    let mut f = text.split_whitespace();
    let pid: u32 = f.next()?.parse().ok()?;
    let start: u64 = f.next()?.parse().ok()?;
    let (state, now) = identity(pid)?;
    (now == start && !matches!(state, 'Z' | 'X')).then_some(pid)
}

fn pid_alive(name: &str) -> Option<u32> {
    check_pid_file(&std::fs::read_to_string(pid_file(name)).ok()?, proc_identity)
}

/// The AppImage that holds `exe`. `$APPIMAGE` alone proves nothing: a terminal opened from another AppImage passes it on.
fn appimage_of(appimage: Option<PathBuf>, appdir: Option<PathBuf>, exe: Option<PathBuf>) -> Option<PathBuf> {
    let (image, dir, exe) = (appimage?, appdir?, exe?);
    (exe.starts_with(dir) && image.is_file()).then_some(image)
}

/// The AppImage this process runs from, if any.
pub fn appimage() -> Option<PathBuf> {
    appimage_of(
        std::env::var_os("APPIMAGE").map(PathBuf::from),
        std::env::var_os("APPDIR").map(PathBuf::from),
        std::env::current_exe().ok(),
    )
}

/// CLI executable (to launch the bridge as a separate process). In an AppImage it is the file itself, whose AppRun
/// starts the CLI when given arguments: the mount folder goes away with the app.
pub fn cli_exe() -> PathBuf {
    if let Some(image) = appimage() {
        return image;
    }
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("csgo-legacy-server")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("csgo-legacy-server"))
}

/// Runs `cmd` in the background named `unit` (systemd or setsid + PID).
pub fn spawn_bg(unit: &str, cwd: &Path, env: &[(&str, String)], cmd: &[String], log: &Path) -> crate::Result<()> {
    std::fs::create_dir_all(cwd)?;
    if systemd_available() {
        // a failed unit with the same name would make systemd-run refuse; it stays until here so its exit can be read
        let _ = Command::new("systemctl").args(["--user", "reset-failed", unit]).stderr(Stdio::null()).status();
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--quiet", &format!("--unit={unit}")]).arg(format!("--working-directory={}", cwd.display()));
        for (k, v) in env {
            c.arg(format!("--setenv={k}={v}"));
        }
        let st = c.arg("--").args(cmd).status().context("running systemd-run")?;
        if !st.success() {
            bail!("systemd-run could not start {unit}");
        }
        return Ok(());
    }
    use std::os::unix::process::CommandExt;
    std::fs::create_dir_all(paths::runtime_dir())?;
    if let Some(d) = log.parent() {
        std::fs::create_dir_all(d)?;
    }
    let out = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(cwd)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .process_group(0)
        .spawn()
        .with_context(|| format!("starting {}", cmd[0]))?;
    let pid = child.id();
    let start = proc_identity(pid).map_or(0, |(_, s)| s);
    std::fs::write(pid_file(unit), format!("{pid} {start}"))?;
    Ok(())
}

pub fn running(unit: &str) -> bool {
    (systemd_available() && unit_active(unit)) || pid_alive(unit).is_some()
}

/// TERM to a process group, then KILL if it is still there after 5 s.
fn stop_group(pid: u32) {
    // whole group: srcds_run + srcds_linux
    let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).stderr(Stdio::null()).status();
    let t = Instant::now();
    while proc_identity(pid).is_some_and(|(s, _)| s != 'Z') && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = Command::new("kill").args(["-KILL", "--", &format!("-{pid}")]).stderr(Stdio::null()).status();
}

pub(super) fn kill_bg(unit: &str) {
    if systemd_available() {
        if unit_active(unit) {
            let _ = Command::new("systemctl").args(["--user", "stop", unit]).status();
        }
        let _ = Command::new("systemctl").args(["--user", "reset-failed", unit]).stderr(Stdio::null()).status();
    }
    if let Some(pid) = pid_alive(unit) {
        stop_group(pid);
    }
    let _ = std::fs::remove_file(pid_file(unit));
    if unit == super::BRIDGE_UNIT {
        let _ = std::fs::remove_file(crate::bridge::stats_path());
    }
}

/// Server processes running from this install, found by their folder. With nothing tracked as running, they are
/// leftovers: a lost PID file, a crash of the launcher or a killed unit.
pub(super) fn orphans(install: &Path) -> Vec<u32> {
    let Ok(install) = install.canonicalize() else { return Vec::new() };
    if super::running(super::UNIT) {
        return Vec::new();
    }
    let mut v = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let comm = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
        if matches!(comm.trim(), "srcds_run" | "srcds_linux")
            && std::fs::read_link(e.path().join("cwd")).is_ok_and(|c| c == install)
            && proc_identity(pid).is_some_and(|(s, _)| s != 'Z')
        {
            v.push(pid);
        }
    }
    v
}

pub(super) fn stop_pids(pids: &[u32]) {
    for pid in pids {
        stop_group(*pid);
        let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).stderr(Stdio::null()).status();
    }
}

// ---------- how the last run ended ----------

#[derive(Debug, Serialize, Deserialize)]
struct LastRun {
    /// Still true after the server ends unless someone asked to stop it.
    running: bool,
    boot_id: String,
    started_at: u64,
    mode: String,
    map: String,
}

fn last_run_path() -> PathBuf {
    paths::data_dir().join("last_run.json")
}

fn boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default().trim().to_string()
}

pub(super) fn record_start(mode: &str, map: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let run = LastRun { running: true, boot_id: boot_id(), started_at: now, mode: mode.into(), map: map.into() };
    if let Ok(t) = serde_json::to_string(&run) {
        let _ = std::fs::create_dir_all(paths::data_dir());
        let _ = std::fs::write(last_run_path(), t);
    }
}

pub(super) fn record_stop() {
    let _ = std::fs::remove_file(last_run_path());
}

/// Why a server that nobody asked to stop is gone.
#[derive(Debug, Clone, PartialEq)]
pub enum LastStop {
    /// The computer restarted while it ran.
    Rebooted { mode: String, map: String },
    /// It ended by itself, with what systemd knows about it.
    Crashed { mode: String, map: String, detail: String },
}

impl LastStop {
    pub fn describe(&self) -> String {
        match self {
            LastStop::Rebooted { mode, map } => format!("The computer restarted while the server ran {mode} on {map}"),
            LastStop::Crashed { mode, map, detail } if detail.is_empty() => {
                format!("The server closed by itself while running {mode} on {map}")
            }
            LastStop::Crashed { mode, map, detail } => format!("The server closed by itself while running {mode} on {map}: {detail}"),
        }
    }
}

fn classify_stop(run: LastRun, boot: &str, detail: String) -> LastStop {
    if run.boot_id != boot {
        LastStop::Rebooted { mode: run.mode, map: run.map }
    } else {
        LastStop::Crashed { mode: run.mode, map: run.map, detail }
    }
}

/// `result=exit-code status=139` from `systemctl show`, when there is something to say.
fn parse_unit_exit(show: &str) -> String {
    let get = |k: &str| show.lines().find_map(|l| l.strip_prefix(k)).unwrap_or("");
    let (result, status) = (get("Result="), get("ExecMainStatus="));
    if result.is_empty() || result == "success" {
        String::new()
    } else {
        format!("{result} (exit status {status})")
    }
}

/// What happened to the last run, if it did not end by request and nothing is running now.
pub fn last_stop() -> Option<LastStop> {
    let run: LastRun = serde_json::from_str(&std::fs::read_to_string(last_run_path()).ok()?).ok()?;
    if !run.running || running(super::UNIT) {
        return None;
    }
    let detail = if systemd_available() {
        let o = Command::new("systemctl").args(["--user", "show", "-p", "Result", "-p", "ExecMainStatus", super::UNIT]).output().ok()?;
        parse_unit_exit(&String::from_utf8_lossy(&o.stdout))
    } else {
        String::new()
    };
    Some(classify_stop(run, &boot_id(), detail))
}

/// Use of the machine by the server process.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProcStats {
    pub cpu_percent: u32,
    pub rss_mb: u64,
    pub uptime_secs: u64,
}

/// (CPU ticks used, start time in ticks) from the text of /proc/PID/stat.
fn parse_proc_stat(stat: &str) -> Option<(u64, u64)> {
    // the name in parentheses may hold spaces; after it: state is field 0, utime 11, stime 12, starttime 19
    let f: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?, f.get(19)?.parse().ok()?))
}

fn server_pid(port: u16) -> Option<u32> {
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
        let pid: u32 = e.file_name().to_str()?.parse().ok()?;
        let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
        let cmd = std::fs::read(e.path().join("cmdline")).ok()?;
        (comm.trim() == "srcds_linux" && String::from_utf8_lossy(&cmd).contains(&format!("-port\0{port}\0"))).then_some(pid)
    })
}

/// Samples the server for a moment. shortcut: assumes 100 ticks per second, as on every usual Linux build.
pub fn process_stats(port: u16) -> Option<ProcStats> {
    const TICKS: u64 = 100;
    const SAMPLE_MS: u64 = 250;
    let pid = server_pid(port)?;
    let stat = || std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|s| parse_proc_stat(&s));
    let (before, start) = stat()?;
    std::thread::sleep(Duration::from_millis(SAMPLE_MS));
    let (after, _) = stat()?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kb: u64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.split_whitespace().next()?.parse().ok()?;
    let boot_secs: f64 = std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()?;
    Some(ProcStats {
        cpu_percent: ((after - before) * 100 * 1000 / (TICKS * SAMPLE_MS)) as u32,
        rss_mb: rss_kb / 1024,
        uptime_secs: (boot_secs as u64).saturating_sub(start / TICKS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_appimage_only_counts_when_this_process_lives_in_it() {
        let dir = std::env::temp_dir().join(format!("csls-appimage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("a.AppImage");
        std::fs::write(&image, "").unwrap();
        let p = |s: &str| Some(PathBuf::from(s));
        assert_eq!(appimage_of(Some(image.clone()), p("/tmp/.mount_a"), p("/tmp/.mount_a/usr/bin/app")), Some(image.clone()));
        // a terminal opened from another AppImage passes the variables on to a system install
        assert_eq!(appimage_of(Some(image.clone()), p("/tmp/.mount_a"), p("/usr/bin/app")), None);
        assert_eq!(appimage_of(Some(image), None, p("/usr/bin/app")), None);
        assert_eq!(appimage_of(Some(dir.join("gone")), p("/tmp/.mount_a"), p("/tmp/.mount_a/usr/bin/app")), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_proc_stat() {
        let stat = "123 (srcds linux) S 1 2 3 4 5 6 7 8 9 10 40 2 0 0 20 0 1 0 5000 999 7";
        assert_eq!(parse_proc_stat(stat), Some((42, 5000)));
        assert_eq!(parse_proc_identity(stat), Some(('S', 5000)));
        assert_eq!(parse_proc_stat("nonsense"), None);
    }

    #[test]
    fn a_reused_pid_is_not_ours() {
        let alive = |_| Some(('S', 5000));
        assert_eq!(check_pid_file("123 5000", alive), Some(123));
        assert_eq!(check_pid_file("123 4000", alive), None, "same PID, another process");
        assert_eq!(check_pid_file("123", alive), None, "no start time: cannot be sure");
        assert_eq!(check_pid_file("garbage", alive), None);
        assert_eq!(check_pid_file("123 5000", |_| None), None, "gone");
        assert_eq!(check_pid_file("123 5000", |_| Some(('Z', 5000))), None, "dead, waiting to be reaped");
    }

    #[test]
    fn tells_a_crash_from_a_reboot() {
        let run = || LastRun { running: true, boot_id: "a".into(), started_at: 0, mode: "awp".into(), map: "de_dust2".into() };
        assert!(matches!(classify_stop(run(), "b", String::new()), LastStop::Rebooted { .. }));
        let s = classify_stop(run(), "a", parse_unit_exit("Result=exit-code\nExecMainStatus=139\n"));
        assert!(s.describe().contains("exit-code (exit status 139)"), "{s:?}");
        assert_eq!(parse_unit_exit("Result=success\nExecMainStatus=0\n"), "");
    }
}
