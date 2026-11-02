//! Abrir CS:GO Legacy en este PC (antes jugar.sh). Busca Steam nativo, flatpak y snap, y el juego
//! en cualquier biblioteca de Steam (libraryfolders.vdf).
use anyhow::{bail, Context};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub const GAME_DIR: &str = "Counter-Strike Global Offensive";
const FLATPAK_ID: &str = "com.valvesoftware.Steam";

#[derive(Debug, Clone, PartialEq)]
pub enum Flavor {
    Native,
    Flatpak,
    Snap,
}

#[derive(Debug, Clone)]
pub struct Steam {
    pub root: PathBuf,
    pub flavor: Flavor,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn find_steam() -> Option<Steam> {
    let h = home();
    [
        (h.join(".local/share/Steam"), Flavor::Native),
        (h.join(".steam/steam"), Flavor::Native),
        (h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"), Flavor::Flatpak),
        (h.join("snap/steam/common/.local/share/Steam"), Flavor::Snap),
    ]
    .into_iter()
    .find(|(p, _)| p.join("steamapps").is_dir())
    .map(|(root, flavor)| Steam { root, flavor })
}

/// Rutas "path" de libraryfolders.vdf (más la propia carpeta de Steam).
pub fn parse_library_folders(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|l| {
            let mut it = l.split('"').filter(|s| !s.trim().is_empty());
            (it.next()? == "path").then(|| it.next().map(|p| PathBuf::from(p.replace("\\\\", "\\"))))?
        })
        .collect()
}

impl Steam {
    pub fn libraries(&self) -> Vec<PathBuf> {
        let mut v = vec![self.root.clone()];
        let vdf = std::fs::read_to_string(self.root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
        for p in parse_library_folders(&vdf) {
            if !v.contains(&p) {
                v.push(p);
            }
        }
        v
    }

    /// Primer steamapps/common/<name> que exista en alguna biblioteca.
    pub fn find_common(&self, name: &str) -> Option<PathBuf> {
        self.libraries().into_iter().map(|l| l.join("steamapps/common").join(name)).find(|p| p.is_dir())
    }

    pub fn game_dir(&self) -> Option<PathBuf> {
        self.find_common(GAME_DIR).filter(|d| d.join("csgo.sh").is_file())
    }
}

/// ¿Hay un proceso "steam" corriendo?
pub fn steam_running() -> bool {
    std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "steam")
    })
}

/// Abre CS:GO Legacy, opcionalmente conectando a `connect` (IP:PUERTO).
pub fn launch_csgo(connect: Option<&str>) -> crate::Result<()> {
    let s = find_steam().context("no encuentro la carpeta de Steam")?;
    if !steam_running() {
        bail!("abre Steam primero");
    }
    let game = s.game_dir().context("no está CS:GO Legacy: pon la beta csgo_legacy en Steam > CS2 > Propiedades > Betas")?;
    let soldier = s.find_common("SteamLinuxRuntime_soldier").context("falta SteamLinuxRuntime_soldier (Steam lo instala al abrir CS2 una vez)")?;
    let scout = s.find_common("SteamLinuxRuntime").context("falta SteamLinuxRuntime (Steam lo instala al abrir CS2 una vez)")?;
    // Steam abre siempre CS2; CS:GO se lanza con su csgo.sh dentro del Steam Linux Runtime, como haría Steam
    let mut args: Vec<String> = vec![
        soldier.join("_v2-entry-point").to_string_lossy().into(),
        "--verb=waitforexitandrun".into(),
        "--".into(),
        scout.join("scout-on-soldier-entry-point-v2").to_string_lossy().into(),
        "--".into(),
        game.join("csgo.sh").to_string_lossy().into(),
        "-steam".into(),
        "-console".into(),
    ];
    if let Some(c) = connect {
        if !c.bytes().all(|b| b.is_ascii_alphanumeric() || b".:-".contains(&b)) {
            bail!("dirección inválida '{c}'");
        }
        args.extend(["+connect".into(), c.into()]);
    }
    let mut cmd = match s.flavor {
        // shortcut: flatpak y snap no se han probado; se ejecuta dentro del sandbox de Steam
        Flavor::Flatpak => {
            let mut c = Command::new("flatpak");
            c.args(["run", &format!("--command={}", args[0]), FLATPAK_ID]).args(&args[1..]);
            c
        }
        Flavor::Snap => {
            let mut c = Command::new("snap");
            c.args(["run", "--shell", "steam", "-c"]).arg(args.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" "));
            c
        }
        Flavor::Native => {
            let mut c = Command::new(&args[0]);
            c.args(&args[1..]);
            c
        }
    };
    use std::os::unix::process::CommandExt;
    cmd.current_dir(&game)
        .env("SteamAppId", "730")
        .env("SteamGameId", "730")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("lanzando CS:GO Legacy")?;
    Ok(())
}

/// Abre CS2 (lo que hace el botón Jugar de Steam).
pub fn launch_cs2() -> crate::Result<()> {
    let s = find_steam().context("no encuentro Steam")?;
    let mut c = match s.flavor {
        Flavor::Flatpak => {
            let mut c = Command::new("flatpak");
            c.args(["run", FLATPAK_ID]);
            c
        }
        _ => Command::new("steam"),
    };
    c.arg("steam://rungameid/730").stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bibliotecas() {
        let v = parse_library_folders(
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/juegos/SteamLibrary\"\n\t}\n}\n",
        );
        assert_eq!(v, vec![PathBuf::from("/home/u/.local/share/Steam"), PathBuf::from("/mnt/juegos/SteamLibrary")]);
    }
}
