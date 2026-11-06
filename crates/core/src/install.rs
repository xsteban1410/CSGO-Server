//! Install (formerly setup.sh): idempotent steps with progress. Can be repeated and resumed.
use crate::config::Config;
use crate::{deploy, paths};
use anyhow::{bail, Context};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Present while an install runs and after one is interrupted or fails: the server may be half written.
pub const INCOMPLETE_MARKER: &str = ".csls-install-incomplete";

/// A SourceMod or Metamod build this version was tested with. Bumping it means testing the plugins again.
struct Pinned {
    base: &'static str,
    file: &'static str,
    sha256: &'static str,
}

const METAMOD: Pinned = Pinned {
    base: "https://mms.alliedmods.net/mmsdrop/1.12",
    file: "mmsource-1.12.0-git1229-linux.tar.gz",
    sha256: "df2f8723741d90aea730e06103e57844bac970225be9fd4137ed1d2b753bedb2",
};
const SOURCEMOD: Pinned = Pinned {
    base: "https://sm.alliedmods.net/smdrop/1.12",
    file: "sourcemod-1.12.0-git7260-linux.tar.gz",
    sha256: "b01719e6143bc7d5bb63aae8086d031574da6ef838f041ba4b8ab72c8c75b888",
};

/// Space needed for app 740, with margin.
pub const NEEDED_BYTES: u64 = 40 * 1024 * 1024 * 1024;

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
    let get = |k: &str| {
        t.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
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
    let errt = std::thread::spawn(move || {
        BufReader::new(err).lines().map_while(Result::ok).collect::<Vec<_>>().join("\n")
    });
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
        Command::new("curl").args(["-fsSL", "--retry", "5", "-o"]).arg(&tgz).arg("https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz"),
        cancel,
        |_| {},
    )?;
    if !ok || !Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&tmp).status()?.success() {
        bail!("could not download SteamCMD");
    }
    std::fs::remove_file(tgz)?;
    // steamcmd.sh is missing here, so this folder of ours holds nothing that works
    std::fs::remove_dir_all(dir)?;
    std::fs::rename(&tmp, dir)?;
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
        let ok = run_logged(&mut c, cancel, |l| {
            if let Some((f, state)) = steamcmd_progress(l) {
                cb(Progress { step: "server", fraction: Some(f), message: format!("CS:GO server (app 740, ~33 GB): {state}") });
            } else if !l.trim().is_empty() {
                last = l.trim().to_string();
            }
        })?;
        if ok && dir.join("srcds_run").is_file() {
            return Ok(());
        }
        // SteamCMD often fails at first (self-updates, timeouts): retry
        cb(Progress { step: "server", fraction: None, message: format!("SteamCMD did not finish ({last}); retry {attempt}/5") });
        std::thread::sleep(std::time::Duration::from_secs(5 * attempt));
    }
    bail!("could not install app 740; try again (it resumes where it stopped)")
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
    let f = dl.join(p.file);
    if f.is_file() && sha256(&f)? == p.sha256 {
        return Ok(f);
    }
    let tmp = dl.join(format!("{}.part", p.file));
    if !run_logged(Command::new("curl").args(["-fsSL", "--retry", "5", "-o"]).arg(&tmp).arg(format!("{}/{}", p.base, p.file)), cancel, |_| {})? {
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

fn tar(args: &[&str], file: &Path, dest: &Path) -> crate::Result<()> {
    let st = Command::new("tar").arg("-xzf").arg(file).arg("-C").arg(dest).args(args).status()?;
    if !st.success() {
        bail!("tar failed on {}", file.display());
    }
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
            if free < NEEDED_BYTES {
                bail!("~40 GB free needed in {} and there are {} GB", dir.display(), free / (1 << 30));
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

    if !csgo.join("addons/metamod/bin/server.so").is_file() || resume {
        cb(Progress { step: "metamod", fraction: None, message: "Installing Metamod:Source".into() });
        let f = fetch(&METAMOD, &paths::data_dir().join("dl"), cancel)?;
        tar(&[], &f, &csgo)?;
    }
    if !csgo.join("addons/sourcemod/bin/sourcemod_mm_i486.so").is_file() || update || resume {
        cb(Progress { step: "sourcemod", fraction: None, message: "Installing SourceMod".into() });
        let f = fetch(&SOURCEMOD, &paths::data_dir().join("dl"), cancel)?;
        // shortcut: a file cut in half by an interruption stays if it is not in the overwrite list below; deleting
        // addons/sourcemod would also delete the skins database. Improve it if a broken config ever shows up.
        // do not overwrite configs, data or the skins database when reinstalling
        tar(&["--skip-old-files"], &f, &csgo)?;
        tar(
            &["--overwrite", "--wildcards", "addons/sourcemod/bin/*", "addons/sourcemod/scripting/*", "addons/sourcemod/gamedata/*", "addons/sourcemod/extensions/*"],
            &f,
            &csgo,
        )?;
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

    fn pinned(base: &str, sha: &str) -> Pinned {
        Pinned { base: Box::leak(format!("file://{base}").into_boxed_str()), file: "a.tar.gz", sha256: Box::leak(sha.to_string().into_boxed_str()) }
    }

    #[test]
    fn fetch_checks_the_hash() {
        let d = tmp("fetch");
        let (src, dl) = (d.join("src"), d.join("dl"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.tar.gz"), "hello\n").unwrap();
        let cancel = AtomicBool::new(false);
        let good = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";

        let bad = fetch(&pinned(src.to_str().unwrap(), "00"), &dl, &cancel).unwrap_err();
        assert!(bad.to_string().contains("checksum"), "{bad}");
        assert!(!dl.join("a.tar.gz").exists() && !dl.join("a.tar.gz.part").exists());

        let f = fetch(&pinned(src.to_str().unwrap(), good), &dl, &cancel).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "hello\n");

        // a corrupt copy is replaced, a good one needs no network
        std::fs::write(&f, "corrupt").unwrap();
        fetch(&pinned(src.to_str().unwrap(), good), &dl, &cancel).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "hello\n");
        fetch(&pinned("/does/not/exist", good), &dl, &cancel).unwrap();
        assert!(fetch(&pinned("/does/not/exist", "00"), &dl, &cancel).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn only_one_install_at_a_time() {
        let d = tmp("lock");
        let p = d.join("install.lock");
        let first = try_lock(&p).unwrap();
        assert!(try_lock(&p).is_err());
        drop(first);
        assert!(try_lock(&p).is_ok());
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
    fn free_space_of_folder() {
        assert!(free_bytes(Path::new("/tmp/no/existe")).is_some());
    }
}
