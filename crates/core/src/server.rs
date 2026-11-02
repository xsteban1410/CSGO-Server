//! Arrancar, parar y cambiar de modo el servidor (antes arrancar.sh y modo.sh).
//! Con systemd --user el servidor corre como unidad transitoria y sobrevive a cerrar la app;
//! sin systemd, como grupo de procesos propio con archivo PID.
use crate::config::{Config, Network};
use crate::launch::LaunchOptions;
use crate::modes::Modes;
use crate::rcon::Rcon;
use crate::{deploy, net, paths};
use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const UNIT: &str = "csgo-legacy-server";
pub const BRIDGE_UNIT: &str = "csgo-legacy-server-bridge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Running,
}

pub struct Started {
    pub map: String,
    /// Dirección para `connect`.
    pub connect: String,
    pub deploy: deploy::Report,
    /// Cosas que no impiden arrancar (UPnP rechazado…).
    pub warnings: Vec<String>,
}

pub fn systemd_available() -> bool {
    if std::env::var_os("CSLS_NO_SYSTEMD").is_some() {
        return false;
    }
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn unit_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .status()
        .is_ok_and(|s| s.success())
}

fn pid_file(name: &str) -> PathBuf {
    paths::runtime_dir().join(format!("{name}.pid"))
}

fn pid_alive(name: &str) -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(pid_file(name)).ok()?.trim().parse().ok()?;
    Path::new(&format!("/proc/{pid}")).exists().then_some(pid)
}

/// Ejecutable del CLI (para lanzar el puente como proceso aparte).
pub fn cli_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("csgo-legacy-server")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("csgo-legacy-server"))
}

/// Lanza `cmd` en segundo plano con nombre `unit` (systemd o setsid + PID).
pub fn spawn_bg(unit: &str, cwd: &Path, env: &[(&str, String)], cmd: &[String], log: &Path) -> crate::Result<()> {
    std::fs::create_dir_all(cwd)?;
    if systemd_available() {
        // si quedó una unidad fallida con el mismo nombre, systemd-run se niega
        let _ = Command::new("systemctl").args(["--user", "reset-failed", unit]).stderr(Stdio::null()).status();
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--collect", "--quiet", &format!("--unit={unit}")])
            .arg(format!("--working-directory={}", cwd.display()));
        for (k, v) in env {
            c.arg(format!("--setenv={k}={v}"));
        }
        let st = c.arg("--").args(cmd).status().context("ejecutando systemd-run")?;
        if !st.success() {
            bail!("systemd-run no pudo lanzar {unit}");
        }
        return Ok(());
    }
    use std::os::unix::process::CommandExt;
    std::fs::create_dir_all(paths::runtime_dir())?;
    if let Some(d) = log.parent() {
        std::fs::create_dir_all(d)?;
    }
    let out = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(cwd)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .process_group(0)
        .spawn()
        .with_context(|| format!("lanzando {}", cmd[0]))?;
    std::fs::write(pid_file(unit), child.id().to_string())?;
    Ok(())
}

pub fn running(unit: &str) -> bool {
    (systemd_available() && unit_active(unit)) || pid_alive(unit).is_some()
}

fn kill_bg(unit: &str) {
    if systemd_available() && unit_active(unit) {
        let _ = Command::new("systemctl").args(["--user", "stop", unit]).status();
    }
    if let Some(pid) = pid_alive(unit) {
        // grupo entero: srcds_run + srcds_linux
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
        let t = Instant::now();
        while Path::new(&format!("/proc/{pid}")).exists() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = Command::new("kill").args(["-KILL", "--", &format!("-{pid}")]).stderr(Stdio::null()).status();
    }
    let _ = std::fs::remove_file(pid_file(unit));
}

pub struct Server<'a> {
    pub cfg: &'a Config,
}

impl<'a> Server<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Server { cfg }
    }

    pub fn dir(&self) -> &Path {
        self.cfg.install_dir()
    }
    pub fn csgo_dir(&self) -> PathBuf {
        self.dir().join("csgo")
    }
    pub fn maps_dir(&self) -> PathBuf {
        self.csgo_dir().join("maps")
    }
    pub fn console_log(&self) -> PathBuf {
        self.csgo_dir().join("console.log")
    }
    pub fn is_installed(&self) -> bool {
        self.dir().join("srcds_run").is_file()
    }

    pub fn state(&self) -> State {
        if running(UNIT) { State::Running } else { State::Stopped }
    }

    pub fn rcon(&self) -> crate::Result<Rcon> {
        Rcon::from_config(self.cfg)
    }

    /// Perfiles en los que juega gente de fuera (puerto abierto).
    pub fn public(&self) -> bool {
        matches!(self.cfg.server.network, Network::Internet | Network::Community)
    }

    /// Dirección para jugadores de internet (consulta la IP pública: hace una petición).
    pub fn public_address(&self) -> Option<String> {
        net::public_ip().map(|ip| format!("{ip}:{}", self.cfg.server.port))
    }

    pub fn connect_address(&self) -> String {
        let ip = net::local_ip().map(|i| i.to_string()).unwrap_or_else(|| "IP_DE_ESTE_PC".into());
        format!("{ip}:{}", self.cfg.server.port)
    }

    /// server.cfg se genera desde la config (nombre, contraseña, región, etiquetas).
    fn write_server_cfg(&self) -> crate::Result<()> {
        let s = &self.cfg.server;
        let txt = format!(
            "// Generado por csgo-legacy-server desde config.toml: no editar a mano.\n\
             hostname \"{}\"\nsv_password \"{}\"\nsv_cheats 0\nsv_region {}\nsv_tags \"{}\"\nbot_chatter off\n",
            s.hostname, s.password, s.region, s.tags
        );
        std::fs::write(self.csgo_dir().join("cfg/server.cfg"), txt)?;
        Ok(())
    }

    /// Arreglos que `steamcmd validate` deshace, por eso se aplican en cada arranque.
    fn patch(&self) -> crate::Result<()> {
        // el cliente CS:GO Legacy es la rama csgo_legacy de la app 730
        std::fs::write(self.dir().join("steam_appid.txt"), "730")?;
        let inf = self.csgo_dir().join("steam.inf");
        if let Ok(t) = std::fs::read_to_string(&inf) {
            let n: String = t
                .split_inclusive('\n')
                .map(|l| if l.starts_with("appID=") { "appID=730\r\n" } else { l })
                .collect();
            if n != t {
                std::fs::write(&inf, n)?;
            }
        }
        // la libgcc de Valve cuelga srcds al arrancar en distros modernas
        let _ = std::fs::remove_file(self.dir().join("bin/libgcc_s.so.1"));
        Ok(())
    }

    fn srcds_args(&self, opts: &LaunchOptions, map: &str, t: u8, g: u8) -> crate::Result<Vec<String>> {
        let s = &self.cfg.server;
        let mut a: Vec<String> = vec![
            self.dir().join("srcds_run").to_string_lossy().into_owned(),
            "-game".into(), "csgo".into(), "-console".into(), "-insecure".into(), "-condebug".into(), "-usercon".into(),
            "-port".into(), s.port.to_string(),
            "-maxplayers_override".into(), s.max_players.to_string(),
            "-tickrate".into(), s.tickrate.to_string(),
        ];
        if !s.workshop_authkey.is_empty() {
            a.extend(["-authkey".into(), s.workshop_authkey.clone()]);
        }
        a.extend(["+sv_lan".into(), s.network.sv_lan().to_string()]);
        if s.network == Network::Community {
            if s.gslt.is_empty() {
                bail!("el perfil comunitario necesita un token GSLT (https://steamcommunity.com/dev/managegameservers, app 730)");
            }
            a.extend(["+sv_setsteamaccount".into(), s.gslt.clone()]);
        }
        a.extend(["+game_type".into(), t.to_string(), "+game_mode".into(), g.to_string(), "+mapgroup".into(), "mg_active".into()]);
        match opts.workshop {
            Some(id) => a.extend(["+host_workshop_map".into(), id.to_string()]),
            None => a.extend(["+map".into(), map.into()]),
        }
        a.extend(["+rcon_password".into(), s.rcon_password.clone(), "+exec".into(), "server.cfg".into()]);
        Ok(a)
    }

    /// Arranca el servidor con estas opciones.
    pub fn start(&self, modes: &Modes, opts: &LaunchOptions) -> crate::Result<Started> {
        if !self.is_installed() {
            bail!("el servidor no está instalado en {} (csgo-legacy-server install)", self.dir().display());
        }
        if self.state() == State::Running {
            bail!("el servidor ya está encendido");
        }
        opts.validate(modes, self.cfg)?;
        let port = self.cfg.server.port;
        if !net::udp_port_free(port) {
            bail!("el puerto {port} está ocupado (¿otro servidor o el juego usando ese puerto?)");
        }
        let mode = modes.get(&opts.mode)?;
        let map = opts.pick_map(modes, self.cfg, None, &self.maps_dir())?;
        let (t, g) = opts.game_type_mode(mode);

        let report = deploy::deploy(self.cfg, false, false)?;
        if let Some(e) = report.error_text() {
            bail!("hay plugins que no compilan:\n{e}");
        }
        self.patch()?;
        self.write_server_cfg()?;
        // modo inicial: el plugin lo ejecuta una vez al cargar
        std::fs::write(
            self.csgo_dir().join("cfg/modo_inicio.cfg"),
            format!("modo_nombre \"{}\"\nmodo_humanos {}\nmodo_extra \"{}\"\n", opts.mode, opts.humans, opts.extra()),
        )?;

        // shortcut: console.log solo se rota al arrancar (crece ~1 MB/h con la app abierta); si molesta, rotar por tamaño
        let log = self.console_log();
        if log.is_file() {
            let _ = std::fs::rename(&log, log.with_extension("log.1"));
        }
        let args = self.srcds_args(opts, &map, t, g)?;
        let ld = format!(".:bin:{}", std::env::var("LD_LIBRARY_PATH").unwrap_or_default());
        spawn_bg(UNIT, self.dir(), &[("LD_LIBRARY_PATH", ld)], &args, &paths::data_dir().join("server.log"))?;

        let mut warnings = Vec::new();
        if self.public() && self.cfg.server.upnp {
            if let Err(e) = net::upnp_open(port) {
                warnings.push(format!("{e:#}"));
            }
        }
        if self.cfg.server.network == Network::Vpn && !running(BRIDGE_UNIT) {
            let exe = cli_exe().to_string_lossy().into_owned();
            spawn_bg(BRIDGE_UNIT, &paths::data_dir(), &[], &[exe, "bridge".into()], &paths::data_dir().join("bridge.log"))?;
        }
        Ok(Started { map, connect: self.connect_address(), deploy: report, warnings })
    }

    /// Espera a que rcon responda (el arranque tarda unos segundos).
    pub fn wait_rcon(&self, timeout: Duration) -> crate::Result<Rcon> {
        let t = Instant::now();
        loop {
            match self.rcon() {
                Ok(r) => return Ok(r),
                Err(e) if t.elapsed() > timeout => return Err(e),
                Err(e) if e.to_string().contains("contraseña") => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(700)),
            }
            if self.state() == State::Stopped {
                bail!("el servidor se cerró al arrancar; mira {}", self.console_log().display());
            }
        }
    }

    /// Para el servidor: `quit` por rcon y, si no basta, se mata.
    pub fn stop(&self) -> crate::Result<()> {
        if let Ok(mut r) = self.rcon() {
            let _ = r.exec("quit");
        }
        let t = Instant::now();
        while running(UNIT) && t.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(300));
        }
        kill_bg(UNIT);
        kill_bg(BRIDGE_UNIT);
        if self.public() && self.cfg.server.upnp {
            let _ = net::upnp_close(self.cfg.server.port);
        }
        Ok(())
    }

    pub fn current_map(r: &mut Rcon) -> Option<String> {
        // "host_map" = "de_dust2.bsp" ( def. "" )
        let o = r.exec("host_map").ok()?;
        let v = cvar_value(&o)?;
        Some(v.strip_suffix(".bsp").unwrap_or(v).to_string())
    }

    /// Suma (o resta) bots a bot_quota. Devuelve la cuota nueva.
    pub fn add_bots(&self, delta: i32) -> crate::Result<u32> {
        let mut r = self.rcon()?;
        let now: i32 = cvar_value(&r.exec("bot_quota")?).and_then(|v| v.parse().ok()).unwrap_or(0);
        let q = (now + delta).clamp(0, 63) as u32;
        r.exec(&format!("bot_quota {q}"))?;
        Ok(q)
    }

    /// Acciones rápidas de partida: reiniciar ronda o acabar el calentamiento.
    pub fn restart_round(&self) -> crate::Result<()> {
        self.rcon()?.exec("mp_restartgame 1")?;
        Ok(())
    }

    pub fn end_warmup(&self) -> crate::Result<()> {
        self.rcon()?.exec("mp_warmup_end")?;
        Ok(())
    }

    /// Cambia modo/mapa en caliente (antes modo.sh). Devuelve el mapa.
    pub fn change(&self, modes: &Modes, opts: &LaunchOptions) -> crate::Result<String> {
        opts.validate(modes, self.cfg)?;
        let mut r = self.rcon()?;
        let current = Self::current_map(&mut r);
        let map = opts.pick_map(modes, self.cfg, current.as_deref(), &self.maps_dir())?;
        let (t, g) = opts.game_type_mode(modes.get(&opts.mode)?);
        let report = deploy::deploy(self.cfg, false, true)?;
        if let Some(e) = report.error_text() {
            bail!("hay plugins que no compilan:\n{e}");
        }
        // el plugin aplica modo_base + modo_<modo> + modo_extra al cargar el mapa
        for c in [
            format!("modo_nombre {}", opts.mode),
            format!("modo_humanos {}", opts.humans),
            format!("modo_extra \"{}\"", opts.extra()),
            format!("game_type {t}"),
            format!("game_mode {g}"),
            match opts.workshop {
                Some(id) => format!("host_workshop_map {id}"),
                None => format!("changelevel {map}"),
            },
        ] {
            r.exec(&c)?;
        }
        Ok(map)
    }
}

/// Puente del amigo: escucha en `listen` (su PC) y reenvía a `remote` (puente del servidor, IP_VPN:PUERTO+1).
/// Valor de la respuesta a una cvar: `"bot_quota" = "5" ( def. "0" )` -> `5`.
pub fn cvar_value(out: &str) -> Option<&str> {
    out.split('"').nth(3)
}

pub fn start_friend_bridge(listen: u16, remote: &str) -> crate::Result<()> {
    let _: std::net::SocketAddr = remote.parse().context("dirección del servidor: IP_VPN:PUERTO")?;
    if running(BRIDGE_UNIT) {
        kill_bg(BRIDGE_UNIT);
    }
    let exe = cli_exe().to_string_lossy().into_owned();
    spawn_bg(BRIDGE_UNIT, &paths::data_dir(), &[], &[exe, "bridge".into(), listen.to_string(), remote.into()], &paths::data_dir().join("bridge.log"))
}

pub fn bridge_running() -> bool {
    running(BRIDGE_UNIT)
}

pub fn stop_bridge() {
    kill_bg(BRIDGE_UNIT);
}

pub const INSTALL_UNIT: &str = "csgo-legacy-server-install";

/// Progreso de la instalación en segundo plano: "fracción|mensaje", "done|..." o "error|...".
pub fn install_progress_file() -> PathBuf {
    paths::data_dir().join("install.progress")
}

/// Lanza `csgo-legacy-server install` en segundo plano (sigue aunque se cierre la app).
pub fn spawn_install(update: bool) -> crate::Result<()> {
    if running(INSTALL_UNIT) {
        return Ok(());
    }
    let _ = std::fs::remove_file(install_progress_file());
    let mut cmd = vec![cli_exe().to_string_lossy().into_owned(), "install".to_string()];
    if update {
        cmd.push("--update".into());
    }
    spawn_bg(INSTALL_UNIT, &paths::data_dir(), &[], &cmd, &paths::data_dir().join("install.log"))
}

impl Server<'_> {
    /// Genera el .nav del mapa cargado (para que los bots se muevan) y lo vuelve a cargar.
    /// El motor recarga mal los mapas del workshop al terminar, por eso se hace changelevel a mano.
    pub fn generate_nav(&self, timeout: Duration, force: bool) -> crate::Result<String> {
        let mut r = self.rcon()?;
        let map = Self::current_map(&mut r).context("no sé qué mapa está cargado")?;
        let nav = self.maps_dir().join(format!("{map}.nav"));
        // los oficiales traen un .nav hecho a mano mucho mejor que el generado: no pisarlo sin querer
        if !force && (nav.exists() || !map.contains('/')) {
            bail!("{map} ya tiene navegación para los bots (usa --force para regenerarla)");
        }
        let before = nav.metadata().and_then(|m| m.modified()).ok();
        // vacío e hibernando, el servidor no simula y la generación no avanza
        let hib = r.exec("sv_hibernate_when_empty")?.split('"').nth(3).unwrap_or("1").to_string();
        r.exec("sv_hibernate_when_empty 0")?;
        r.exec("sv_cheats 1")?;
        // la generación bloquea el servidor: la respuesta puede no llegar a tiempo
        let _ = r.exec("nav_generate");
        drop(r);
        let t = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = nav.metadata().and_then(|m| m.modified()).ok();
            if now.is_some() && now != before {
                break;
            }
            if t.elapsed() > timeout {
                bail!("el .nav no apareció en {} s", timeout.as_secs());
            }
        }
        std::thread::sleep(Duration::from_secs(3));
        // la conexión rcon se cae con el cambio de mapa del motor: una nueva
        let mut r = self.wait_rcon(Duration::from_secs(60))?;
        r.exec(&format!("changelevel {map}"))?;
        r.exec(&format!("sv_hibernate_when_empty {hib}"))?;
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn valor_de_cvar() {
        assert_eq!(super::cvar_value(r#""bot_quota" = "5" ( def. "0" )"#), Some("5"));
        assert_eq!(super::cvar_value("Unknown command"), None);
    }
}
