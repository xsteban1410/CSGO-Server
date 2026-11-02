//! Workshop de la app 730 (CS:GO y CS2 comparten app). Sin clave de la Web API:
//! búsqueda con la página pública del workshop y detalles con GetPublishedFileDetails (no pide clave).
//! Los mapas de CS2 son Source 2 (.vpk) y el motor de CS:GO Legacy no los carga: se marcan incompatibles.
use crate::paths;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Carpeta bajo maps/ donde se instalan. No se usa maps/workshop/ porque el motor intenta revalidar
/// esos mapas con Steam (sin clave falla) y solo cargan la primera vez.
pub const MAPS_SUBDIR: &str = "csls";

const DETAILS: &str = "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
const COLLECTION: &str = "https://api.steampowered.com/ISteamRemoteStorage/GetCollectionDetails/v1/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compat {
    /// Mapa de CS:GO (.bsp): se puede instalar.
    Csgo,
    /// Solo CS2: no funciona en CS:GO Legacy.
    Cs2,
    /// No es un mapa o no se sabe.
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub id: u64,
    pub title: String,
    /// Ruta original (mymaps/de_x.bsp); vacía en los de CS2.
    pub filename: String,
    pub file_url: String,
    pub file_size: u64,
    pub preview_url: String,
    pub tags: Vec<String>,
    pub subscriptions: u64,
    pub time_updated: u64,
    pub collection: bool,
}

impl Item {
    pub fn compat(&self) -> Compat {
        let f = self.filename.to_ascii_lowercase();
        if f.ends_with(".bsp") {
            Compat::Csgo
        } else if f.ends_with(".vpk") || self.tags.iter().any(|t| t.eq_ignore_ascii_case("cs2")) {
            Compat::Cs2
        } else {
            Compat::Unknown
        }
    }

    /// Nombre del mapa sin carpeta ni .bsp.
    pub fn map_name(&self) -> Option<String> {
        let f = self.filename.rsplit('/').next()?;
        let n = f.strip_suffix(".bsp").or_else(|| f.strip_suffix(".BSP"))?;
        crate::launch::valid_map_name(n).then(|| n.to_string())
    }
}

fn curl(args: &[String]) -> crate::Result<String> {
    let o = Command::new("curl").args(["-fsSL", "--max-time", "30"]).args(args).output().context("ejecutando curl")?;
    if !o.status.success() {
        bail!("error de red: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn num(v: &serde_json::Value) -> u64 {
    v.as_u64().or_else(|| v.as_str()?.parse().ok()).unwrap_or(0)
}

pub fn parse_details(json: &str) -> crate::Result<Vec<Item>> {
    let v: serde_json::Value = serde_json::from_str(json).context("respuesta del workshop inválida")?;
    let list = v["response"]["publishedfiledetails"].as_array().cloned().unwrap_or_default();
    Ok(list
        .into_iter()
        .filter(|x| x["result"].as_i64() == Some(1))
        .map(|x| Item {
            id: num(&x["publishedfileid"]),
            title: x["title"].as_str().unwrap_or("").to_string(),
            filename: x["filename"].as_str().unwrap_or("").to_string(),
            file_url: x["file_url"].as_str().unwrap_or("").to_string(),
            file_size: num(&x["file_size"]),
            preview_url: x["preview_url"].as_str().unwrap_or("").to_string(),
            tags: x["tags"].as_array().map(|t| t.iter().filter_map(|t| t["tag"].as_str().map(str::to_string)).collect()).unwrap_or_default(),
            subscriptions: num(&x["lifetime_subscriptions"]),
            time_updated: num(&x["time_updated"]),
            // file_type 2 = colección
            collection: num(&x["file_type"]) == 2,
        })
        .collect())
}

/// Detalles de varios ítems (sin clave).
pub fn details(ids: &[u64]) -> crate::Result<Vec<Item>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut a = vec!["-d".to_string(), format!("itemcount={}", ids.len())];
    for (i, id) in ids.iter().enumerate() {
        a.push("-d".into());
        a.push(format!("publishedfileids[{i}]={id}"));
    }
    a.push(DETAILS.into());
    parse_details(&curl(&a)?)
}

/// Ítems de una colección.
pub fn collection(id: u64) -> crate::Result<Vec<u64>> {
    let j = curl(&["-d".into(), "collectioncount=1".into(), "-d".into(), format!("publishedfileids[0]={id}"), COLLECTION.into()])?;
    let v: serde_json::Value = serde_json::from_str(&j)?;
    Ok(v["response"]["collectiondetails"][0]["children"]
        .as_array()
        .map(|c| c.iter().map(|x| num(&x["publishedfileid"])).filter(|n| *n > 0).collect())
        .unwrap_or_default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Trend,
    Popular,
    Recent,
    Text,
}

/// IDs de la página pública del workshop (sin clave).
// shortcut: lee el HTML de steamcommunity.com; si Valve cambia la página, usar IPublishedFileService/QueryFiles con clave.
pub fn search_ids(text: &str, tag: Option<&str>, sort: Sort, page: u32) -> crate::Result<Vec<u64>> {
    let sort = match (sort, text.trim().is_empty()) {
        (_, false) | (Sort::Text, _) => "textsearch",
        (Sort::Trend, _) => "trend",
        (Sort::Popular, _) => "totaluniquesubscribers",
        (Sort::Recent, _) => "mostrecent",
    };
    let mut url = format!(
        "https://steamcommunity.com/workshop/browse/?appid=730&section=readytouseitems&excludedtags%5B%5D=Cs2&browsesort={sort}&actualsort={sort}&p={}",
        page.max(1)
    );
    if !text.trim().is_empty() {
        url += &format!("&searchtext={}", urlencode(text.trim()));
    }
    if let Some(t) = tag {
        url += &format!("&requiredtags%5B%5D={}", urlencode(t));
    }
    Ok(parse_browse(&curl(&[url])?))
}

pub fn parse_browse(html: &str) -> Vec<u64> {
    let mut v = Vec::new();
    for part in html.split("filedetails/?id=").skip(1) {
        let n: String = part.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(id) = n.parse::<u64>() {
            if !v.contains(&id) {
                v.push(id);
            }
        }
    }
    v
}

pub fn search(text: &str, tag: Option<&str>, sort: Sort, page: u32) -> crate::Result<Vec<Item>> {
    details(&search_ids(text, tag, sort, page)?)
}

// ---------- biblioteca local ----------

fn cache_dir() -> PathBuf {
    paths::data_dir().join("workshop")
}

/// Miniatura en caché (la descarga si falta).
pub fn thumbnail(it: &Item) -> crate::Result<PathBuf> {
    let p = cache_dir().join(format!("{}.jpg", it.id));
    if !p.is_file() {
        if it.preview_url.is_empty() {
            bail!("sin miniatura");
        }
        std::fs::create_dir_all(cache_dir())?;
        let tmp = p.with_extension("part");
        curl(&["-o".into(), tmp.to_string_lossy().into(), it.preview_url.clone()])?;
        std::fs::rename(tmp, &p)?;
    }
    Ok(p)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    /// Carpeta bajo maps/csls (id del workshop o "local").
    pub dir: String,
    /// Nombre para changelevel: csls/<dir>/<mapa>.
    pub map: String,
    pub title: String,
    pub has_nav: bool,
    pub item: Option<Item>,
}

pub fn installed(maps_dir: &Path) -> Vec<Installed> {
    let mut v = Vec::new();
    let base = maps_dir.join(MAPS_SUBDIR);
    for d in std::fs::read_dir(&base).into_iter().flatten().flatten() {
        let dir = d.file_name().to_string_lossy().into_owned();
        let item: Option<Item> = std::fs::read_to_string(cache_dir().join(format!("{dir}.json"))).ok().and_then(|t| serde_json::from_str(&t).ok());
        for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().into_owned();
            let Some(stem) = n.strip_suffix(".bsp") else { continue };
            v.push(Installed {
                map: format!("{MAPS_SUBDIR}/{dir}/{stem}"),
                title: item.as_ref().map(|i| i.title.clone()).unwrap_or_else(|| stem.to_string()),
                has_nav: d.path().join(format!("{stem}.nav")).is_file(),
                dir: dir.clone(),
                item: item.clone(),
            });
        }
    }
    v.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    v
}

/// Instala un mapa de CS:GO del workshop. Devuelve el nombre para changelevel.
pub fn install(maps_dir: &Path, it: &Item) -> crate::Result<String> {
    match it.compat() {
        Compat::Cs2 => bail!("«{}» es un mapa de CS2 (Source 2): CS:GO Legacy no puede cargarlo", it.title),
        Compat::Unknown => bail!("«{}» no es un mapa de CS:GO (.bsp)", it.title),
        Compat::Csgo => {}
    }
    let name = it.map_name().context("nombre de mapa inválido")?;
    let dir = maps_dir.join(MAPS_SUBDIR).join(it.id.to_string());
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(format!("{name}.bsp"));
    let tmp = dest.with_extension("bsp.part");
    if !it.file_url.is_empty() {
        curl(&["-o".into(), tmp.to_string_lossy().into(), it.file_url.clone()])?;
    } else {
        // contenido en SteamPipe: lo baja SteamCMD
        let sc = crate::install::which("steamcmd").unwrap_or_else(|| paths::data_dir().join("steamcmd/steamcmd.sh"));
        let root = paths::data_dir().join("steamcmd-workshop");
        let st = Command::new(&sc)
            .arg("+force_install_dir")
            .arg(&root)
            .args(["+login", "anonymous", "+workshop_download_item", "730", &it.id.to_string(), "+quit"])
            .status()
            .context("ejecutando SteamCMD")?;
        let got = root.join(format!("steamapps/workshop/content/730/{}", it.id));
        let bsp = std::fs::read_dir(&got).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "bsp"));
        match bsp {
            Some(b) if st.success() => {
                std::fs::copy(b, &tmp)?;
            }
            _ => bail!("SteamCMD no pudo bajar el ítem {} (prueba a cargarlo con el servidor encendido)", it.id),
        }
    }
    // los mapas antiguos del workshop vienen dentro de un .zip
    if magic(&tmp) == *b"PK\x03\x04" {
        let r = unzip_bsp(&tmp, &dest.with_extension("bsp.unzip"));
        let _ = std::fs::remove_file(&tmp);
        std::fs::rename(r?, &tmp)?;
    }
    // un .bsp empieza por "VBSP"
    if magic(&tmp) != *b"VBSP" {
        let _ = std::fs::remove_file(&tmp);
        bail!("lo descargado no es un mapa .bsp");
    }
    std::fs::rename(&tmp, &dest)?;
    std::fs::create_dir_all(cache_dir())?;
    std::fs::write(cache_dir().join(format!("{}.json", it.id)), serde_json::to_string(it)?)?;
    Ok(format!("{MAPS_SUBDIR}/{}/{name}", it.id))
}

fn magic(p: &Path) -> [u8; 4] {
    use std::io::Read;
    let mut h = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open(p) {
        let _ = f.read_exact(&mut h);
    }
    h
}

/// Saca el primer .bsp de un zip (sin rutas: solo el contenido). Límite de 1 GB por si el zip es malicioso.
fn unzip_bsp(zip_path: &Path, out: &Path) -> crate::Result<PathBuf> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?).context("zip inválido")?;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        if !f.name().map(|n| n.to_ascii_lowercase().ends_with(".bsp")).unwrap_or(false) {
            continue;
        }
        let mut o = std::fs::File::create(out)?;
        use std::io::Read;
        let n = std::io::copy(&mut (&mut f).take(1 << 30), &mut o)?;
        if n >= 1 << 30 {
            let _ = std::fs::remove_file(out);
            bail!("el .bsp del zip es demasiado grande");
        }
        return Ok(out.to_path_buf());
    }
    bail!("el zip no contiene ningún .bsp")
}

pub fn remove(maps_dir: &Path, dir: &str) -> crate::Result<()> {
    if dir.is_empty() || !dir.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        bail!("carpeta inválida");
    }
    let p = maps_dir.join(MAPS_SUBDIR).join(dir);
    if p.is_dir() {
        std::fs::remove_dir_all(p)?;
    }
    let _ = std::fs::remove_file(cache_dir().join(format!("{dir}.json")));
    Ok(())
}

/// Copia un .bsp local a maps/workshop/local/. Devuelve el nombre para changelevel.
pub fn import_local(maps_dir: &Path, bsp: &Path) -> crate::Result<String> {
    let stem = bsp.file_stem().and_then(|s| s.to_str()).context("archivo sin nombre")?;
    if !crate::launch::valid_map_name(stem) || stem.contains('/') {
        bail!("nombre de mapa inválido '{stem}'");
    }
    let mut head = [0u8; 4];
    use std::io::Read;
    std::fs::File::open(bsp)?.read_exact(&mut head)?;
    if &head != b"VBSP" {
        bail!("{} no es un mapa .bsp de Source", bsp.display());
    }
    let dir = maps_dir.join(MAPS_SUBDIR).join("local");
    std::fs::create_dir_all(&dir)?;
    crate::deploy::copy_writable(bsp, &dir.join(format!("{stem}.bsp")))?;
    // si viene con su .nav al lado, también
    let nav = bsp.with_extension("nav");
    if nav.is_file() {
        crate::deploy::copy_writable(&nav, &dir.join(format!("{stem}.nav")))?;
    }
    Ok(format!("{MAPS_SUBDIR}/local/{stem}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const J: &str = r#"{"response":{"result":1,"resultcount":2,"publishedfiledetails":[
      {"publishedfileid":"3785985069","result":1,"filename":"","file_url":"","file_size":"121822375","preview_url":"p","title":"VIP | Mirage","tags":[{"tag":"Cs2"},{"tag":"Map"}],"lifetime_subscriptions":2292,"time_updated":1793215152},
      {"publishedfileid":"125438255","result":1,"filename":"mymaps/de_dust2.bsp","file_url":"https://x/y","file_size":"1444042","preview_url":"q","title":"Dust 2","tags":[{"tag":"Classic"}],"lifetime_subscriptions":1515585},
      {"publishedfileid":"1","result":9}]}}"#;

    #[test]
    fn detalles_y_compat() {
        let v = parse_details(J).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].compat(), Compat::Cs2);
        assert_eq!(v[1].compat(), Compat::Csgo);
        assert_eq!(v[1].map_name().as_deref(), Some("de_dust2"));
        assert_eq!(v[1].file_size, 1444042);
    }

    #[test]
    fn navegador() {
        let h = "<a href=\"https://steamcommunity.com/sharedfiles/filedetails/?id=123&searchtext=\">x</a> filedetails/?id=456\" filedetails/?id=123";
        assert_eq!(parse_browse(h), vec![123, 456]);
        assert_eq!(urlencode("aim map"), "aim%20map");
    }

    #[test]
    fn importar_local_y_borrar() {
        let d = std::env::temp_dir().join(format!("csls-ws-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let bsp = d.join("aim_test.bsp");
        std::fs::write(&bsp, b"VBSP....").unwrap();
        let maps = d.join("maps");
        assert_eq!(import_local(&maps, &bsp).unwrap(), "csls/local/aim_test");
        let i = installed(&maps);
        assert_eq!(i.len(), 1);
        assert!(!i[0].has_nav);
        std::fs::write(d.join("malo.bsp"), b"nope").unwrap();
        assert!(import_local(&maps, &d.join("malo.bsp")).is_err());
        remove(&maps, "local").unwrap();
        assert!(installed(&maps).is_empty());
        assert!(remove(&maps, "../x").is_err());
        std::fs::remove_dir_all(d).ok();
    }
}
