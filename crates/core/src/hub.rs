//! CSGO-Hub: game modes, plugins and packs that live outside the app, in github.com/xsteban1410/CSGO-Hub.
//! Its CI publishes `index.json` and one zip per item; this module installs them into `paths::hub_dir()/<id>/`.
//!
//! Only what was merged into the hub's main branch is published, but every item is checked again here,
//! because it ends up running on the user's server: allowed paths and file types only, sizes, the SHA-256
//! of the index, valid mode cfgs and plugin names. Plugins come as source and are compiled by `deploy`.
use crate::modes::{CfgDoc, Localized};
use crate::{http, paths};
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const INDEX_URL: &str = "https://xsteban1410.github.io/CSGO-Hub/index.json";
/// Version of the plugin API (`game/plugin/include/csls.inc`, CSLS_API) and of the item format.
pub const API: u32 = 1;

const MAX_INDEX: u64 = 4 * http::MB;
const MAX_ZIP: u64 = 32 * http::MB;
const MAX_UNPACKED: u64 = 64 * http::MB;
const MAX_FILES: usize = 500;
/// Text and images only: never binaries, compiled plugins or scripts.
const ALLOWED: &[&str] = &["toml", "cfg", "sp", "inc", "txt", "md", "png", "jpg", "jpeg", "webp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Mode,
    Plugin,
    /// Only a list of other items in `requires`.
    Pack,
    /// Workshop maps by id; nothing to compile.
    Map,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mode => "mode",
            Kind::Plugin => "plugin",
            Kind::Pack => "pack",
            Kind::Map => "map",
        }
    }
}

/// `item.toml` of an item. The index has the same fields plus where to download it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub version: String,
    pub name: Localized,
    pub description: Localized,
    pub author: String,
    pub license: String,
    pub tags: Vec<String>,
    /// Other items installed with this one, such as the plugin a mode needs.
    pub requires: Vec<String>,
    /// Lowest app version it works with.
    pub min_engine: String,
    pub api: u32,
    /// File name of the preview image inside the item.
    pub preview: String,
    /// Needs SourceMod's FollowCSGOServerGuidelines off (skins giving special knives).
    pub guidelines_off: bool,
    /// Workshop ids, for map items.
    pub workshop: Vec<u64>,
}

/// One entry of `index.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Entry {
    #[serde(flatten)]
    pub item: Item,
    pub archive: String,
    pub sha256: String,
    pub size: u64,
    pub preview_url: String,
    /// It brings SourceMod plugin code.
    pub has_code: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Index {
    pub format: u32,
    pub items: Vec<Entry>,
}

impl Index {
    pub fn get(&self, id: &str) -> crate::Result<&Entry> {
        self.items.iter().find(|e| e.item.id == id).with_context(|| format!("'{id}' is not in the hub"))
    }

    /// `id` and everything it needs, dependencies first, each once.
    pub fn with_requirements(&self, id: &str) -> crate::Result<Vec<&Entry>> {
        fn visit<'a>(ix: &'a Index, id: &str, path: &mut Vec<String>, out: &mut Vec<&'a Entry>) -> crate::Result<()> {
            if out.iter().any(|e| e.item.id == id) {
                return Ok(());
            }
            if path.iter().any(|p| p == id) {
                bail!("the hub items {} require each other", path.join(" -> "));
            }
            let e = ix.get(id)?;
            path.push(id.to_string());
            for r in &e.item.requires {
                visit(ix, r, path, out)?;
            }
            path.pop();
            out.push(e);
            Ok(())
        }
        let mut out = Vec::new();
        visit(self, id, &mut Vec::new(), &mut out)?;
        Ok(out)
    }
}

pub fn index_url() -> String {
    std::env::var("CSLS_HUB_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| INDEX_URL.to_string())
}

pub fn parse_index(json: &str) -> crate::Result<Index> {
    let ix: Index = serde_json::from_str(json).context("the hub index is not valid")?;
    if ix.format != API {
        bail!("the hub uses format {} and this app reads format {API}: update the app", ix.format);
    }
    Ok(ix)
}

pub fn fetch_index() -> crate::Result<Index> {
    parse_index(&http::text(&[index_url()], MAX_INDEX)?)
}

/// Ids are folder names and part of `sm plugins` and cfg names: lowercase letters, digits, `_` and `-`.
pub fn valid_item_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 48 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Whether this app can run the item.
pub fn compatible(it: &Item) -> crate::Result<()> {
    if it.api != API {
        bail!("{} is made for plugin API {} and this app has API {API}", it.id, it.api);
    }
    if !it.min_engine.is_empty() && crate::updates::is_newer(&it.min_engine, env!("CARGO_PKG_VERSION")) {
        bail!("{} needs app version {} or newer", it.id, it.min_engine);
    }
    Ok(())
}

// ---------- installed items ----------

/// An installed item: its folder and its item.toml.
#[derive(Debug, Clone)]
pub struct Installed {
    pub dir: PathBuf,
    pub item: Item,
}

impl Installed {
    pub fn plugin_dir(&self) -> PathBuf {
        self.dir.join("plugin")
    }
    pub fn modes_dir(&self) -> PathBuf {
        self.dir.join("modes")
    }
    pub fn cfg_dir(&self) -> PathBuf {
        self.dir.join("cfg")
    }
    pub fn data_dir(&self) -> PathBuf {
        self.dir.join("data")
    }
}

fn read_item(dir: &Path) -> crate::Result<Item> {
    let p = dir.join("item.toml");
    let t = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    toml::from_str(&t).with_context(|| format!("invalid {}", p.display()))
}

/// Installed items, sorted by id. A broken folder is skipped: one bad item must not take the others down.
pub fn installed() -> Vec<Installed> {
    installed_in(&paths::hub_dir())
}

pub fn installed_in(root: &Path) -> Vec<Installed> {
    let mut v: Vec<Installed> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(valid_item_id))
        .filter_map(|e| {
            read_item(&e.path()).ok().filter(|it| e.file_name() == it.id.as_str()).map(|item| Installed { dir: e.path(), item })
        })
        .collect();
    v.sort_by(|a, b| a.item.id.cmp(&b.item.id));
    v
}

/// Installed items with a newer version in the index.
pub fn updates(ix: &Index) -> Vec<String> {
    installed()
        .into_iter()
        .filter(|i| ix.get(&i.item.id).is_ok_and(|e| crate::updates::is_newer(&e.item.version, &i.item.version)))
        .map(|i| i.item.id)
        .collect()
}

// ---------- checking an item folder (also what the hub's CI runs) ----------

/// Checks an item folder: item.toml, file names and types, mode cfgs and plugins. Returns the item.
pub fn check_dir(dir: &Path) -> crate::Result<Item> {
    let it = read_item(dir)?;
    if !valid_item_id(&it.id) {
        bail!("invalid item id '{}': lowercase letters, digits, - and _", it.id);
    }
    if dir.file_name().and_then(|n| n.to_str()) != Some(it.id.as_str()) {
        bail!("the folder of {} must be called {}", dir.display(), it.id);
    }
    if it.version.is_empty() || it.name.en.is_empty() || it.description.en.is_empty() {
        bail!("{}: version, name.en and description.en are required", it.id);
    }
    if let Some(r) = it.requires.iter().find(|r| !valid_item_id(r) || **r == it.id) {
        bail!("{}: invalid requirement '{r}'", it.id);
    }
    let mut total = 0u64;
    let mut count = 0usize;
    for f in walk(dir)? {
        let rel = f.strip_prefix(dir).unwrap_or(&f);
        check_member(&rel.to_string_lossy())?;
        total += std::fs::metadata(&f)?.len();
        count += 1;
    }
    if total > MAX_UNPACKED || count > MAX_FILES {
        bail!("{} is too large: {count} files, {} MB", it.id, total / http::MB);
    }
    if !it.preview.is_empty() {
        let p = dir.join(&it.preview);
        check_member(&it.preview)?;
        let size = std::fs::metadata(&p).with_context(|| format!("{}: the preview {} is missing", it.id, it.preview))?.len();
        if size > 300 * http::KB {
            bail!("{}: the preview is larger than 300 KB", it.id);
        }
    }
    let inst = Installed { dir: dir.to_path_buf(), item: it.clone() };
    for (id, cfg) in mode_files(&inst) {
        if !crate::modes::valid_id(&id) {
            bail!("{}: invalid mode id '{id}'", it.id);
        }
        let txt = std::fs::read_to_string(&cfg)?;
        check_cfg(&txt).with_context(|| format!("{}: cfg/modo_{id}.cfg", it.id))?;
        let meta = inst.modes_dir().join(format!("{id}.toml"));
        if meta.is_file() {
            let m: crate::modes::ModeMeta =
                toml::from_str(&std::fs::read_to_string(&meta)?).with_context(|| format!("{}: modes/{id}.toml", it.id))?;
            if let Some(map) = m.default_map.as_deref().filter(|m| !crate::launch::valid_map_name(m)) {
                bail!("{}: invalid default_map '{map}'", it.id);
            }
        }
    }
    for p in crate::deploy::find_plugins(&inst.plugin_dir()) {
        if crate::deploy::ENGINE_PLUGINS.contains(&p.name.as_str()) {
            bail!("{}: the plugin {} would replace one of the engine", it.id, p.name);
        }
    }
    Ok(it)
}

/// A mode cfg from the hub: the same rules as an imported mode, line by line, so the file that reaches the
/// server is exactly what was checked.
fn check_cfg(txt: &str) -> crate::Result<()> {
    for l in txt.lines() {
        let l = l.trim();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        CfgDoc::parse(l).validate()?;
    }
    Ok(())
}

/// A path inside an item or its zip: plain names, an allowed type, and nothing hidden.
fn check_member(name: &str) -> crate::Result<()> {
    let ok = !name.is_empty()
        && name
            .split('/')
            .all(|c| !c.is_empty() && !c.starts_with('.') && c.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)));
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if !ok || !ALLOWED.contains(&ext.as_str()) {
        bail!("file not allowed in a hub item: {name}");
    }
    Ok(())
}

fn walk(dir: &Path) -> crate::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let t = e.file_type()?;
        if t.is_symlink() {
            bail!("links are not allowed in a hub item: {}", e.path().display());
        } else if t.is_dir() {
            out.extend(walk(&e.path())?);
        } else {
            out.push(e.path());
        }
    }
    Ok(out)
}

/// (mode id, cfg path) of each mode in an item.
pub fn mode_files(i: &Installed) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(i.cfg_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_str()?.to_string();
            let id = n.strip_prefix("modo_")?.strip_suffix(".cfg")?.to_string();
            Some((id, e.path()))
        })
        .collect();
    v.sort();
    v
}

// ---------- install, update, remove ----------

/// Unpacks a zip into `to` with the same rules as `check_member`, and limits on files and size.
fn unpack(zip_path: &Path, to: &Path) -> crate::Result<()> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?).context("the item is not a valid zip")?;
    if z.len() > MAX_FILES {
        bail!("the item has too many files");
    }
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        let name = f.name()?.into_owned();
        if f.is_dir() {
            continue;
        }
        if f.is_symlink() {
            bail!("links are not allowed in a hub item: {name}");
        }
        check_member(&name)?;
        let dest = to.join(&name);
        std::fs::create_dir_all(dest.parent().unwrap_or(to))?;
        let mut buf = Vec::new();
        (&mut f).take(MAX_UNPACKED - total + 1).read_to_end(&mut buf)?;
        total += buf.len() as u64;
        if total > MAX_UNPACKED {
            bail!("the item is larger than {} MB unpacked", MAX_UNPACKED / http::MB);
        }
        std::fs::write(dest, buf)?;
    }
    Ok(())
}

/// Installs one entry's zip, already downloaded, as `root/<id>`. The old version stays until the new one is checked.
fn install_zip(root: &Path, e: &Entry, zip_path: &Path) -> crate::Result<()> {
    let id = &e.item.id;
    if !valid_item_id(id) {
        bail!("invalid item id '{id}'");
    }
    compatible(&e.item)?;
    if crate::install::sha256(zip_path)? != e.sha256 {
        bail!("{id} does not match its checksum in the hub index: not installing it");
    }
    // unpacked under a folder named after the id, so check_dir sees it as it will be installed
    let stage = root.join(".staging");
    let new = stage.join(id);
    let old = root.join(format!(".{id}.old"));
    let _ = std::fs::remove_dir_all(&new);
    let _ = std::fs::remove_dir_all(&old);
    std::fs::create_dir_all(&new)?;
    let checked = unpack(zip_path, &new).and_then(|()| check_dir(&new));
    match checked {
        Ok(it) if it.id == *id => {}
        Ok(it) => {
            let _ = std::fs::remove_dir_all(&new);
            bail!("the zip of {id} holds the item {}", it.id);
        }
        Err(err) => {
            let _ = std::fs::remove_dir_all(&new);
            return Err(err);
        }
    }
    let dest = root.join(id);
    if dest.exists() {
        std::fs::rename(&dest, &old)?;
    }
    std::fs::rename(&new, &dest)?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Installs `id` and what it requires from the index. Returns the ids installed or updated.
pub fn install(ix: &Index, id: &str) -> crate::Result<Vec<String>> {
    install_into(&paths::hub_dir(), ix, id, false)
}

/// Updates every installed item with a newer version.
pub fn update_all(ix: &Index) -> crate::Result<Vec<String>> {
    let mut done = Vec::new();
    for id in updates(ix) {
        done.extend(install_into(&paths::hub_dir(), ix, &id, true)?);
    }
    Ok(done)
}

fn install_into(root: &Path, ix: &Index, id: &str, update: bool) -> crate::Result<Vec<String>> {
    std::fs::create_dir_all(root)?;
    let have: BTreeMap<String, String> = installed_in(root).into_iter().map(|i| (i.item.id, i.item.version)).collect();
    let mut done = Vec::new();
    for e in ix.with_requirements(id)? {
        let current = have.get(&e.item.id);
        let wanted = e.item.id == id && (update || current.is_none());
        if current.is_some_and(|v| !crate::updates::is_newer(&e.item.version, v)) || (current.is_some() && !wanted && !update) {
            continue;
        }
        let dl = root.join(format!(".{}.zip", e.item.id));
        let r = http::download(&e.archive, &dl, MAX_ZIP, 300, Some(e.size)).and_then(|()| install_zip(root, e, &dl));
        let _ = std::fs::remove_file(&dl);
        r.with_context(|| format!("installing {}", e.item.id))?;
        done.push(e.item.id.clone());
    }
    Ok(done)
}

/// Installed items that need `id`.
pub fn dependents(id: &str) -> Vec<String> {
    installed().into_iter().filter(|i| i.item.requires.iter().any(|r| r == id)).map(|i| i.item.id).collect()
}

/// Removes an item. Refuses while another installed item needs it. The next deploy removes its plugins and cfgs.
pub fn remove(id: &str) -> crate::Result<()> {
    remove_in(&paths::hub_dir(), id)
}

fn remove_in(root: &Path, id: &str) -> crate::Result<()> {
    if !valid_item_id(id) {
        bail!("invalid item id '{id}'");
    }
    let users: Vec<String> =
        installed_in(root).into_iter().filter(|i| i.item.requires.iter().any(|r| r == id)).map(|i| i.item.id).collect();
    if !users.is_empty() {
        bail!("{id} is needed by {}: remove those first", users.join(", "));
    }
    let dir = root.join(id);
    if !dir.join("item.toml").is_file() {
        bail!("{id} is not installed");
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

/// Cached preview image of an entry.
pub fn preview(e: &Entry) -> crate::Result<PathBuf> {
    if e.preview_url.is_empty() || !valid_item_id(&e.item.id) {
        bail!("no preview");
    }
    let dir = paths::data_dir().join("hub-cache");
    let ext = e.preview_url.rsplit('.').next().filter(|x| ["png", "jpg", "jpeg", "webp"].contains(x)).unwrap_or("png");
    let p = dir.join(format!("{}-{}.{ext}", e.item.id, e.item.version.replace(|c: char| !c.is_ascii_alphanumeric(), "_")));
    if !p.is_file() {
        std::fs::create_dir_all(&dir)?;
        http::download(&e.preview_url, &p, 2 * http::MB, 30, None)?;
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("csls-hub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn item_dir(root: &Path, id: &str, extra: &[(&str, &str)]) -> PathBuf {
        let d = root.join(id);
        std::fs::create_dir_all(d.join("cfg")).unwrap();
        std::fs::write(
            d.join("item.toml"),
            format!(
                "id = \"{id}\"\nkind = \"mode\"\nversion = \"1.0.0\"\napi = 1\nname = {{ en = \"X\" }}\ndescription = {{ en = \"Y\" }}\n"
            ),
        )
        .unwrap();
        for (p, t) in extra {
            let f = d.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
        d
    }

    #[test]
    fn checks_members() {
        for ok in ["item.toml", "cfg/modo_x.cfg", "plugin/a/a.sp", "plugin/a/translations/a.phrases.txt", "preview.png"] {
            assert!(check_member(ok).is_ok(), "{ok}");
        }
        for bad in ["/etc/passwd", "../x.cfg", "a/../b.cfg", "x.smx", "x.so", "run.sh", ".hidden.cfg", "a b.cfg", "noext"] {
            assert!(check_member(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn checks_item_folders() {
        let root = tmp("check");
        let good = item_dir(&root, "awp", &[("cfg/modo_awp.cfg", "// AWP only\nmodo_t_arma weapon_awp\n")]);
        assert_eq!(check_dir(&good).unwrap().id, "awp");
        let bad = item_dir(&root, "evil", &[("cfg/modo_evil.cfg", "sv_cheats 1\nrcon_password x\n")]);
        assert!(check_dir(&bad).is_err());
        let chain = item_dir(&root, "chain", &[("cfg/modo_chain.cfg", "mp_maxrounds 5; quit\n")]);
        assert!(check_dir(&chain).is_err());
        let bin = item_dir(&root, "bin", &[("plugin/x/x.smx", "FFPS")]);
        assert!(check_dir(&bin).is_err());
        let engine = item_dir(&root, "fake", &[("plugin/modos/modos.sp", "myinfo")]);
        assert!(check_dir(&engine).is_err(), "cannot replace an engine plugin");
        let _ = std::fs::remove_dir_all(root);
    }

    fn zip_of(dir: &Path, to: &Path) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(to).unwrap());
        for f in walk(dir).unwrap() {
            let rel = f.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
            w.start_file(rel, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut w, &std::fs::read(&f).unwrap()).unwrap();
        }
        w.finish().unwrap();
    }

    fn entry(src: &Path, id: &str, version: &str, requires: &[&str]) -> Entry {
        let d = item_dir(src, id, &[(&format!("cfg/modo_{id}.cfg"), "mp_maxrounds 5\n")]);
        let mut it = read_item(&d).unwrap();
        it.version = version.into();
        it.requires = requires.iter().map(|s| s.to_string()).collect();
        std::fs::write(d.join("item.toml"), toml::to_string(&it).unwrap()).unwrap();
        let z = src.join(format!("{id}-{version}.zip"));
        zip_of(&d, &z);
        Entry {
            item: it,
            archive: format!("file://{}", z.display()),
            sha256: crate::install::sha256(&z).unwrap(),
            size: std::fs::metadata(&z).unwrap().len(),
            ..Default::default()
        }
    }

    #[test]
    fn installs_with_requirements_updates_and_removes() {
        let src = tmp("src");
        let root = tmp("root");
        let ix = Index { format: API, items: vec![entry(&src, "lib", "1.0.0", &[]), entry(&src, "top", "1.0.0", &["lib"])] };
        assert_eq!(install_into(&root, &ix, "top", false).unwrap(), ["lib", "top"]);
        assert_eq!(installed_in(&root).len(), 2);
        assert!(install_into(&root, &ix, "top", false).unwrap().is_empty(), "nothing new");
        assert!(remove_in(&root, "lib").is_err(), "top needs it");

        let mut bad = entry(&src, "lib", "2.0.0", &[]);
        bad.sha256 = "0".repeat(64);
        let ix2 = Index { format: API, items: vec![bad, ix.items[1].clone()] };
        assert!(install_into(&root, &ix2, "lib", true).is_err());
        assert_eq!(installed_in(&root)[0].item.version, "1.0.0", "a failed update keeps the old version");

        let ix3 = Index { format: API, items: vec![entry(&src, "lib", "2.0.0", &[]), ix.items[1].clone()] };
        assert_eq!(install_into(&root, &ix3, "lib", true).unwrap(), ["lib"]);
        assert_eq!(installed_in(&root)[0].item.version, "2.0.0");
        remove_in(&root, "top").unwrap();
        remove_in(&root, "lib").unwrap();
        assert!(installed_in(&root).is_empty());
        let _ = std::fs::remove_dir_all(src);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn requirement_cycles_and_formats() {
        let mut a = Entry::default();
        a.item.id = "a".into();
        a.item.requires = vec!["b".into()];
        let mut b = Entry::default();
        b.item.id = "b".into();
        b.item.requires = vec!["a".into()];
        assert!(Index { format: API, items: vec![a, b] }.with_requirements("a").is_err());
        assert!(parse_index(r#"{"format": 99, "items": []}"#).is_err());
        let mut it = Item { id: "x".into(), api: API, min_engine: "999.0.0".into(), ..Default::default() };
        assert!(compatible(&it).is_err());
        it.min_engine = "0.1.0".into();
        assert!(compatible(&it).is_ok());
    }
}
