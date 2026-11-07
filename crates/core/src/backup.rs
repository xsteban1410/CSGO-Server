//! Copies of what cannot be downloaded again: config, modes, plugins, presets and the skins database.
//! Never the server. A backup holds the rcon password and tokens, so it is readable by the user only.
use crate::config::Config;
use crate::{paths, skins};
use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Automatic backups kept; the ones the user makes are never removed.
const KEEP_AUTO: usize = 5;
const CONFIG_DIR: &str = "config";
const SKINS_FILE: &str = "skins.sq3";

/// Where the data lives.
struct Roots {
    config: PathBuf,
    skins: PathBuf,
    backups: PathBuf,
}

impl Roots {
    fn new(cfg: &Config) -> Roots {
        Roots { config: paths::config_dir(), skins: skins::db_path(&cfg.install_dir().join("csgo")), backups: backups_dir() }
    }
}

/// A file a restore would write.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    /// true = it does not exist now; false = it exists and would be replaced.
    pub new: bool,
}

pub fn backups_dir() -> PathBuf {
    paths::data_dir().join("backups")
}

fn run(cmd: &mut Command, what: &str) -> crate::Result<String> {
    let o = cmd.output().with_context(|| format!("running {what}"))?;
    if !o.status.success() {
        bail!("{what} failed: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Copies the regular files of a folder. Links are skipped: a backup holds no paths from outside.
fn copy_tree(from: &Path, to: &Path) -> crate::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = e.file_type()?;
        if t.is_dir() {
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        } else if t.is_file() {
            std::fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}

fn files_in(dir: &Path, out: &mut Vec<PathBuf>) -> crate::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            files_in(&e.path(), out)?;
        } else {
            out.push(e.path());
        }
    }
    Ok(())
}

/// A scratch folder next to the backups, removed when dropped.
struct Work(PathBuf);

impl Work {
    fn new(backups: &Path) -> crate::Result<Work> {
        // a restore makes a backup while its own work folder is in use
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let w = backups.join(format!(".work-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&w);
        std::fs::create_dir_all(&w)?;
        Ok(Work(w))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn create_in(r: &Roots, prefix: &str) -> crate::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(&r.backups)?;
    std::fs::set_permissions(&r.backups, std::fs::Permissions::from_mode(0o700))?;
    let work = Work::new(&r.backups)?;
    if r.config.is_dir() {
        copy_tree(&r.config, &work.0.join(CONFIG_DIR))?;
    }
    if r.skins.is_file() {
        std::fs::copy(&r.skins, work.0.join(SKINS_FILE))?;
    }
    let stamp = run(Command::new("date").arg("+%Y%m%d-%H%M%S"), "date")?;
    let stem = format!("{prefix}-{}", stamp.trim());
    let mut out = r.backups.join(format!("{stem}.tar.gz"));
    let mut n = 1;
    while out.exists() {
        out = r.backups.join(format!("{stem}-{n}.tar.gz"));
        n += 1;
    }
    let part = out.with_extension("part");
    std::fs::File::create(&part)?.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    run(Command::new("tar").arg("-czf").arg(&part).arg("-C").arg(&work.0).arg("."), "tar")?;
    std::fs::rename(&part, &out)?;
    Ok(out)
}

/// A backup the user asked for.
pub fn create(cfg: &Config) -> crate::Result<PathBuf> {
    create_in(&Roots::new(cfg), "backup")
}

/// A backup before something risky; only the newest few stay.
pub fn create_auto(cfg: &Config) -> crate::Result<PathBuf> {
    let r = Roots::new(cfg);
    let out = create_in(&r, "auto")?;
    prune(&r.backups)?;
    Ok(out)
}

fn prune(dir: &Path) -> crate::Result<()> {
    let mut auto: Vec<PathBuf> =
        list_in(dir)?.into_iter().filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("auto-"))).collect();
    // list_in gives the newest first
    for old in auto.drain(KEEP_AUTO.min(auto.len())..) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

fn list_in(dir: &Path) -> crate::Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    if dir.is_dir() {
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            if p.to_string_lossy().ends_with(".tar.gz") {
                v.push(p);
            }
        }
    }
    // the stamp in the name sorts by date, whichever the prefix
    v.sort_by_key(|p| std::cmp::Reverse(p.file_name().map(|n| stamp_of(&n.to_string_lossy()))));
    Ok(v)
}

fn stamp_of(name: &str) -> String {
    name.split_once('-').map_or(name, |x| x.1).to_string()
}

/// Backups, newest first.
pub fn list() -> crate::Result<Vec<PathBuf>> {
    list_in(&backups_dir())
}

/// Whether a name inside an archive stays where a restore may write.
fn safe_member(name: &str) -> bool {
    let name = name.strip_prefix("./").unwrap_or(name).trim_end_matches('/');
    let inside = name.is_empty() || name == CONFIG_DIR || name == SKINS_FILE || name.starts_with(&format!("{CONFIG_DIR}/"));
    inside && !name.split('/').any(|c| c == "..")
}

/// An archive can come from anywhere: it is checked before anything is unpacked.
fn unpack(archive: &Path, into: &Path) -> crate::Result<()> {
    let names = run(Command::new("tar").arg("-tzf").arg(archive), "tar")?;
    if let Some(bad) = names.lines().find(|n| !safe_member(n)) {
        bail!("not a backup of this app: it contains {bad}");
    }
    let details = run(Command::new("tar").arg("-tzvf").arg(archive), "tar")?;
    if let Some(bad) = details.lines().find(|l| !l.starts_with(['-', 'd'])) {
        bail!("not a backup of this app: it contains {bad}");
    }
    run(Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(into).arg("--no-same-owner"), "tar")?;
    Ok(())
}

/// Pairs of (file in the unpacked backup, where it goes) that differ from what is there now.
fn differences(r: &Roots, unpacked: &Path) -> crate::Result<Vec<(PathBuf, PathBuf)>> {
    let mut pairs = Vec::new();
    let conf = unpacked.join(CONFIG_DIR);
    if conf.is_dir() {
        let mut files = Vec::new();
        files_in(&conf, &mut files)?;
        for f in files {
            pairs.push((f.clone(), r.config.join(f.strip_prefix(&conf)?)));
        }
    }
    if unpacked.join(SKINS_FILE).is_file() {
        pairs.push((unpacked.join(SKINS_FILE), r.skins.clone()));
    }
    pairs.retain(|(from, to)| match (std::fs::read(from), std::fs::read(to)) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    });
    Ok(pairs)
}

fn preview_in(r: &Roots, archive: &Path) -> crate::Result<Vec<Change>> {
    std::fs::create_dir_all(&r.backups)?;
    let work = Work::new(&r.backups)?;
    unpack(archive, &work.0)?;
    Ok(differences(r, &work.0)?.into_iter().map(|(_, to)| Change { new: !to.exists(), path: to }).collect())
}

fn restore_in(r: &Roots, archive: &Path) -> crate::Result<Vec<Change>> {
    std::fs::create_dir_all(&r.backups)?;
    let work = Work::new(&r.backups)?;
    unpack(archive, &work.0)?;
    let pairs = differences(r, &work.0)?;
    if !pairs.is_empty() {
        // undoing this restore must be possible too
        create_in(r, "auto")?;
        prune(&r.backups)?;
    }
    let mut changes = Vec::new();
    for (from, to) in pairs {
        changes.push(Change { new: !to.exists(), path: to.clone() });
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d)?;
        }
        // rename: the file is either the old one or the new one, never half written
        let part = to.with_extension("csls-restore");
        std::fs::copy(&from, &part)?;
        std::fs::rename(&part, &to)?;
    }
    Ok(changes)
}

/// What a restore would change, without changing anything.
pub fn preview(cfg: &Config, archive: &Path) -> crate::Result<Vec<Change>> {
    preview_in(&Roots::new(cfg), archive)
}

/// Puts back the files of a backup. Files the backup does not have stay. The server must be stopped.
pub fn restore(cfg: &Config, archive: &Path) -> crate::Result<Vec<Change>> {
    if crate::server::Server::new(cfg).state() == crate::server::State::Running {
        bail!("stop the server before restoring a backup");
    }
    restore_in(&Roots::new(cfg), archive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> (PathBuf, Roots) {
        let d = std::env::temp_dir().join(format!("csls-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let r = Roots { config: d.join("conf"), skins: d.join("db/skins.sq3"), backups: d.join("backups") };
        std::fs::create_dir_all(r.config.join("modes")).unwrap();
        std::fs::create_dir_all(r.skins.parent().unwrap()).unwrap();
        std::fs::write(r.config.join("config.toml"), "one").unwrap();
        std::fs::write(r.config.join("modes/mine.toml"), "mode").unwrap();
        std::fs::write(&r.skins, "skins").unwrap();
        (d, r)
    }

    #[test]
    fn restores_what_changed_and_only_that() {
        use std::os::unix::fs::PermissionsExt;
        let (d, r) = setup("round");
        let b = create_in(&r, "backup").unwrap();
        assert_eq!(std::fs::metadata(&b).unwrap().permissions().mode() & 0o777, 0o600);

        std::fs::write(r.config.join("config.toml"), "two").unwrap();
        std::fs::remove_file(r.config.join("modes/mine.toml")).unwrap();
        std::fs::remove_file(&r.skins).unwrap();
        std::fs::write(r.config.join("later.txt"), "keep").unwrap();

        // the preview touches nothing
        let mut p = preview_in(&r, &b).unwrap();
        p.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(p.len(), 3);
        assert!(p.iter().any(|c| c.path == r.config.join("config.toml") && !c.new));
        assert!(p.iter().any(|c| c.path == r.skins && c.new));
        assert_eq!(std::fs::read_to_string(r.config.join("config.toml")).unwrap(), "two");

        assert_eq!(restore_in(&r, &b).unwrap().len(), 3);
        assert_eq!(std::fs::read_to_string(r.config.join("config.toml")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(r.config.join("modes/mine.toml")).unwrap(), "mode");
        assert_eq!(std::fs::read_to_string(&r.skins).unwrap(), "skins");
        assert_eq!(std::fs::read_to_string(r.config.join("later.txt")).unwrap(), "keep");

        // a restore can be undone
        let undo = list_in(&r.backups).unwrap().into_iter().find(|p| p.to_string_lossy().contains("auto-")).unwrap();
        restore_in(&r, &undo).unwrap();
        assert_eq!(std::fs::read_to_string(r.config.join("config.toml")).unwrap(), "two");
        assert!(preview_in(&r, &undo).unwrap().is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn rejects_archives_that_write_elsewhere() {
        let (d, r) = setup("evil");
        std::fs::create_dir_all(&r.backups).unwrap();
        std::fs::write(d.join("a"), "x").unwrap();
        let evil = d.join("evil.tar.gz");
        let tar =
            |args: &[&str]| assert!(Command::new("tar").current_dir(&d).arg("-czPf").arg(&evil).args(args).status().unwrap().success());

        tar(&["--transform", "s,^a,config/../../evil,", "a"]);
        assert!(preview_in(&r, &evil).is_err());
        tar(&["--transform", "s,^a,other,", "a"]);
        assert!(preview_in(&r, &evil).is_err());
        std::os::unix::fs::symlink("/etc", d.join("link")).unwrap();
        tar(&["--transform", "s,^link,config/link,", "link"]);
        assert!(preview_in(&r, &evil).is_err());
        assert!(!d.parent().unwrap().join("evil").exists());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn prunes_only_old_automatic_backups() {
        let (d, r) = setup("prune");
        std::fs::create_dir_all(&r.backups).unwrap();
        for i in 0..8 {
            std::fs::write(r.backups.join(format!("auto-20260101-00000{i}.tar.gz")), "").unwrap();
        }
        std::fs::write(r.backups.join("backup-20200101-000000.tar.gz"), "").unwrap();
        prune(&r.backups).unwrap();
        let left: Vec<String> =
            list_in(&r.backups).unwrap().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(left.len(), KEEP_AUTO + 1);
        assert_eq!(left[0], "auto-20260101-000007.tar.gz");
        assert!(left.contains(&"backup-20200101-000000.tar.gz".to_string()));
        assert!(!left.contains(&"auto-20260101-000002.tar.gz".to_string()));
        std::fs::remove_dir_all(d).unwrap();
    }
}
