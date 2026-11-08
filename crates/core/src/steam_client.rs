//! Opens CS:GO Legacy on this PC (formerly jugar.sh). Finds native, flatpak and snap Steam, and the game
//! in any Steam library (libraryfolders.vdf).
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

/// "path" entries from libraryfolders.vdf (plus the Steam folder itself).
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

    /// First steamapps/common/<name> existing in any library.
    pub fn find_common(&self, name: &str) -> Option<PathBuf> {
        self.libraries().into_iter().map(|l| l.join("steamapps/common").join(name)).find(|p| p.is_dir())
    }

    pub fn game_dir(&self) -> Option<PathBuf> {
        self.find_common(GAME_DIR).filter(|d| d.join("csgo.sh").is_file())
    }
}

/// Is a "steam" process running?
pub fn steam_running() -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "steam"))
}

/// A `+connect` arrives before the engine is ready: the game prints "Deferring connect command!" and the command
/// is lost. With the game already open, `steam://connect` does get in.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(240);
const CONNECT_RETRY: Duration = Duration::from_secs(8);
/// Without network the game never writes the "ready" marker: try anyway after this long.
const READY_FALLBACK: Duration = Duration::from_secs(60);

fn valid_address(addr: &str) -> bool {
    !addr.is_empty() && addr.bytes().all(|b| b.is_ascii_alphanumeric() || b".:-".contains(&b))
}

/// What the game has written to console.log since `from`.
fn log_since(log: &Path, from: u64) -> String {
    let Ok(mut f) = std::fs::File::open(log) else { return String::new() };
    let mut b = Vec::new();
    if f.seek(SeekFrom::Start(from)).is_err() || f.read_to_end(&mut b).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&b).into_owned()
}

/// Waits for the menu and asks to connect until the console confirms. `alive` = the game is still open
/// (without it, steam://connect would open CS2). `true` if it has started connecting.
fn connect_when_ready(
    log: &Path,
    from: u64,
    mut alive: impl FnMut() -> bool,
    mut send: impl FnMut(),
    timeout: Duration,
    retry: Duration,
    ready_after: Duration,
) -> bool {
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

/// Opens CS:GO Legacy, optionally connecting to `connect` (IP:PORT).
/// With an address it returns the thread doing the connection: `true` if the game has started connecting.
/// You must stay alive for it to finish (the app does; the CLI waits).
pub fn launch_csgo(connect: Option<&str>) -> crate::Result<Option<JoinHandle<bool>>> {
    if connect.is_some_and(|c| !valid_address(c)) {
        bail!("invalid address '{}'", connect.unwrap_or_default());
    }
    if in_sandbox() {
        // through the portal steam://connect would open CS2 if the game is closed, and csgo.sh cannot be reached from here
        match connect {
            Some(addr) => bail!("the Flatpak cannot open the game: open CS:GO Legacy yourself and type in its console: connect {addr}"),
            None => bail!("the Flatpak cannot open the game: open CS:GO Legacy yourself"),
        }
    }
    let s = find_steam().context("Steam folder not found")?;
    if !steam_running() {
        bail!("open Steam first");
    }
    let game = s.game_dir().context("CS:GO Legacy is not installed: select the csgo_legacy beta in Steam > CS2 > Properties > Betas")?;
    let soldier = s
        .find_common("SteamLinuxRuntime_soldier")
        .context("SteamLinuxRuntime_soldier is missing (Steam installs it when CS2 is opened once)")?;
    let scout = s.find_common("SteamLinuxRuntime").context("SteamLinuxRuntime is missing (Steam installs it when CS2 is opened once)")?;
    // Steam always opens CS2; CS:GO is launched with its csgo.sh inside the Steam Linux Runtime, as Steam would
    let args: Vec<String> = vec![
        soldier.join("_v2-entry-point").to_string_lossy().into(),
        "--verb=waitforexitandrun".into(),
        "--".into(),
        scout.join("scout-on-soldier-entry-point-v2").to_string_lossy().into(),
        "--".into(),
        game.join("csgo.sh").to_string_lossy().into(),
        "-steam".into(),
        "-console".into(),
        // to know when the menu is ready and whether it joined. shortcut: console.log grows without rotation; trim it if it becomes a problem
        "-condebug".into(),
    ];
    let log = game.join("csgo/console.log");
    let from = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    let mut cmd = match s.flavor {
        // shortcut: flatpak and snap are untested; it runs inside Steam's sandbox
        Flavor::Flatpak => {
            let mut c = Command::new("flatpak");
            c.args(["run", &format!("--command={}", args[0]), FLATPAK_ID]).args(&args[1..]);
            c
        }
        Flavor::Snap => {
            let mut c = Command::new("snap");
            c.args(["run", "--shell", "steam", "-c"]).arg(shell_words(&args));
            c
        }
        Flavor::Native => {
            let mut c = Command::new(&args[0]);
            c.args(&args[1..]);
            c
        }
    };
    use std::os::unix::process::CommandExt;
    let mut child = cmd
        .current_dir(&game)
        .env("SteamAppId", "730")
        .env("SteamGameId", "730")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("launching CS:GO Legacy")?;
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

/// Inside this app's own Flatpak, where Steam's folders and processes are out of reach.
fn in_sandbox() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

/// The Steam executable, to pass it a steam:// URL.
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

/// Opens CS2 (what Steam's Play button does).
/// Arguments as one POSIX shell line: each in single quotes, with its own quotes written as '\''.
fn shell_words(args: &[String]) -> String {
    args.iter().map(|a| format!("'{}'", a.replace('\'', r"'\''"))).collect::<Vec<_>>().join(" ")
}

pub fn launch_cs2() -> crate::Result<()> {
    if in_sandbox() {
        Command::new("xdg-open").arg("steam://rungameid/730").stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        return Ok(());
    }
    let s = find_steam().context("Steam not found")?;
    steam_command(&s.flavor).arg("steam://rungameid/730").stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_words_survive_quotes() {
        let args: Vec<String> = ["/home/o'neil/steam", "a b", "$HOME;x"].map(String::from).to_vec();
        let line = shell_words(&args);
        let out = Command::new("sh").args(["-c", &format!("printf '%s\\n' {line}")]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "/home/o'neil/steam\na b\n$HOME;x\n");
    }

    #[test]
    fn addresses() {
        assert!(valid_address("1.2.3.4:27030"));
        assert!(!valid_address("1.2.3.4;quit"));
        assert!(!valid_address(""));
    }

    #[test]
    fn connects_when_menu_is_ready() {
        use std::cell::Cell;
        use std::io::Write;
        let log = std::env::temp_dir().join(format!("csls-console-{}.log", std::process::id()));
        std::fs::write(&log, "Connected to 9.9.9.9:1 (previous match)\n").unwrap();
        let from = std::fs::metadata(&log).unwrap().len();
        let sent_count = Cell::new(0);
        let (ms, long) = (Duration::from_millis(1), Duration::from_secs(20));
        // the game opens without writing "ready": nothing is sent, and when it closes it gives up (without opening CS2)
        let polls = Cell::new(0);
        let alive = || {
            polls.set(polls.get() + 1);
            polls.get() < 3
        };
        assert!(!connect_when_ready(&log, from, alive, || sent_count.set(sent_count.get() + 1), long, ms, long));
        assert_eq!(sent_count.get(), 0);
        // ready: sends until the console confirms
        let append = |t: &str| std::fs::OpenOptions::new().append(true).open(&log).unwrap().write_all(t.as_bytes()).unwrap();
        append("SDR RelayNetworkStatus: ...\n");
        let ok = connect_when_ready(
            &log,
            from,
            || true,
            || {
                sent_count.set(sent_count.get() + 1);
                if sent_count.get() == 2 {
                    append("Connecting to 1.2.3.4:27030\n");
                }
            },
            long,
            ms,
            long,
        );
        assert!(ok);
        assert_eq!(sent_count.get(), 2);
        std::fs::remove_file(log).ok();
    }

    #[test]
    fn libraries_from_vdf() {
        let v = parse_library_folders(
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games/SteamLibrary\"\n\t}\n}\n",
        );
        assert_eq!(v, vec![PathBuf::from("/home/u/.local/share/Steam"), PathBuf::from("/mnt/games/SteamLibrary")]);
    }
}
