//! Data only the app can extract for plugins. An item asks for it with `needs = ["nav"]` in its item.toml; on deploy the app
//! writes it to `sourcemod/data/csls_export/<name>/<map>.<ext>` (see `CSLS_ExportPath` in csls.inc), only when its source changed.
//! A new exporter is one function and one line of `EXPORTERS`.

use anyhow::Context;
use std::path::Path;

pub mod nav;

/// Where the files go, inside SourceMod's folder: `csls.inc` has the same path for plugins.
pub const OUT_DIR: &str = "data/csls_export";

/// What an exporter reads and writes. `run` gets the server's `maps` folder and the map name, and returns the file to
/// write, or None when the map has no source for it.
pub struct Exporter {
    pub name: &'static str,
    /// File extension of the output.
    pub ext: &'static str,
    /// File of the map it is built from, such as "nav" for `maps/<map>.nav`.
    pub source_ext: &'static str,
    pub run: fn(&[u8]) -> crate::Result<String>,
}

pub const EXPORTERS: &[Exporter] = &[Exporter { name: "nav", ext: "txt", source_ext: "nav", run: nav::export }];

pub fn known(name: &str) -> bool {
    EXPORTERS.iter().any(|e| e.name == name)
}

pub fn names() -> Vec<&'static str> {
    EXPORTERS.iter().map(|e| e.name).collect()
}

/// FNV-1a: stable across Rust versions, which a stamp kept on disk needs.
fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for x in b {
        h ^= u64::from(*x);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Runs the exporters in `needed` for every map in `maps_dir` that has a source. Returns "name/map" for what it wrote.
/// A map whose source did not change since the last time is skipped; a broken source is skipped with the rest going on.
pub fn export_for(needed: &[String], maps_dir: &Path, out_dir: &Path) -> crate::Result<Vec<String>> {
    let mut wrote = Vec::new();
    // exports nobody asks for any more are removed
    for e in std::fs::read_dir(out_dir).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if known(&n) && !needed.contains(&n) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    for ex in EXPORTERS.iter().filter(|e| needed.iter().any(|n| n == e.name)) {
        let dir = out_dir.join(ex.name);
        for f in std::fs::read_dir(maps_dir).into_iter().flatten().flatten() {
            let fname = f.file_name().to_string_lossy().into_owned();
            let Some(map) = fname.strip_suffix(&format!(".{}", ex.source_ext)) else { continue };
            if !crate::launch::valid_map_name(map) {
                continue;
            }
            let bytes = std::fs::read(f.path()).with_context(|| format!("reading {}", f.path().display()))?;
            let stamp = dir.join(format!("{map}.stamp"));
            let want = format!("{} {}", fnv(&bytes), ex.name);
            if std::fs::read_to_string(&stamp).ok().as_deref() == Some(want.as_str()) {
                continue;
            }
            let Ok(text) = (ex.run)(&bytes) else { continue };
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join(format!("{map}.{}", ex.ext)), text)?;
            std::fs::write(stamp, want)?;
            wrote.push(format!("{}/{map}", ex.name));
        }
    }
    Ok(wrote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        assert!(known("nav") && !known("nope"));
        let mut n = names();
        n.sort_unstable();
        n.dedup();
        assert_eq!(n.len(), EXPORTERS.len(), "names are unique");
    }

    #[test]
    fn skips_unchanged_and_broken_sources() {
        let d = std::env::temp_dir().join(format!("csls-exports-{}", std::process::id()));
        let (maps, out) = (d.join("maps"), d.join("out"));
        std::fs::create_dir_all(&maps).unwrap();
        std::fs::write(maps.join("de_bad.nav"), b"not a nav").unwrap();
        std::fs::write(maps.join("de_ok.nav"), nav::tests::tiny()).unwrap();
        let need = vec!["nav".to_string()];
        let w = export_for(&need, &maps, &out).unwrap();
        assert_eq!(w, vec!["nav/de_ok".to_string()], "the broken one is skipped");
        assert!(export_for(&need, &maps, &out).unwrap().is_empty(), "nothing changed");
        assert!(export_for(&[], &maps, &out).unwrap().is_empty());
        assert!(!out.join("nav").exists(), "unrequested exports are removed");
        std::fs::remove_dir_all(d).unwrap();
    }
}
