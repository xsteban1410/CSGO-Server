//! Is there a newer release? Only when the user asks, or with the option on. Nothing is installed without permission.
use anyhow::{bail, Context};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const LATEST_URL: &str = "https://api.github.com/repos/xsteban1410/CSGO-Server/releases/latest";

#[derive(Debug, Clone, PartialEq)]
pub enum Channel {
    AppImage(PathBuf),
    Flatpak,
    Deb,
    Pacman,
    Other,
}

pub fn channel() -> Channel {
    if let Some(p) = crate::server::appimage() {
        return Channel::AppImage(p);
    }
    if std::env::var_os("FLATPAK_ID").is_some() {
        return Channel::Flatpak;
    }
    if Path::new("/etc/arch-release").exists() {
        return Channel::Pacman;
    }
    if Path::new("/etc/debian_version").exists() {
        return Channel::Deb;
    }
    Channel::Other
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub assets: Vec<(String, String)>,
}

fn numbers(v: &str) -> Vec<u32> {
    v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Is `other` later than `current`? Plain numbers, the only versions this project publishes.
pub fn is_newer(other: &str, current: &str) -> bool {
    numbers(other) > numbers(current)
}

pub fn parse_release(json: &str) -> crate::Result<Release> {
    let v: Value = serde_json::from_str(json).context("the answer about releases is not valid")?;
    let version = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
    if version.is_empty() {
        bail!("the answer about releases has no version");
    }
    let assets = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["name"].as_str()?.to_string(), a["browser_download_url"].as_str()?.to_string())))
        .collect();
    Ok(Release { version, page: v["html_url"].as_str().unwrap_or("").to_string(), assets })
}

/// The latest release on GitHub. This is the only request: it sends nothing but the usual client headers.
pub fn latest() -> crate::Result<Release> {
    let o = Command::new("curl")
        .args(["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", LATEST_URL])
        .output()
        .context("running curl")?;
    if !o.status.success() {
        bail!("network error: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    parse_release(&String::from_utf8_lossy(&o.stdout))
}

/// What the user runs to update. `None`: the app does it itself.
pub fn command_for(c: &Channel) -> Option<&'static str> {
    match c {
        Channel::AppImage(_) => None,
        Channel::Flatpak => Some("flatpak update io.github.xsteban1410.CsgoLegacyServer"),
        Channel::Deb => Some("sudo apt install ./csgo-legacy-server_VERSION_amd64.deb"),
        Channel::Pacman => Some("sudo pacman -Syu csgo-legacy-server"),
        Channel::Other => None,
    }
}

/// The hash of `file` in a SHA256SUMS text.
fn expected_hash<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines().find_map(|l| {
        let (h, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then_some(h)
    })
}

fn download(url: &str, to: &Path) -> crate::Result<()> {
    let s = Command::new("curl").args(["-fsSL", "--max-time", "600", "-o"]).arg(to).arg(url).status().context("running curl")?;
    if !s.success() {
        bail!("the download failed: {url}");
    }
    Ok(())
}

/// Replaces the AppImage, only if the file matches the release's SHA256SUMS. Applies on the next start.
pub fn update_appimage(r: &Release, target: &Path) -> crate::Result<()> {
    let find = |pred: &dyn Fn(&str) -> bool| r.assets.iter().find(|(n, _)| pred(n));
    let (name, url) = find(&|n| n.ends_with(".AppImage")).context("this release has no AppImage")?;
    let (_, sums_url) = find(&|n| n == "SHA256SUMS").context("this release has no SHA256SUMS")?;
    let part = target.with_extension("part");
    let sums = part.with_extension("sums");
    let result = (|| {
        download(sums_url, &sums)?;
        let text = std::fs::read_to_string(&sums)?;
        let want = expected_hash(&text, name).with_context(|| format!("{name} is not in SHA256SUMS"))?.to_string();
        download(url, &part)?;
        if crate::install::sha256(&part)? != want {
            bail!("the downloaded file does not match SHA256SUMS: nothing was changed");
        }
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&part, target)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&part);
    let _ = std::fs::remove_file(&sums);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_as_numbers() {
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("v1.2.0", "1.1.0"));
        assert!(!is_newer("1.1.0", "1.1.0"));
        assert!(!is_newer("1.0.9", "1.1.0"));
    }

    #[test]
    fn reads_a_release() {
        let r = parse_release(
            r#"{"tag_name":"v1.2.0","html_url":"https://x/r","assets":[{"name":"a.AppImage","browser_download_url":"https://x/a"},{"name":"bad"}]}"#,
        )
        .unwrap();
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.assets, [("a.AppImage".to_string(), "https://x/a".to_string())]);
        assert!(parse_release("{}").is_err());
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn finds_the_hash_of_a_file() {
        let sums = "aaa  one.deb\nbbb *two.AppImage\n";
        assert_eq!(expected_hash(sums, "one.deb"), Some("aaa"));
        assert_eq!(expected_hash(sums, "two.AppImage"), Some("bbb"));
        assert_eq!(expected_hash(sums, "three"), None);
    }

    #[test]
    fn a_wrong_hash_changes_nothing() {
        let tmp = std::env::temp_dir().join(format!("csls-upd-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("new.AppImage"), "new").unwrap();
        std::fs::write(tmp.join("SHA256SUMS"), "0000  new.AppImage\n").unwrap();
        let target = tmp.join("app.AppImage");
        std::fs::write(&target, "old").unwrap();
        let url = |n: &str| (n.to_string(), format!("file://{}", tmp.join(n).display()));
        let r = Release { assets: vec![url("new.AppImage"), url("SHA256SUMS")], ..Default::default() };
        assert!(update_appimage(&r, &target).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert!(!target.with_extension("part").exists());
        let good = crate::install::sha256(&tmp.join("new.AppImage")).unwrap();
        std::fs::write(tmp.join("SHA256SUMS"), format!("{good}  new.AppImage\n")).unwrap();
        update_appimage(&r, &target).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
