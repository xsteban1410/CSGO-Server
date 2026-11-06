//! Abrir CS:GO Legacy en este PC (antes jugar.sh). Busca Steam nativo, flatpak y snap, y el juego
//! en cualquier biblioteca de Steam (libraryfolders.vdf).
use anyhow::{bail, Context};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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

/// Un `+connect` llega antes de que el motor esté listo: el juego imprime "Deferring connect command!" y la orden
/// se pierde. Con el juego ya abierto, `steam://connect` sí entra.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(240);
const CONNECT_RETRY: Duration = Duration::from_secs(8);
/// Sin red el juego no escribe la marca de "listo": se intenta igualmente pasado este tiempo.
const READY_FALLBACK: Duration = Duration::from_secs(60);

fn valid_address(addr: &str) -> bool {
    !addr.is_empty() && addr.bytes().all(|b| b.is_ascii_alphanumeric() || b".:-".contains(&b))
}

/// Lo que el juego ha escrito en console.log desde `from`.
fn log_since(log: &Path, from: u64) -> String {
    let Ok(mut f) = std::fs::File::open(log) else { return String::new() };
    let mut b = Vec::new();
    if f.seek(SeekFrom::Start(from)).is_err() || f.read_to_end(&mut b).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&b).into_owned()
}

/// Espera al menú y pide conectar hasta que la consola lo confirma. `alive` = el juego sigue abierto
/// (sin él, steam://connect abriría CS2). `true` si ha empezado a conectar.
fn connect_when_ready(log: &Path, from: u64, mut alive: impl FnMut() -> bool, mut send: impl FnMut(), timeout: Duration, retry: Duration, ready_after: Duration) -> bool {
    let t = Instant::now();
    let mut sent: Option<Instant> = None;
    while t.elapsed() < timeout && alive() {
        let out = log_since(log, from);
        if out.contains("Connecting to") || out.contains("Connected to") {
            return true;
        }
        let ready = out.contains("SDR RelayNetworkStatus") || t.elapsed() >= ready_after;
        if ready && sent.is_none_or(|s| s.elapsed() >= retry) {
            send();
            sent = Some(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// Abre CS:GO Legacy, opcionalmente conectando a `connect` (IP:PUERTO).
/// Con dirección devuelve el hilo que hace la conexión: `true` si el juego ha empezado a conectar.
/// Hay que seguir vivo para que termine (la app lo está; el CLI espera).
pub fn launch_csgo(connect: Option<&str>) -> crate::Result<Option<JoinHandle<bool>>> {
    if connect.is_some_and(|c| !valid_address(c)) {
        bail!("dirección inválida '{}'", connect.unwrap_or_default());
    }
    let s = find_steam().context("no encuentro la carpeta de Steam")?;
    if !steam_running() {
        bail!("abre Steam primero");
    }
    let game = s.game_dir().context("no está CS:GO Legacy: pon la beta csgo_legacy en Steam > CS2 > Propiedades > Betas")?;
    let soldier = s.find_common("SteamLinuxRuntime_soldier").context("falta SteamLinuxRuntime_soldier (Steam lo instala al abrir CS2 una vez)")?;
    let scout = s.find_common("SteamLinuxRuntime").context("falta SteamLinuxRuntime (Steam lo instala al abrir CS2 una vez)")?;
    // Steam abre siempre CS2; CS:GO se lanza con su csgo.sh dentro del Steam Linux Runtime, como haría Steam
    let args: Vec<String> = vec![
        soldier.join("_v2-entry-point").to_string_lossy().into(),
        "--verb=waitforexitandrun".into(),
        "--".into(),
        scout.join("scout-on-soldier-entry-point-v2").to_string_lossy().into(),
        "--".into(),
        game.join("csgo.sh").to_string_lossy().into(),
        "-steam".into(),
        "-console".into(),
        // para saber cuándo está listo el menú y si ha entrado. shortcut: console.log crece sin rotar; recortarlo si molesta
        "-condebug".into(),
    ];
    let log = game.join("csgo/console.log");
    let from = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
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
    let mut child = cmd.current_dir(&game)
        .env("SteamAppId", "730")
        .env("SteamGameId", "730")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("lanzando CS:GO Legacy")?;
    let Some(addr) = connect.map(str::to_string) else { return Ok(None) };
    let url = format!("steam://connect/{addr}");
    let flavor = s.flavor;
    Ok(Some(std::thread::spawn(move || {
        connect_when_ready(
            &log,
            from,
            || matches!(child.try_wait(), Ok(None)),
            || {
                let _ = steam_command(&flavor).arg(&url).stdout(Stdio::null()).stderr(Stdio::null()).status();
            },
            CONNECT_TIMEOUT,
            CONNECT_RETRY,
            READY_FALLBACK,
        )
    })))
}

/// El ejecutable de Steam, para pasarle una URL steam://.
fn steam_command(flavor: &Flavor) -> Command {
    match flavor {
        Flavor::Flatpak => {
            let mut c = Command::new("flatpak");
            c.args(["run", FLATPAK_ID]);
            c
        }
        _ => Command::new("steam"),
    }
}

/// Abre CS2 (lo que hace el botón Jugar de Steam).
pub fn launch_cs2() -> crate::Result<()> {
    let s = find_steam().context("no encuentro Steam")?;
    steam_command(&s.flavor).arg("steam://rungameid/730").stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direcciones() {
        assert!(valid_address("1.2.3.4:27030"));
        assert!(!valid_address("1.2.3.4;quit"));
        assert!(!valid_address(""));
    }

    #[test]
    fn conecta_cuando_el_menu_esta_listo() {
        use std::cell::Cell;
        use std::io::Write;
        let log = std::env::temp_dir().join(format!("csls-console-{}.log", std::process::id()));
        std::fs::write(&log, "Connected to 9.9.9.9:1 (partida anterior)\n").unwrap();
        let from = std::fs::metadata(&log).unwrap().len();
        let enviados = Cell::new(0);
        let (ms, largo) = (Duration::from_millis(1), Duration::from_secs(20));
        // el juego abre sin escribir "listo": no se envía nada, y al cerrarse se abandona (sin abrir CS2)
        let vueltas = Cell::new(0);
        let vivo = || { vueltas.set(vueltas.get() + 1); vueltas.get() < 3 };
        assert!(!connect_when_ready(&log, from, vivo, || enviados.set(enviados.get() + 1), largo, ms, largo));
        assert_eq!(enviados.get(), 0);
        // listo: envía hasta que la consola confirma
        let append = |t: &str| std::fs::OpenOptions::new().append(true).open(&log).unwrap().write_all(t.as_bytes()).unwrap();
        append("SDR RelayNetworkStatus: ...\n");
        let ok = connect_when_ready(&log, from, || true, || {
            enviados.set(enviados.get() + 1);
            if enviados.get() == 2 {
                append("Connecting to 1.2.3.4:27030\n");
            }
        }, largo, ms, largo);
        assert!(ok);
        assert_eq!(enviados.get(), 2);
        std::fs::remove_file(log).ok();
    }

    #[test]
    fn bibliotecas() {
        let v = parse_library_folders(
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/juegos/SteamLibrary\"\n\t}\n}\n",
        );
        assert_eq!(v, vec![PathBuf::from("/home/u/.local/share/Steam"), PathBuf::from("/mnt/juegos/SteamLibrary")]);
    }
}
