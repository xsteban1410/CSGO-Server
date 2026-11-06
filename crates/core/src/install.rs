//! Install (formerly setup.sh): idempotent steps with progress. Can be repeated and resumed.
use crate::config::Config;
use crate::{deploy, paths};
use anyhow::{bail, Context};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

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
    let tgz = dir.join("steamcmd_linux.tar.gz");
    let ok = run_logged(
        Command::new("curl").args(["-fsSL", "-o"]).arg(&tgz).arg("https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz"),
        cancel,
        |_| {},
    )?;
    if !ok || !Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(dir).status()?.success() {
        bail!("could not download SteamCMD");
    }
    let _ = std::fs::remove_file(tgz);
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
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
    bail!("could not install app 740; try again (it resumes where it stopped)")
}

/// Downloads the latest build of an AlliedModders branch (mmsdrop/smdrop).
fn alliedmods(base: &str, prefix: &str, cancel: &AtomicBool) -> crate::Result<PathBuf> {
    let dl = paths::data_dir().join("dl");
    std::fs::create_dir_all(&dl)?;
    let o = Command::new("curl").args(["-fsSL", &format!("{base}/{prefix}-latest-linux")]).output()?;
    let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if !o.status.success() || !name.ends_with(".tar.gz") || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
        bail!("unexpected response from {base}: {name}");
    }
    let f = dl.join(&name);
    if !f.is_file() {
        let tmp = dl.join(format!("{name}.part"));
        if !run_logged(Command::new("curl").args(["-fsSL", "-o"]).arg(&tmp).arg(format!("{base}/{name}")), cancel, |_| {})? {
            bail!("could not download {name}");
        }
        std::fs::rename(tmp, &f)?;
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

    if !dir.join("srcds_run").is_file() || update {
        let sc = ensure_steamcmd(cancel, cb)?;
        app_update(&sc, &dir, update, cancel, cb)?;
    }

    cb(Progress { step: "fixes", fraction: None, message: "Adjusting the server".into() });
    let _ = std::fs::remove_file(dir.join("bin/libgcc_s.so.1"));
    let csgo = dir.join("csgo");

    if !csgo.join("addons/metamod/bin/server.so").is_file() {
        cb(Progress { step: "metamod", fraction: None, message: "Installing Metamod:Source".into() });
        let f = alliedmods("https://mms.alliedmods.net/mmsdrop/1.12", "mmsource", cancel)?;
        tar(&[], &f, &csgo)?;
    }
    if !csgo.join("addons/sourcemod/bin/sourcemod_mm_i486.so").is_file() || update {
        cb(Progress { step: "sourcemod", fraction: None, message: "Installing SourceMod".into() });
        let f = alliedmods("https://sm.alliedmods.net/smdrop/1.12", "sourcemod", cancel)?;
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
    #[test]
    fn free_space_of_folder() {
        assert!(free_bytes(Path::new("/tmp/no/existe")).is_some());
    }
}
