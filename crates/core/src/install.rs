//! Install (formerly setup.sh): idempotent steps with progress. Can be repeated and resumed.
use crate::config::Config;
use crate::{deploy, paths};
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Present while an install runs and after one is interrupted or fails: the server may be half written.
pub const INCOMPLETE_MARKER: &str = ".csls-install-incomplete";

/// A SourceMod or Metamod build this version was tested with.
#[derive(Debug, Clone, Deserialize)]
pub struct Pinned {
    pub version: String,
    base: String,
    file: String,
    sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PinnedSet {
    pub metamod: Pinned,
    pub sourcemod: Pinned,
}

/// The builds this version installs. Debug builds can read them elsewhere so tests need no network;
/// a release has only the ones it was built with.
pub fn pinned() -> crate::Result<PinnedSet> {
    #[cfg(debug_assertions)]
    if let Some(p) = std::env::var_os("CSLS_PINNED") {
        return Ok(toml::from_str(&std::fs::read_to_string(p)?)?);
    }
    Ok(toml::from_str(include_str!("../../../game/data/pinned.toml"))?)
}

/// What is installed, in the data folder because `steamcmd validate` must not remove it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub metamod: String,
    pub sourcemod: String,
    /// SteamCMD has no fixed hash to compare with, so this only says which file was installed.
    pub steamcmd_sha256: String,
}

fn manifest_path() -> PathBuf {
    paths::data_dir().join("install.json")
}

pub fn read_manifest() -> Manifest {
    std::fs::read_to_string(manifest_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_manifest(m: &Manifest) -> crate::Result<()> {
    let tmp = manifest_path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(m)?)?;
    std::fs::rename(tmp, manifest_path())?;
    Ok(())
}

/// Space needed for app 740, with margin.
pub const NEEDED_BYTES: u64 = 40 * 1024 * 1024 * 1024;
/// Below this SteamCMD is stopped, and the addons are not installed: a full disk breaks far more than the install.
const MIN_FREE_BYTES: u64 = 1024 * 1024 * 1024;
const ADDONS_BYTES: u64 = 300 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Progress {
    pub step: &'static str,
    /// 0..1 if known.
    pub fraction: Option<f64>,
    pub message: String,
}

// ---------- dependencias ----------

/// What the system lacks to install and run.
pub fn missing_deps() -> Vec<&'static str> {
    let mut v = Vec::new();
    for c in ["curl", "tar"] {
        if which(c).is_none() {
            v.push(c);
        }
    }
    // srcds and steamcmd are 32-bit
    if !["/lib/ld-linux.so.2", "/lib32/ld-linux.so.2", "/usr/lib32/ld-linux.so.2"].iter().any(|p| Path::new(p).exists()) {
        v.push("libc 32 bits");
    }
    v
}

pub fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?.to_str()?.split(':').map(|d| Path::new(d).join(cmd)).find(|p| p.is_file())
}

/// (ID, ID_LIKE) from /etc/os-release.
pub fn os_release() -> (String, String) {
    let t = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let get =
        |k: &str| t.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(|v| v.trim_matches('"').to_string()).unwrap_or_default();
    (get("ID"), get("ID_LIKE"))
}

/// Exact command to install the dependencies on this distro.
pub fn deps_command() -> String {
    let (id, like) = os_release();
    let is = |x: &str| id == x || like.split_whitespace().any(|l| l == x);
    if id == "steamos" {
        "SteamOS is not supported (read-only system): use a distrobox container".into()
    } else if is("arch") {
        let multilib = std::fs::read_to_string("/etc/pacman.conf").is_ok_and(|t| t.lines().any(|l| l.trim() == "[multilib]"));
        let pre = if multilib { "" } else { "activa [multilib] en /etc/pacman.conf y luego: " };
        format!("{pre}sudo pacman -S --needed curl tar lib32-glibc lib32-gcc-libs")
    } else if is("debian") || is("ubuntu") {
        "sudo dpkg --add-architecture i386 && sudo apt update && sudo apt install curl tar lib32gcc-s1 lib32stdc++6 libc6-i386".into()
    } else if is("fedora") || is("rhel") {
        "sudo dnf install curl tar glibc.i686 libstdc++.i686".into()
    } else if is("suse") || is("opensuse") {
        "sudo zypper install curl tar glibc-32bit libstdc++6-32bit".into()
    } else if id == "void" {
        "sudo xbps-install -S void-repo-multilib && sudo xbps-install curl tar glibc-32bit libgcc-32bit libstdc++-32bit".into()
    } else if is("gentoo") {
        "sudo emerge --ask net-misc/curl app-arch/tar (and ABI_X86=\"32 64\" for glibc and gcc)".into()
    } else {
        "install curl, tar and the 32-bit libraries (glibc and libstdc++/libgcc) with your distro package manager".into()
    }
}

/// Free bytes in the folder (or in its first existing parent).
pub fn free_bytes(dir: &Path) -> Option<u64> {
    let mut d = dir;
    while !d.exists() {
        d = d.parent()?;
    }
    let o = Command::new("df").args(["--output=avail", "-B1"]).arg(d).output().ok()?;
    String::from_utf8_lossy(&o.stdout).lines().nth(1)?.trim().parse().ok()
}

// ---------- pasos ----------

fn run_logged(cmd: &mut Command, cancel: &AtomicBool, mut line: impl FnMut(&str)) -> crate::Result<bool> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null()).spawn()?;
    let err = child.stderr.take().unwrap();
    let errt = std::thread::spawn(move || BufReader::new(err).lines().map_while(Result::ok).collect::<Vec<_>>().join("\n"));
    for l in BufReader::new(child.stdout.take().unwrap()).split(b'\n').map_while(Result::ok) {
        // steamcmd sometimes separates with \r
        for part in String::from_utf8_lossy(&l).split('\r') {
            line(part);
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("cancelled");
        }
    }
    let st = child.wait()?;
    let e = errt.join().unwrap_or_default();
    if !st.success() && !e.is_empty() {
        line(&e);
    }
    Ok(st.success())
}

/// " Update state (0x61) downloading, progress: 12.34 (123 / 456)" -> 0.1234
pub fn steamcmd_progress(l: &str) -> Option<(f64, String)> {
    let i = l.find("progress: ")?;
    let pct: f64 = l[i + 10..].split_whitespace().next()?.parse().ok()?;
    let state = l.split(',').next()?.split(')').nth(1).unwrap_or("").trim().to_string();
    Some((pct / 100.0, state))
}

/// " ... progress: 12.34 (123 / 456)" -> (123, 456) bytes
fn steamcmd_bytes(l: &str) -> Option<(u64, u64)> {
    let (done, total) = l.rsplit_once('(')?.1.trim_end_matches(')').split_once(" / ")?;
    Some((done.trim().parse().ok()?, total.trim().parse().ok()?))
}

/// Download speed between two progress lines.
fn speed_text(bytes: u64, secs: f64) -> String {
    format!("{:.1} MB/s", bytes as f64 / secs.max(0.001) / 1_000_000.0)
}

fn steamcmd_path() -> crate::Result<PathBuf> {
    if let Some(p) = which("steamcmd") {
        return Ok(p);
    }
    let dir = paths::data_dir().join("steamcmd");
    Ok(dir.join("steamcmd.sh"))
}

fn ensure_steamcmd(cancel: &AtomicBool, cb: &mut dyn FnMut(Progress)) -> crate::Result<PathBuf> {
    let p = steamcmd_path()?;
    if p.is_file() {
        return Ok(p);
    }
    cb(Progress { step: "steamcmd", fraction: None, message: "Downloading SteamCMD (~20 MB)".into() });
    let dir = p.parent().unwrap();
    std::fs::create_dir_all(dir)?;
    // its tarball changes without a version, so it cannot have a fixed hash: SteamCMD updates and checks itself.
    // Unpacked aside so an interrupted download never leaves a broken steamcmd.sh behind.
    let tmp = dir.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let tgz = tmp.join("steamcmd_linux.tar.gz");
    let ok = run_logged(
        Command::new("curl")
            .args(["-fsSL", "--retry", "5", "-o"])
            .arg(&tgz)
            .arg("https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz"),
        cancel,
        |_| {},
    )?;
    if !ok {
        bail!("could not download SteamCMD");
    }
    validate_archive(&tgz, &["steamcmd.sh"]).context("the SteamCMD download")?;
    if !Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&tmp).status()?.success() {
        bail!("could not unpack SteamCMD");
    }
    let hash = sha256(&tgz)?;
    std::fs::remove_file(tgz)?;
    // steamcmd.sh is missing here, so this folder of ours holds nothing that works
    std::fs::remove_dir_all(dir)?;
    std::fs::rename(&tmp, dir)?;
    write_manifest(&Manifest { steamcmd_sha256: hash, ..read_manifest() })?;
    Ok(p)
}

fn app_update(steamcmd: &Path, dir: &Path, validate: bool, cancel: &AtomicBool, cb: &mut dyn FnMut(Progress)) -> crate::Result<()> {
    std::fs::create_dir_all(dir)?;
    for attempt in 1..=5 {
        let mut c = Command::new(steamcmd);
        c.arg("+force_install_dir").arg(dir).args(["+login", "anonymous", "+app_update", "740"]);
        if validate {
            c.arg("validate");
        }
        c.arg("+quit");
        let mut last = String::new();
        // bytes and time of the last speed sample, and the speed it gave
        let mut sample: Option<(u64, std::time::Instant)> = None;
        let mut speed = String::new();
        let mut checked = std::time::Instant::now();
        let mut disk_low = false;
        let ok = run_logged(&mut c, cancel, |l| {
            if checked.elapsed().as_secs() >= 10 {
                checked = std::time::Instant::now();
                if free_bytes(dir).is_some_and(|f| f < MIN_FREE_BYTES) {
                    disk_low = true;
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            if let Some((f, mut state)) = steamcmd_progress(l) {
                if let Some((done, _)) = steamcmd_bytes(l) {
                    match sample {
                        Some((before, at)) if at.elapsed().as_secs_f64() >= 2.0 => {
                            speed = speed_text(done.saturating_sub(before), at.elapsed().as_secs_f64());
                            sample = Some((done, std::time::Instant::now()));
                        }
                        None => sample = Some((done, std::time::Instant::now())),
                        _ => {}
                    }
                }
                if !speed.is_empty() && state == "downloading" {
                    state = format!("{state} · {speed}");
                }
                cb(Progress { step: "server", fraction: Some(f), message: format!("CS:GO server (app 740, ~33 GB): {state}") });
            } else if !l.trim().is_empty() {
                last = l.trim().to_string();
            }
        });
        if disk_low {
            cancel.store(false, Ordering::Relaxed);
            bail!("the disk is almost full ({}): free some space and try again, it resumes where it stopped", dir.display());
        }
        let ok = ok?;
        if ok && dir.join("srcds_run").is_file() {
            return Ok(());
        }
        if means_disk_full(&last) {
            bail!("SteamCMD ran out of disk space in {}: free some space and try again, it resumes where it stopped", dir.display());
        }
        // SteamCMD often fails at first (self-updates, timeouts): retry
        cb(Progress { step: "server", fraction: None, message: format!("SteamCMD did not finish ({last}); retry {attempt}/5") });
        std::thread::sleep(std::time::Duration::from_secs(retry_wait() * attempt));
    }
    bail!("could not install app 740; try again (it resumes where it stopped)")
}

/// "Error! App '740' state is 0x202 after update job." is SteamCMD's way of saying the disk is full.
fn means_disk_full(line: &str) -> bool {
    line.contains("0x202") || line.to_ascii_lowercase().contains("no space left") || line.contains("Disk write failure")
}

/// Space to ask for on a first install. Debug builds can change it, as tests install a few KB on a small disk.
fn needed_bytes() -> u64 {
    if cfg!(debug_assertions) {
        if let Some(n) = std::env::var("CSLS_NEEDED_BYTES").ok().and_then(|v| v.parse().ok()) {
            return n;
        }
    }
    NEEDED_BYTES
}

/// Seconds between SteamCMD attempts. Debug builds can skip them so tests do not wait.
fn retry_wait() -> u64 {
    if cfg!(debug_assertions) && std::env::var_os("CSLS_NO_RETRY_WAIT").is_some() {
        0
    } else {
        5
    }
}

fn sha256(file: &Path) -> crate::Result<String> {
    let o = Command::new("sha256sum").arg(file).output().context("running sha256sum")?;
    if !o.status.success() {
        bail!("could not read {}", file.display());
    }
    Ok(String::from_utf8_lossy(&o.stdout).split_whitespace().next().unwrap_or("").to_string())
}

/// Returns the pinned file in `dl`, downloading it if needed. Nothing unverified ever gets the final name.
fn fetch(p: &Pinned, dl: &Path, cancel: &AtomicBool) -> crate::Result<PathBuf> {
    std::fs::create_dir_all(dl)?;
    let f = dl.join(&p.file);
    if f.is_file() && sha256(&f)? == p.sha256 {
        return Ok(f);
    }
    let tmp = dl.join(format!("{}.part", p.file));
    if !run_logged(
        Command::new("curl").args(["-fsSL", "--retry", "5", "-o"]).arg(&tmp).arg(format!("{}/{}", p.base, p.file)),
        cancel,
        |_| {},
    )? {
        let _ = std::fs::remove_file(&tmp);
        bail!("could not download {}", p.file);
    }
    if sha256(&tmp)? != p.sha256 {
        std::fs::remove_file(&tmp)?;
        bail!("{} does not match its checksum: not installing it", p.file);
    }
    std::fs::rename(tmp, &f)?;
    Ok(f)
}

/// Only one install at a time: the lock goes away with the process, even if it crashes.
fn lock_install() -> crate::Result<std::fs::File> {
    std::fs::create_dir_all(paths::data_dir())?;
    try_lock(&paths::data_dir().join("install.lock"))
}

fn try_lock(path: &Path) -> crate::Result<std::fs::File> {
    let f = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(path)?;
    if f.try_lock().is_err() {
        bail!("another install is already running");
    }
    Ok(f)
}

/// Reads the whole archive before anything is unpacked: a damaged file fails here, and no entry may land outside its folder.
fn validate_archive(file: &Path, required: &[&str]) -> crate::Result<()> {
    let o = Command::new("tar").arg("-tzf").arg(file).output().context("running tar")?;
    if !o.status.success() {
        bail!("{} is damaged", file.display());
    }
    let names = String::from_utf8_lossy(&o.stdout);
    check_entries(&names, required).with_context(|| format!("{} is not a valid package", file.display()))
}

fn check_entries(names: &str, required: &[&str]) -> crate::Result<()> {
    for n in names.lines() {
        if n.starts_with('/') || n.split('/').any(|part| part == "..") {
            bail!("unsafe path {n}");
        }
    }
    for r in required {
        if !names.lines().any(|n| n.strip_prefix("./").unwrap_or(n) == *r) {
            bail!("{r} is missing");
        }
    }
    Ok(())
}

fn ensure_free(dir: &Path, needed: u64) -> crate::Result<()> {
    if free_bytes(dir).is_some_and(|f| f < needed + MIN_FREE_BYTES) {
        bail!("not enough free disk space in {} ({} MB needed)", dir.display(), (needed + MIN_FREE_BYTES) >> 20);
    }
    Ok(())
}

/// A package whose program folders are replaced whole, leaving everything else the user may have changed.
struct Addon {
    name: &'static str,
    /// Replaced, keeping any extra files already there (an extension someone added, for example).
    replace: &'static [&'static str],
    required: &'static [&'static str],
}

const METAMOD_ADDON: Addon = Addon { name: "metamod", replace: &["addons/metamod/bin"], required: &["addons/metamod/bin/server.so"] };
const SOURCEMOD_ADDON: Addon = Addon {
    name: "sourcemod",
    replace: &["addons/sourcemod/bin", "addons/sourcemod/scripting", "addons/sourcemod/gamedata", "addons/sourcemod/extensions"],
    required: &["addons/sourcemod/bin/sourcemod_mm_i486.so"],
};

fn stage_dir(csgo: &Path, name: &str) -> PathBuf {
    csgo.join(format!(".csls-stage-{name}"))
}

/// The version replaced last, kept until the next update for the rollback.
fn prev_dir(csgo: &Path, name: &str) -> PathBuf {
    csgo.join(".csls-prev").join(name)
}

/// Copies from `from` the files `to` does not have. Links are skipped, as in backups.
fn merge_missing(from: &Path, to: &Path) -> crate::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (t, dest) = (e.file_type()?, to.join(e.file_name()));
        if t.is_dir() {
            merge_missing(&e.path(), &dest)?;
        } else if t.is_file() && !dest.exists() {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// Unpacks aside, and only then swaps the program folders: a failure never leaves half a SourceMod behind.
fn install_addon(a: &Addon, file: &Path, csgo: &Path) -> crate::Result<()> {
    validate_archive(file, a.required)?;
    ensure_free(csgo, ADDONS_BYTES)?;
    let stage = stage_dir(csgo, a.name);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage)?;
    let st = Command::new("tar").arg("-xzf").arg(file).arg("-C").arg(&stage).status()?;
    if !st.success() {
        let _ = std::fs::remove_dir_all(&stage);
        bail!("could not unpack {}", file.display());
    }
    let prev = prev_dir(csgo, a.name);
    let _ = std::fs::remove_dir_all(&prev);
    for d in a.replace {
        let (new, cur, old) = (stage.join(d), csgo.join(d), prev.join(d));
        if !new.is_dir() {
            continue;
        }
        if cur.exists() {
            std::fs::create_dir_all(old.parent().unwrap())?;
            std::fs::rename(&cur, &old)?;
        }
        std::fs::create_dir_all(cur.parent().unwrap())?;
        std::fs::rename(&new, &cur)?;
        if old.is_dir() {
            merge_missing(&old, &cur)?;
        }
    }
    // the rest (configs, data, the skins database) is only added where missing
    merge_missing(&stage, csgo)?;
    std::fs::remove_dir_all(&stage)?;
    Ok(())
}

/// Puts back the version `install_addon` replaced.
fn rollback_addon(a: &Addon, csgo: &Path) -> crate::Result<bool> {
    let prev = prev_dir(csgo, a.name);
    if !prev.is_dir() {
        return Ok(false);
    }
    for d in a.replace {
        let (old, cur) = (prev.join(d), csgo.join(d));
        if old.is_dir() {
            if cur.exists() {
                std::fs::remove_dir_all(&cur)?;
            }
            std::fs::rename(&old, &cur)?;
        }
    }
    let _ = std::fs::remove_dir_all(&prev);
    Ok(true)
}

/// The steps after the game files, which can be undone. `replaced` says what to undo.
fn install_addons(
    cfg: &Config,
    pins: &PinnedSet,
    update: bool,
    resume: bool,
    cancel: &AtomicBool,
    cb: &mut dyn FnMut(Progress),
    replaced: &mut Vec<&'static Addon>,
) -> crate::Result<()> {
    let csgo = cfg.install_dir().join("csgo");
    let mut man = read_manifest();
    if !csgo.join(METAMOD_ADDON.required[0]).is_file() || update || resume {
        cb(Progress { step: "metamod", fraction: None, message: "Installing Metamod:Source".into() });
        let f = fetch(&pins.metamod, &paths::data_dir().join("dl"), cancel)?;
        install_addon(&METAMOD_ADDON, &f, &csgo)?;
        replaced.push(&METAMOD_ADDON);
        man.metamod = pins.metamod.version.clone();
    }
    if !csgo.join(SOURCEMOD_ADDON.required[0]).is_file() || update || resume {
        cb(Progress { step: "sourcemod", fraction: None, message: "Installing SourceMod".into() });
        let f = fetch(&pins.sourcemod, &paths::data_dir().join("dl"), cancel)?;
        install_addon(&SOURCEMOD_ADDON, &f, &csgo)?;
        replaced.push(&SOURCEMOD_ADDON);
        man.sourcemod = pins.sourcemod.version.clone();
    }
    // the server is 32-bit: the 64-bit vdf stops Metamod from loading
    let _ = std::fs::remove_file(csgo.join("addons/metamod_x64.vdf"));
    std::fs::write(csgo.join("addons/metamod.vdf"), "\"Plugin\"\n{\n\t\"file\"\t\"addons/metamod/bin/server\"\n}\n")?;

    // skin list up to date with the installed game (on failure the package one stays)
    if update || !paths::data_dir().join(crate::skins_gen::catalog_name("en")).is_file() {
        cb(Progress { step: "skins", fraction: None, message: "Generating the skin list".into() });
        let _ = crate::skins_gen::write_catalogs(&csgo);
    }

    cb(Progress { step: "deploy", fraction: None, message: "Compiling plugins and copying configs".into() });
    let r = deploy::deploy(cfg, update, false).context("deploying")?;
    if let Some(e) = r.error_text() {
        bail!("some plugins do not compile:\n{e}");
    }
    write_manifest(&man)?;
    Ok(())
}

/// Installs or updates everything. `update` revalidates with SteamCMD and updates SourceMod.
pub fn install(cfg: &Config, update: bool, cancel: &AtomicBool, cb: &mut dyn FnMut(Progress)) -> crate::Result<()> {
    let dir = cfg.install_dir().to_path_buf();
    let _lock = lock_install()?;
    if crate::server::Server::new(cfg).state() == crate::server::State::Running {
        bail!("stop the server before installing or updating");
    }
    let missing = missing_deps();
    if !missing.is_empty() {
        bail!("missing: {}\n  {}", missing.join(", "), deps_command());
    }
    if !dir.join("srcds_run").is_file() {
        if let Some(free) = free_bytes(&dir) {
            if free < needed_bytes() {
                bail!("~{} GB free needed in {} and there are {} GB", needed_bytes() >> 30, dir.display(), free >> 30);
            }
        }
    }

    // a previous run did not finish: repeat its steps, they are idempotent and SteamCMD resumes
    let marker = dir.join(INCOMPLETE_MARKER);
    let resume = marker.is_file();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&marker, "")?;

    // SteamCMD cannot go back to an older build, but what the user made can be saved first
    if update && !resume && dir.join("srcds_run").is_file() {
        cb(Progress { step: "backup", fraction: None, message: "Saving a backup".into() });
        crate::backup::create_auto(cfg).context("saving a backup before updating")?;
    }

    if !dir.join("srcds_run").is_file() || update || resume {
        let sc = ensure_steamcmd(cancel, cb)?;
        app_update(&sc, &dir, update, cancel, cb)?;
    }

    cb(Progress { step: "fixes", fraction: None, message: "Adjusting the server".into() });
    let _ = std::fs::remove_file(dir.join("bin/libgcc_s.so.1"));
    let csgo = dir.join("csgo");

    let pins = pinned()?;
    let mut replaced: Vec<&Addon> = Vec::new();
    let result = install_addons(cfg, &pins, update, resume, cancel, cb, &mut replaced);
    if let Err(e) = result {
        // a new SourceMod that breaks the plugins must not take the server down with it
        for a in &replaced {
            if rollback_addon(a, &csgo)? {
                cb(Progress { step: "rollback", fraction: None, message: format!("Restored the previous {}", a.name) });
            }
        }
        // what remains works as before, unless this is a first install
        if !replaced.is_empty() && update && !resume {
            std::fs::remove_file(&marker)?;
        }
        return Err(e);
    }
    std::fs::remove_file(&marker)?;
    cb(Progress { step: "done", fraction: Some(1.0), message: "Done".into() });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_steamcmd_progress() {
        let (f, s) = steamcmd_progress(" Update state (0x61) downloading, progress: 12.34 (123 / 456)").unwrap();
        assert!((f - 0.1234).abs() < 1e-9);
        assert_eq!(s, "downloading");
        assert!(steamcmd_progress("Loading Steam API...OK").is_none());
    }
    /// A folder for one test, with the pinned file served from `src` through file://.
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("csls-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn pin(base: &str, sha: &str) -> Pinned {
        Pinned { version: "1".into(), base: format!("file://{base}"), file: "a.tar.gz".into(), sha256: sha.into() }
    }

    #[test]
    fn the_packaged_versions_can_be_read() {
        let p = pinned().unwrap();
        assert!(p.sourcemod.file.contains(&p.sourcemod.version) && p.metamod.file.contains(&p.metamod.version));
        assert_eq!(p.sourcemod.sha256.len(), 64);
    }

    #[test]
    fn refuses_unsafe_or_incomplete_archives() {
        assert!(check_entries("addons/\naddons/x\n", &["addons/x"]).is_ok());
        assert!(check_entries("./addons/x\n", &["addons/x"]).is_ok());
        assert!(check_entries("addons/../../etc/passwd\n", &[]).is_err());
        assert!(check_entries("/etc/passwd\n", &[]).is_err());
        assert!(check_entries("addons/y\n", &["addons/x"]).is_err());
    }

    /// A folder with a package that holds `version` in bin/v, a config and a data file.
    fn package(root: &Path, version: &str) -> PathBuf {
        let src = root.join(format!("src-{version}"));
        std::fs::create_dir_all(src.join("addons/sourcemod/bin")).unwrap();
        std::fs::create_dir_all(src.join("addons/sourcemod/configs")).unwrap();
        std::fs::write(src.join("addons/sourcemod/bin/v"), version).unwrap();
        std::fs::write(src.join("addons/sourcemod/bin/sourcemod_mm_i486.so"), "").unwrap();
        std::fs::write(src.join("addons/sourcemod/configs/core.cfg"), version).unwrap();
        let tgz = root.join(format!("{version}.tar.gz"));
        assert!(Command::new("tar").arg("-czf").arg(&tgz).arg("-C").arg(&src).arg("addons").status().unwrap().success());
        tgz
    }

    #[test]
    fn swaps_programs_keeps_user_files_and_rolls_back() {
        let d = tmp("addon");
        let csgo = d.join("csgo");
        std::fs::create_dir_all(&csgo).unwrap();
        install_addon(&SOURCEMOD_ADDON, &package(&d, "one"), &csgo).unwrap();
        let bin = csgo.join("addons/sourcemod/bin");
        let core = csgo.join("addons/sourcemod/configs/core.cfg");
        assert_eq!(std::fs::read_to_string(bin.join("v")).unwrap(), "one");
        std::fs::write(&core, "mine").unwrap();
        std::fs::write(bin.join("extra.so"), "added by the user").unwrap();

        install_addon(&SOURCEMOD_ADDON, &package(&d, "two"), &csgo).unwrap();
        assert_eq!(std::fs::read_to_string(bin.join("v")).unwrap(), "two");
        assert_eq!(std::fs::read_to_string(&core).unwrap(), "mine", "a config is the user's");
        assert!(bin.join("extra.so").is_file(), "an extra file survives");
        assert!(!stage_dir(&csgo, "sourcemod").exists());

        assert!(rollback_addon(&SOURCEMOD_ADDON, &csgo).unwrap());
        assert_eq!(std::fs::read_to_string(bin.join("v")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&core).unwrap(), "mine");
        assert!(!rollback_addon(&SOURCEMOD_ADDON, &csgo).unwrap(), "nothing left to restore");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_damaged_package_changes_nothing() {
        let d = tmp("damaged");
        let csgo = d.join("csgo");
        std::fs::create_dir_all(&csgo).unwrap();
        install_addon(&SOURCEMOD_ADDON, &package(&d, "one"), &csgo).unwrap();
        let bad = d.join("bad.tar.gz");
        std::fs::write(&bad, "not a tarball").unwrap();
        assert!(install_addon(&SOURCEMOD_ADDON, &bad, &csgo).is_err());
        assert_eq!(std::fs::read_to_string(csgo.join("addons/sourcemod/bin/v")).unwrap(), "one");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn knows_when_steamcmd_means_a_full_disk() {
        assert!(means_disk_full("Error! App '740' state is 0x202 after update job."));
        assert!(!means_disk_full("Error! App '740' state is 0x402 after update job."));
    }

    #[test]
    fn fetch_checks_the_hash() {
        let d = tmp("fetch");
        let (src, dl) = (d.join("src"), d.join("dl"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.tar.gz"), "hello\n").unwrap();
        let cancel = AtomicBool::new(false);
        let good = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";

        let bad = fetch(&pin(src.to_str().unwrap(), "00"), &dl, &cancel).unwrap_err();
        assert!(bad.to_string().contains("checksum"), "{bad}");
        assert!(!dl.join("a.tar.gz").exists() && !dl.join("a.tar.gz.part").exists());

        let f = fetch(&pin(src.to_str().unwrap(), good), &dl, &cancel).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "hello\n");

        // a corrupt copy is replaced, a good one needs no network
        std::fs::write(&f, "corrupt").unwrap();
        fetch(&pin(src.to_str().unwrap(), good), &dl, &cancel).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "hello\n");
        fetch(&pin("/does/not/exist", good), &dl, &cancel).unwrap();
        assert!(fetch(&pin("/does/not/exist", "00"), &dl, &cancel).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn only_one_install_at_a_time() {
        let d = tmp("lock");
        let p = d.join("install.lock");
        // another test starting a process holds a copy of the file for an instant, until its exec
        let lock_soon = || {
            (0..50).find_map(|_| {
                try_lock(&p).ok().or_else(|| {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    None
                })
            })
        };
        let first = lock_soon().unwrap();
        assert!(try_lock(&p).is_err());
        drop(first);
        assert!(lock_soon().is_some());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn interrupted_install_is_not_installed() {
        let d = tmp("marker");
        std::fs::write(d.join("srcds_run"), "").unwrap();
        let mut cfg = Config::default();
        cfg.server.install_dir = d.clone();
        assert!(crate::server::Server::new(&cfg).is_installed());
        std::fs::write(d.join(INCOMPLETE_MARKER), "").unwrap();
        assert!(!crate::server::Server::new(&cfg).is_installed());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn reads_download_speed() {
        assert_eq!(steamcmd_bytes(" Update state (0x61) downloading, progress: 12.34 (123 / 456)"), Some((123, 456)));
        assert_eq!(steamcmd_bytes("Loading Steam API...OK"), None);
        assert_eq!(speed_text(5_000_000, 2.0), "2.5 MB/s");
    }

    #[test]
    fn free_space_of_folder() {
        assert!(free_bytes(Path::new("/tmp/no/existe")).is_some());
    }
}
