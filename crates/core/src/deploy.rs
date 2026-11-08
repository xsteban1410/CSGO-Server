//! Deploy (formerly desplegar.sh): compiles plugins that are missing or out of date,
//! copies cfgs, gamedata and skins, and hot-reloads if the server is running.
use crate::config::Config;
use crate::paths;
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub main: PathBuf,
    /// Folder with its sources (to know whether something changed).
    pub dir: PathBuf,
}

#[derive(Debug, Default)]
pub struct Report {
    pub compiled: Vec<String>,
    /// (plugin, spcomp output)
    pub errors: Vec<(String, String)>,
    pub reloaded: bool,
    /// Own plugins whose .sp was deleted: they get removed from the server.
    pub removed: Vec<String>,
    /// Plugin messages changed: a running server has to read them again.
    pub translations: bool,
}

impl Report {
    pub fn error_text(&self) -> Option<String> {
        (!self.errors.is_empty()).then(|| self.errors.iter().map(|(n, e)| format!("{n}:\n{e}")).collect::<Vec<_>>().join("\n"))
    }
}

/// Plugins of the engine. Nothing from the hub or the user may take one of these names.
pub const ENGINE_PLUGINS: &[&str] = &["modos", "csls_events", "csls_directconnect", "csls_admins", "csls_anticheat"];

/// (plugin, gamedata) no longer used: nolobbyreservation (no license) -> csls_directconnect.
const OBSOLETE: &[(&str, &str)] = &[("nolobbyreservation", "nolobbyreservation.games.txt")];
/// Plugins the engine used to ship that are hub items now: removed unless the hub installed them again.
const MOVED_TO_HUB: &[&str] = &[
    "skins",
    "modos_cheats",
    "modos_aleatorio",
    "modos_unabala",
    "modos_vampiro",
    "modos_jefe",
    "modos_clutch",
    "modos_zombies",
];

/// Plugins in a folder: top-level X.sp with `myinfo` (the rest are includes)
/// and subfolders N/ with N/N.sp. Files with other names are skipped.
pub fn find_plugins(dir: &Path) -> Vec<Plugin> {
    let mut v = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        // the name goes into `sm plugins load <name>` over rcon
        if !crate::plugins::valid_name(&stem) {
            continue;
        }
        if p.is_dir() {
            let main = p.join(format!("{stem}.sp"));
            if main.is_file() {
                v.push(Plugin { name: stem, main, dir: p });
            }
        } else if p.extension().is_some_and(|e| e == "sp") && std::fs::read_to_string(&p).is_ok_and(|t| t.contains("myinfo")) {
            v.push(Plugin { name: stem, main: p, dir: dir.to_path_buf() });
        }
    }
    v
}

/// Hash of the sources in a plugin's folder. Dates are no use here: a package keeps its build date, so
/// after switching between versions an older source could be newer code than the .smx, or the other way round,
/// and the plugin then no longer matches its translations. The engine API (`plugin/include`) counts too,
/// since every plugin is built against it.
fn source_hash(dir: &Path) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let api = paths::resources_dir().ok().map(|r| r.join("plugin/include"));
    let mut files: Vec<PathBuf> = std::iter::once(dir.to_path_buf())
        .chain(api)
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sp" || x == "inc" || x == "txt"))
        .collect();
    files.sort();
    let mut h = DefaultHasher::new();
    for f in files {
        f.file_name().hash(&mut h);
        std::fs::read(&f).unwrap_or_default().hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Remembers which sources an .smx was built from. SourceMod only loads *.smx, so it ignores this file.
fn stamp_path(smx: &Path) -> PathBuf {
    smx.with_extension("smx.src")
}

/// Plugins whose compiled file does not come from the current sources.
pub fn stale_plugins(cfg: &Config) -> crate::Result<Vec<String>> {
    let sm = sourcemod_dir(cfg);
    let plugins = all_plugins(cfg)?;
    Ok(plugins
        .iter()
        .filter(|p| (cfg.server.skins || p.name != "skins") && needs_compile(p, &sm.join("plugins").join(format!("{}.smx", p.name))))
        .map(|p| p.name.clone())
        .collect())
}

pub fn needs_compile(p: &Plugin, smx: &Path) -> bool {
    // shortcut: does not follow external #includes; if a system include changes, force it with `deploy --force`
    !smx.is_file() || std::fs::read_to_string(stamp_path(smx)).ok().as_deref() != Some(source_hash(&p.dir).as_str())
}

pub fn sourcemod_dir(cfg: &Config) -> PathBuf {
    cfg.install_dir().join("csgo/addons/sourcemod")
}

fn spcomp(sm: &Path) -> crate::Result<PathBuf> {
    let s = sm.join("scripting");
    [s.join("spcomp64"), s.join("spcomp")]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| anyhow::anyhow!("spcomp not found in {} (is SourceMod installed?)", s.display()))
}

/// Compiles a plugin to a temp file and moves it only on success (on failure the previous .smx stays).
pub fn compile(sm: &Path, p: &Plugin) -> crate::Result<std::result::Result<(), String>> {
    let out = sm.join("plugins").join(format!("{}.smx", p.name));
    let tmp = out.with_extension("smx.tmp");
    let o = Command::new(spcomp(sm)?)
        .arg(format!("-i{}", sm.join("scripting/include").display()))
        // the engine API (csls.inc) for every plugin, also the ones from the hub or the user
        .args(paths::resources_dir().ok().map(|r| format!("-i{}", r.join("plugin/include").display())))
        .arg(format!("-i{}", p.dir.display()))
        .arg(&p.main)
        .arg(format!("-o{}", tmp.display()))
        .current_dir(&p.dir)
        .output()
        .context("running spcomp")?;
    let text = String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    if !o.status.success() || !tmp.is_file() {
        let _ = std::fs::remove_file(&tmp);
        let errs: Vec<&str> = text.lines().filter(|l| l.contains("error") || l.contains("fatal")).collect();
        return Ok(Err(if errs.is_empty() { text } else { errs.join("\n") }));
    }
    std::fs::rename(&tmp, &out)?;
    std::fs::write(stamp_path(&out), source_hash(&p.dir))?;
    Ok(Ok(()))
}

/// Copies leaving the destination at 0644: package resources can be read-only
/// and fs::copy copies permissions, so the next deploy could not overwrite.
pub fn copy_writable(from: &Path, to: &Path) -> crate::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if to.exists() {
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o644));
    }
    std::fs::copy(from, to).with_context(|| format!("copiando {} a {}", from.display(), to.display()))?;
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o644))?;
    Ok(())
}

fn copy_if_differs(from: &Path, to: &Path) -> crate::Result<()> {
    if std::fs::read(from).ok() != std::fs::read(to).ok() {
        copy_writable(from, to)?;
    }
    Ok(())
}

/// Copies the files that differ and says whether there were any.
fn copy_glob(from: &Path, suffix: &str, to: &Path, rename: impl Fn(&str) -> String) -> crate::Result<bool> {
    std::fs::create_dir_all(to)?;
    let mut changed = false;
    for e in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let dest = to.join(rename(&n));
        if n.ends_with(suffix) && e.path().is_file() && std::fs::read(e.path()).ok() != std::fs::read(&dest).ok() {
            copy_writable(&e.path(), &dest)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Plugins of the installed hub items. None may take an engine plugin's name.
fn hub_plugins(hub: &[crate::hub::Installed]) -> Vec<Plugin> {
    hub.iter().flat_map(|i| find_plugins(&i.plugin_dir())).filter(|p| !ENGINE_PLUGINS.contains(&p.name.as_str())).collect()
}

/// All the plugins (engine + hub + user) that apply with this config.
pub fn all_plugins(cfg: &Config) -> crate::Result<Vec<Plugin>> {
    let res = paths::resources_dir()?;
    let mut v = find_plugins(&res.join("plugin"));
    v.extend(hub_plugins(&crate::hub::installed()));
    v.extend(find_plugins(&paths::user_plugins_dir()));
    if !cfg.server.skins {
        v.retain(|p| p.name != "skins");
    }
    Ok(v)
}

/// Deploys. `force` recompiles everything; `reload` hot-reloads what was compiled (if rcon is up).
pub fn deploy(cfg: &Config, force: bool, reload: bool) -> crate::Result<Report> {
    let res = paths::resources_dir()?;
    let sm = sourcemod_dir(cfg);
    let csgo = cfg.install_dir().join("csgo");
    let mut r = Report::default();

    let plugins = all_plugins(cfg)?;
    std::fs::create_dir_all(sm.join("plugins"))?;
    for p in &plugins {
        let smx = sm.join("plugins").join(format!("{}.smx", p.name));
        // turned off in the plugin manager: compiling would turn it on again
        let disabled = sm.join("plugins/disabled").join(format!("{}.smx", p.name)).is_file();
        if !disabled && (force || needs_compile(p, &smx)) {
            match compile(&sm, p)? {
                Ok(()) => r.compiled.push(p.name.clone()),
                Err(e) => r.errors.push((p.name.clone(), e)),
            }
        }
        copy_glob(&p.dir, ".games.txt", &sm.join("gamedata"), str::to_string)?;
        let tr = p.dir.join("translations");
        if tr.is_dir() {
            r.translations |= copy_glob(&tr, ".phrases.txt", &sm.join("translations"), str::to_string)?;
        }
    }
    let remove_smx = |n: &str| {
        let smx = sm.join("plugins").join(format!("{n}.smx"));
        let _ = std::fs::remove_file(stamp_path(&smx));
        let _ = std::fs::remove_file(smx);
    };
    if !cfg.server.skins {
        remove_smx("skins");
    }
    // plugins from earlier versions replaced by others
    for (n, gd) in OBSOLETE {
        remove_smx(n);
        let _ = std::fs::remove_file(sm.join("gamedata").join(gd));
    }
    for n in MOVED_TO_HUB.iter().filter(|n| !plugins.iter().any(|p| p.name == **n)) {
        remove_smx(n);
    }
    // user and hub plugins deployed earlier whose .sp no longer exists (removed by hand or uninstalled)
    let hub = crate::hub::installed();
    let manifest = paths::data_dir().join("user_plugins.txt");
    let extra: Vec<String> = find_plugins(&paths::user_plugins_dir()).into_iter().chain(hub_plugins(&hub)).map(|p| p.name).collect();
    let before = std::fs::read_to_string(&manifest).unwrap_or_default();
    r.removed = gone(&before, &extra, &plugins).into_iter().filter(|n| crate::plugins::valid_name(n)).collect();
    for n in &r.removed {
        remove_smx(n);
    }
    std::fs::create_dir_all(paths::data_dir())?;
    std::fs::write(&manifest, extra.join("\n"))?;

    // data files of hub items (catalogs, lists) go to sourcemod/configs
    for i in &hub {
        copy_glob(&i.data_dir(), ".cfg", &sm.join("configs"), str::to_string)?;
        copy_glob(&i.data_dir(), ".txt", &sm.join("configs"), str::to_string)?;
    }
    // skins: the list generated when updating the game or the one an item brings, in the chosen language
    let catalog = crate::skins_gen::catalog_name(cfg.language.as_str());
    let skins = [paths::data_dir().join(&catalog), res.join("data").join(&catalog)]
        .into_iter()
        .chain(hub.iter().map(|i| i.data_dir().join(&catalog)))
        .find(|p| p.is_file());
    if let Some(s) = skins {
        std::fs::create_dir_all(sm.join("configs"))?;
        copy_writable(&s, &sm.join("configs/skins.cfg"))?;
    }
    // giving special knives with GivePlayerItem requires bypassing Valve's rules
    let core = sm.join("configs/core.cfg");
    if let Ok(t) = std::fs::read_to_string(&core) {
        let off = (cfg.server.skins && plugins.iter().any(|p| p.name == "skins")) || hub.iter().any(|i| i.item.guidelines_off);
        let want = if off { "no" } else { "yes" };
        let n = set_kv(&t, "FollowCSGOServerGuidelines", want);
        if n != t {
            std::fs::write(&core, n)?;
        }
    }

    copy_glob(&res.join("cfg"), ".cfg", &csgo.join("cfg"), str::to_string)?;
    // mode cfgs of the hub and the user; the ones gone since the last deploy are removed from the server
    let mut cfgs: Vec<String> = Vec::new();
    for i in &hub {
        for (id, path) in crate::hub::mode_files(i) {
            let n = format!("modo_{id}.cfg");
            copy_if_differs(&path, &csgo.join("cfg").join(&n))?;
            cfgs.push(n);
        }
    }
    for e in std::fs::read_dir(paths::user_modes_dir()).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if let Some(id) = n.strip_suffix(".cfg").filter(|id| crate::modes::valid_id(id)) {
            let n = format!("modo_{id}.cfg");
            copy_if_differs(&e.path(), &csgo.join("cfg").join(&n))?;
            cfgs.push(n);
        }
    }
    let cfg_manifest = paths::data_dir().join("deployed_cfgs.txt");
    let before = std::fs::read_to_string(&cfg_manifest).unwrap_or_default();
    for n in before.lines().filter(|n| n.starts_with("modo_") && n.ends_with(".cfg") && !n.contains('/')) {
        if !cfgs.iter().any(|c| c == n) && !res.join("cfg").join(n).is_file() {
            let _ = std::fs::remove_file(csgo.join("cfg").join(n));
        }
    }
    std::fs::write(&cfg_manifest, cfgs.join("\n"))?;
    // Valve's Guardian rules are in maps/cfg, out of reach for exec; the plugin runs them from cfg/
    for e in std::fs::read_dir(csgo.join("maps/cfg")).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with("guardian_") && n.ends_with(".cfg") {
            copy_writable(&e.path(), &csgo.join("cfg").join(&n))?;
        }
    }

    let changed = r.translations || !(r.compiled.is_empty() && r.removed.is_empty());
    if reload && changed && crate::rcon::Rcon::from_config(cfg).is_ok() {
        let mut rc = crate::rcon::RconSession::new(cfg);
        let phrases = r.translations.then(|| "sm_reload_translations".to_string());
        let unload = r.compiled.iter().chain(&r.removed).map(|n| format!("sm plugins unload {n}"));
        let load = r.compiled.iter().map(|n| format!("sm plugins load {n}"));
        for c in phrases.into_iter().chain(unload).chain(load).collect::<Vec<_>>() {
            // loading a plugin can take longer than the rcon timeout; loading it again does no harm
            rc.exec_idempotent(&c)?;
        }
        r.reloaded = true;
    }
    Ok(r)
}

/// Names from the previous manifest that are no longer in the user's folder and are not official.
fn gone(before: &str, user: &[String], all: &[Plugin]) -> Vec<String> {
    before
        .lines()
        .filter(|n| !n.is_empty() && !user.iter().any(|u| u == n) && !all.iter().any(|p| p.name == *n))
        .map(str::to_string)
        .collect()
}

/// Changes the value of "key" "value" in a SourceMod KeyValues file.
fn set_kv(txt: &str, key: &str, val: &str) -> String {
    txt.lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with(&format!("\"{key}\"")) {
                let indent = &l[..l.len() - t.len()];
                format!("{indent}\"{key}\"\t\t\"{val}\"")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if txt.ends_with('\n') { "\n" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repo_plugins() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game/plugin");
        let n: Vec<String> = find_plugins(&root).into_iter().map(|p| p.name).collect();
        // only the engine's: modes, skins and the rest come from the hub
        let mut want: Vec<String> = n.clone();
        want.sort();
        assert_eq!(want, ["csls_directconnect", "csls_events", "modos"]);
        assert!(n.iter().all(|p| ENGINE_PLUGINS.contains(&p.as_str())));
        assert!(!n.contains(&"modos_anarquia".into()), "includes are not compiled on their own");
    }

    #[test]
    fn recompiles_when_sources_differ_whatever_their_date() {
        let d = std::env::temp_dir().join(format!("csls-hash-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let sp = d.join("a.sp");
        std::fs::write(&sp, "new").unwrap();
        let smx = d.join("a.smx");
        let p = Plugin { name: "a".into(), main: sp.clone(), dir: d.clone() };
        assert!(needs_compile(&p, &smx), "no .smx");
        std::fs::write(&smx, "x").unwrap();
        assert!(needs_compile(&p, &smx), ".smx of unknown origin");
        std::fs::write(stamp_path(&smx), source_hash(&d)).unwrap();
        assert!(!needs_compile(&p, &smx));
        // same as installing another version whose files have an older date than the .smx
        std::fs::write(&sp, "other").unwrap();
        std::fs::File::options().write(true).open(&sp).unwrap().set_modified(std::time::SystemTime::UNIX_EPOCH).unwrap();
        assert!(needs_compile(&p, &smx));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn copies_only_what_differs() {
        let d = std::env::temp_dir().join(format!("csls-glob-{}", std::process::id()));
        let (from, to) = (d.join("from"), d.join("to"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("a.txt"), "1").unwrap();
        std::fs::write(from.join("b.other"), "1").unwrap();
        assert!(copy_glob(&from, ".txt", &to, str::to_string).unwrap());
        assert!(!copy_glob(&from, ".txt", &to, str::to_string).unwrap());
        assert!(!to.join("b.other").exists());
        std::fs::write(from.join("a.txt"), "2").unwrap();
        assert!(copy_glob(&from, ".txt", &to, str::to_string).unwrap());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn deleted_plugins() {
        let p = |n: &str| Plugin { name: n.into(), main: PathBuf::new(), dir: PathBuf::new() };
        let all = vec![p("modos"), p("mio")];
        assert_eq!(gone("mio\nviejo\nmodos\n", &["mio".into()], &all), vec!["viejo"]);
    }

    #[test]
    fn copy_of_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("csls-copy-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a"), d.join("b"));
        std::fs::write(&a, "1").unwrap();
        std::fs::set_permissions(&a, std::fs::Permissions::from_mode(0o444)).unwrap();
        copy_writable(&a, &b).unwrap();
        copy_writable(&a, &b).unwrap();
        assert_eq!(std::fs::metadata(&b).unwrap().permissions().mode() & 0o777, 0o644);
        std::fs::set_permissions(&a, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn plugin_names() {
        let dir = std::env::temp_dir().join(format!("csls-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x;quit.sp"), "myinfo").unwrap();
        std::fs::write(dir.join("ok.sp"), "myinfo").unwrap();
        let names: Vec<String> = find_plugins(&dir).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["ok"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn kv() {
        let t = "\t\"FollowCSGOServerGuidelines\"\t\"yes\"\n\t\"Otro\" \"1\"\n";
        assert_eq!(set_kv(t, "FollowCSGOServerGuidelines", "no"), "\t\"FollowCSGOServerGuidelines\"\t\t\"no\"\n\t\"Otro\" \"1\"\n");
    }
}
