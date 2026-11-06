//! Start, stop and change mode on the server (formerly arrancar.sh and modo.sh).
//! With systemd --user the server runs as a transient unit and survives closing the app;
//! without systemd, as its own process group with a PID file.
use crate::config::{Config, Network};
use crate::launch::LaunchOptions;
use crate::modes::Modes;
use crate::rcon::Rcon;
use crate::{deploy, net, paths};
use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SECRETS_CFG: &str = "csls_secrets.cfg";

pub const UNIT: &str = "csgo-legacy-server";
pub const BRIDGE_UNIT: &str = "csgo-legacy-server-bridge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Running,
}

pub struct Started {
    pub map: String,
    /// Address for `connect`.
    pub connect: String,
    pub deploy: deploy::Report,
    /// Things that do not prevent starting (UPnP rejected…).
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

/// CLI executable (to launch the bridge as a separate process).
pub fn cli_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("csgo-legacy-server")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("csgo-legacy-server"))
}

/// Runs `cmd` in the background named `unit` (systemd or setsid + PID).
pub fn spawn_bg(unit: &str, cwd: &Path, env: &[(&str, String)], cmd: &[String], log: &Path) -> crate::Result<()> {
    std::fs::create_dir_all(cwd)?;
    if systemd_available() {
        // if a failed unit with the same name remains, systemd-run refuses
        let _ = Command::new("systemctl").args(["--user", "reset-failed", unit]).stderr(Stdio::null()).status();
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--collect", "--quiet", &format!("--unit={unit}")])
            .arg(format!("--working-directory={}", cwd.display()));
        for (k, v) in env {
            c.arg(format!("--setenv={k}={v}"));
        }
        let st = c.arg("--").args(cmd).status().context("running systemd-run")?;
        if !st.success() {
            bail!("systemd-run could not start {unit}");
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
        self.dir().join("srcds_run").is_file() && !self.dir().join(crate::install::INCOMPLETE_MARKER).exists()
    }

    pub fn state(&self) -> State {
        if running(UNIT) { State::Running } else { State::Stopped }
    }

    pub fn rcon(&self) -> crate::Result<Rcon> {
        Rcon::from_config(self.cfg)
    }

    /// Profiles where outside people play (port open).
    pub fn public(&self) -> bool {
        matches!(self.cfg.server.network, Network::Internet | Network::Community)
    }

    /// Address for internet players (looks up the public IP: makes a request).
    pub fn public_address(&self) -> Option<String> {
        net::public_ip().map(|ip| format!("{ip}:{}", self.cfg.server.port))
    }

    pub fn connect_address(&self) -> String {
        let ip = net::local_ip().map(|i| i.to_string()).unwrap_or_else(|| "THIS_PC_IP".into());
        format!("{ip}:{}", self.cfg.server.port)
    }

    /// server.cfg is generated from the config (name, password, region, tags).
    fn write_server_cfg(&self) -> crate::Result<()> {
        let s = &self.cfg.server;
        let txt = format!(
            "// Generated by csgo-legacy-server from config.toml: do not edit by hand.\n\
             hostname \"{}\"\nsv_password \"{}\"\nsv_cheats 0\nsv_region {}\nsv_tags \"{}\"\nbot_chatter off\n",
            s.hostname, s.password, s.region, s.tags
        );
        write_private(&self.csgo_dir().join("cfg/server.cfg"), &txt)?;
        Ok(())
    }

    /// Secrets go in files only the user can read: anyone on the PC can see the arguments of a process.
    fn write_secrets(&self) -> crate::Result<()> {
        let s = &self.cfg.server;
        let mut txt = format!("// Generated by csgo-legacy-server from config.toml: do not edit by hand.\nrcon_password \"{}\"\n", s.rcon_password);
        if s.network == Network::Community {
            txt.push_str(&format!("sv_setsteamaccount \"{}\"\n", s.gslt));
        }
        write_private(&self.csgo_dir().join(format!("cfg/{SECRETS_CFG}")), &txt)?;
        let authkey = self.csgo_dir().join("webapi_authkey.txt");
        if s.workshop_authkey.is_empty() {
            let _ = std::fs::remove_file(authkey);
        } else {
            write_private(&authkey, &s.workshop_authkey)?;
        }
        Ok(())
    }

    /// The plugin runs this file whenever it loads. It has to follow live changes too: a plugin hot-reload
    /// would otherwise bring back the mode the server was started with.
    fn write_mode_start(&self, mode: &crate::modes::Mode, opts: &LaunchOptions) -> crate::Result<()> {
        std::fs::write(
            self.csgo_dir().join("cfg/modo_inicio.cfg"),
            format!("csls_language \"{}\"\nmodo_nombre \"{}\"\nmodo_tipo \"{}\"\nmodo_humanos {}\nmodo_extra \"{}\"\n", self.cfg.language.as_str(), opts.mode, opts.tipo_cfg(mode), opts.humans, opts.extra(mode)),
        )?;
        Ok(())
    }

    /// Fixes that `steamcmd validate` undoes, so they are applied on every start.
    fn patch(&self) -> crate::Result<()> {
        // the CS:GO Legacy client is the csgo_legacy branch of app 730
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
        // Valve's libgcc hangs srcds at startup on modern distros
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
        a.extend(["+sv_lan".into(), s.network.sv_lan().to_string()]);
        if s.network == Network::Community && s.gslt.is_empty() {
            bail!("the community profile needs a GSLT token (https://steamcommunity.com/dev/managegameservers, app 730)");
        }
        // before the map: the Steam account has to be set by the time the server starts
        a.extend(["+exec".into(), SECRETS_CFG.into()]);
        a.extend(["+game_type".into(), t.to_string(), "+game_mode".into(), g.to_string(), "+mapgroup".into(), "mg_active".into()]);
        match opts.workshop {
            Some(id) => a.extend(["+host_workshop_map".into(), id.to_string()]),
            None => a.extend(["+map".into(), map.into()]),
        }
        a.extend(["+exec".into(), "server.cfg".into()]);
        Ok(a)
    }

    /// Starts the server with these options.
    pub fn start(&self, modes: &Modes, opts: &LaunchOptions) -> crate::Result<Started> {
        if !self.is_installed() {
            bail!("the server is not installed or its install did not finish in {} (csgo-legacy-server install)", self.dir().display());
        }
        if self.state() == State::Running {
            bail!("the server is already running");
        }
        opts.validate(modes, self.cfg)?;
        let port = self.cfg.server.port;
        // a bridge from the Join tab may hold the port
        kill_bg(BRIDGE_UNIT);
        if !net::udp_port_free(port) {
            bail!("port {port} is taken (another server, or the game using it?)");
        }
        let mode = modes.get(&opts.mode)?;
        let map = opts.pick_map(modes, self.cfg, None, &self.maps_dir())?;
        let (t, g) = opts.game_type_mode(mode);

        let report = deploy::deploy(self.cfg, false, false)?;
        if let Some(e) = report.error_text() {
            bail!("some plugins do not compile:\n{e}");
        }
        self.patch()?;
        self.write_server_cfg()?;
        self.write_secrets()?;
        self.write_mode_start(mode, opts)?;

        // shortcut: console.log only rotates at startup (grows ~1 MB/h with the app open); rotate by size if it becomes a problem
        let log = self.console_log();
        if log.is_file() {
            let _ = std::fs::rename(&log, log.with_extension("log.1"));
        }
        let args = self.srcds_args(opts, &map, t, g)?;
        let ld = format!(".:bin:{}", std::env::var("LD_LIBRARY_PATH").unwrap_or_default());
        spawn_bg(UNIT, self.dir(), &[("LD_LIBRARY_PATH", ld)], &args, &paths::data_dir().join("server.log"))?;

        let mut warnings = Vec::new();
        if self.public() {
            warnings.push(exposure_warning(port, &self.cfg.server.password));
        }
        if self.public() && self.cfg.server.upnp {
            if let Err(e) = net::upnp_open(port) {
                warnings.push(format!("{e:#}"));
            }
        }
        if self.cfg.server.network == Network::Vpn {
            if let Err(e) = self.start_bridge() {
                warnings.push(format!("{e:#}"));
            }
            if let Some(h) = net::firewall_hint(port + 1) {
                warnings.push(format!("Firewall active: if you have not done so, open the bridge port with {h}"));
            }
        }
        Ok(Started { map, connect: self.connect_address(), deploy: report, warnings })
    }

    /// Server-side bridge (VPN profile): receives friends on PORT+1.
    pub fn start_bridge(&self) -> crate::Result<()> {
        kill_bg(BRIDGE_UNIT);
        let exe = cli_exe().to_string_lossy().into_owned();
        spawn_bg(BRIDGE_UNIT, &paths::data_dir(), &[], &[exe, "bridge".into()], &paths::data_dir().join("bridge.log"))?;
        // if it fails (port taken, no CLI) it dies instantly
        std::thread::sleep(Duration::from_millis(800));
        if !running(BRIDGE_UNIT) {
            bail!("the bridge did not start: is UDP port {} taken? See `journalctl --user -u {BRIDGE_UNIT}`", self.cfg.server.port + 1);
        }
        Ok(())
    }

    /// Address to give to friends over VPN.
    pub fn vpn_address(&self) -> Option<String> {
        net::vpn_ip().map(|ip| format!("{ip}:{}", self.cfg.server.port))
    }

    /// Waits for rcon to respond (startup takes a few seconds).
    pub fn wait_rcon(&self, timeout: Duration) -> crate::Result<Rcon> {
        let t = Instant::now();
        loop {
            match self.rcon() {
                Ok(r) => return Ok(r),
                Err(e) if t.elapsed() > timeout => return Err(e),
                Err(e) if e.to_string().contains("password") => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(700)),
            }
            if self.state() == State::Stopped {
                bail!("the server closed while starting; see {}", self.console_log().display());
            }
        }
    }

    /// Stops the server: `quit` over rcon and, if that is not enough, kill.
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

    /// The state as JSON, for scripts.
    pub fn status_json(&self) -> crate::Result<String> {
        if self.state() != State::Running {
            return Ok(serde_json::json!({ "running": false }).to_string());
        }
        let mut r = self.rcon()?;
        let st = crate::players::parse_status(&r.exec("status")?);
        let mode = r.exec("modo_nombre").unwrap_or_default();
        Ok(serde_json::json!({
            "running": true,
            "hostname": st.hostname,
            "mode": mode.split('"').nth(3).unwrap_or(""),
            "map": st.map,
            "humans": st.humans,
            "bots": st.bots,
            "max_players": st.max,
            "connect": self.connect_address(),
        })
        .to_string())
    }

    pub fn current_map(r: &mut Rcon) -> Option<String> {
        // "host_map" = "de_dust2.bsp" ( def. "" )
        let o = r.exec("host_map").ok()?;
        let v = cvar_value(&o)?;
        Some(v.strip_suffix(".bsp").unwrap_or(v).to_string())
    }

    /// Adds (or removes) bots to bot_quota. Returns the new quota.
    pub fn add_bots(&self, delta: i32) -> crate::Result<u32> {
        let mut r = self.rcon()?;
        let now: i32 = cvar_value(&r.exec("bot_quota")?).and_then(|v| v.parse().ok()).unwrap_or(0);
        let q = (now + delta).clamp(0, 63) as u32;
        r.exec(&format!("bot_quota {q}"))?;
        Ok(q)
    }

    /// Quick match actions: restart round or end warmup.
    pub fn restart_round(&self) -> crate::Result<()> {
        self.rcon()?.exec("mp_restartgame 1")?;
        Ok(())
    }

    pub fn end_warmup(&self) -> crate::Result<()> {
        self.rcon()?.exec("mp_warmup_end")?;
        Ok(())
    }

    /// Changes mode/map live (formerly modo.sh). Returns the map.
    pub fn change(&self, modes: &Modes, opts: &LaunchOptions) -> crate::Result<String> {
        opts.validate(modes, self.cfg)?;
        let current = Self::current_map(&mut self.rcon()?);
        let map = opts.pick_map(modes, self.cfg, current.as_deref(), &self.maps_dir())?;
        let mode = modes.get(&opts.mode)?;
        let (t, g) = opts.game_type_mode(mode);
        let report = deploy::deploy(self.cfg, false, true)?;
        if let Some(e) = report.error_text() {
            bail!("some plugins do not compile:\n{e}");
        }
        self.write_mode_start(mode, opts)?;
        // after the deploy: reloading plugins can leave an already open connection unresponsive
        let mut r = self.rcon()?;
        // the plugin applies modo_base + modo_<mode> + modo_extra when the map loads
        let load = match opts.workshop {
            Some(id) => format!("host_workshop_map {id}"),
            None => format!("changelevel {map}"),
        };
        for c in [
            format!("modo_nombre {}", opts.mode),
            format!("modo_tipo \"{}\"", opts.tipo_cfg(mode)),
            format!("modo_humanos {}", opts.humans),
            format!("modo_extra \"{}\"", opts.extra(mode)),
            format!("game_type {t}"),
            format!("game_mode {g}"),
        ] {
            r.exec(&c)?;
        }
        // with the map change the response may not arrive in time, even though the map loads
        let _ = r.exec(&load);
        Ok(map)
    }
}

/// Value from a cvar response: `"bot_quota" = "5" ( def. "0" )` -> `5`.
pub fn cvar_value(out: &str) -> Option<&str> {
    out.split('"').nth(3)
}

/// Friend's bridge: listens on `listen` (their PC) and forwards to `remote` (server bridge, VPN_IP:PORT+1).
pub fn start_friend_bridge(listen: u16, remote: &str) -> crate::Result<()> {
    let _: std::net::SocketAddr = remote.parse().context("server address: VPN_IP:PORT")?;
    kill_bg(BRIDGE_UNIT);
    let exe = cli_exe().to_string_lossy().into_owned();
    spawn_bg(BRIDGE_UNIT, &paths::data_dir(), &[], &[exe, "bridge".into(), listen.to_string(), remote.into()], &paths::data_dir().join("bridge.log"))
}

/// Starts the bridge to `target` (server VPN_IP:PORT) and returns the local address to connect to.
/// Steam refuses connecting to the VPN IP: connect to your own local IP instead.
pub fn join_through_bridge(target: &str) -> crate::Result<String> {
    let (ip, p) = target.rsplit_once(':').context("address: VPN_IP:PORT")?;
    let port: u16 = p.parse().context("invalid port")?;
    // the previous bridge is freed before looking for a port
    kill_bg(BRIDGE_UNIT);
    let listen = [27030u16, 27040, 27050].into_iter().find(|p| net::udp_port_free(*p)).context("no free UDP port for the bridge")?;
    start_friend_bridge(listen, &format!("{ip}:{}", port + 1))?;
    let local = net::local_ip().context("no local IP")?;
    Ok(format!("{local}:{listen}"))
}

pub fn bridge_running() -> bool {
    running(BRIDGE_UNIT)
}

pub fn stop_bridge() {
    kill_bg(BRIDGE_UNIT);
}

pub const INSTALL_UNIT: &str = "csgo-legacy-server-install";

/// Background install progress: "fraction|message", "done|..." or "error|...".
pub fn install_progress_file() -> PathBuf {
    paths::data_dir().join("install.progress")
}

/// Runs `csgo-legacy-server install` in the background (continues if the app is closed).
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
    /// Generates the .nav of the loaded map (so bots move) and reloads it.
    /// The engine reloads workshop maps badly when finished, so changelevel is done by hand.
    pub fn generate_nav(&self, timeout: Duration, force: bool) -> crate::Result<String> {
        let mut r = self.rcon()?;
        let map = Self::current_map(&mut r).context("cannot tell which map is loaded")?;
        let nav = self.maps_dir().join(format!("{map}.nav"));
        // official maps ship a hand-made .nav much better than the generated one: do not overwrite it by accident
        if !force && (nav.exists() || !map.contains('/')) {
            bail!("{map} already has bot navigation (use --force to regenerate it)");
        }
        let before = nav.metadata().and_then(|m| m.modified()).ok();
        // empty and hibernating, the server does not simulate and generation does not progress
        let hib = r.exec("sv_hibernate_when_empty")?.split('"').nth(3).unwrap_or("1").to_string();
        r.exec("sv_hibernate_when_empty 0")?;
        r.exec("sv_cheats 1")?;
        // generation blocks the server: the response may not arrive in time
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
                bail!("the .nav did not appear in {} s", timeout.as_secs());
            }
        }
        std::thread::sleep(Duration::from_secs(3));
        // the rcon connection drops on the engine's map change: open a new one
        let mut r = self.wait_rcon(Duration::from_secs(60))?;
        r.exec(&format!("changelevel {map}"))?;
        r.exec(&format!("sv_hibernate_when_empty {hib}"))?;
        Ok(map)
    }
}

/// Use of the machine by the server process.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProcStats {
    pub cpu_percent: u32,
    pub rss_mb: u64,
    pub uptime_secs: u64,
}

/// (CPU ticks used, start time in ticks) from the text of /proc/PID/stat.
fn parse_proc_stat(stat: &str) -> Option<(u64, u64)> {
    // the name in parentheses may hold spaces; after it: state is field 0, utime 11, stime 12, starttime 19
    let f: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?, f.get(19)?.parse().ok()?))
}

fn server_pid(port: u16) -> Option<u32> {
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
        let pid: u32 = e.file_name().to_str()?.parse().ok()?;
        let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
        let cmd = std::fs::read(e.path().join("cmdline")).ok()?;
        (comm.trim() == "srcds_linux" && String::from_utf8_lossy(&cmd).contains(&format!("-port\0{port}\0"))).then_some(pid)
    })
}

/// Samples the server for a moment. shortcut: assumes 100 ticks per second, as on every usual Linux build.
pub fn process_stats(port: u16) -> Option<ProcStats> {
    const TICKS: u64 = 100;
    const SAMPLE_MS: u64 = 250;
    let pid = server_pid(port)?;
    let stat = || std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|s| parse_proc_stat(&s));
    let (before, start) = stat()?;
    std::thread::sleep(Duration::from_millis(SAMPLE_MS));
    let (after, _) = stat()?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kb: u64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.split_whitespace().next()?.parse().ok()?;
    let boot_secs: f64 = std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()?;
    Some(ProcStats {
        cpu_percent: ((after - before) * 100 * 1000 / (TICKS * SAMPLE_MS)) as u32,
        rss_mb: rss_kb / 1024,
        uptime_secs: (boot_secs as u64).saturating_sub(start / TICKS),
    })
}

/// What opening the server to the internet means.
fn exposure_warning(port: u16, password: &str) -> String {
    // the fixed part comes before ": " so the app can translate it
    if password.is_empty() {
        format!("This server is reachable from the internet, has no VAC (-insecure) and anyone can join without a password. UDP port: {port}")
    } else {
        format!("This server is reachable from the internet and has no VAC (-insecure). Players need its password. UDP port: {port}")
    }
}

/// Writes a file only the user can read, also if it already existed with wider permissions.
fn write_private(path: &Path, txt: &str) -> crate::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(txt.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_proc_stat() {
        let stat = "123 (srcds linux) S 1 2 3 4 5 6 7 8 9 10 40 2 0 0 20 0 1 0 5000 999 7";
        assert_eq!(parse_proc_stat(stat), Some((42, 5000)));
        assert_eq!(parse_proc_stat("nonsense"), None);
    }

    #[test]
    fn secrets_are_not_in_the_arguments() {
        let mut cfg = Config::default();
        cfg.server.rcon_password = "rconsecret".into();
        cfg.server.gslt = "gsltsecret".into();
        cfg.server.workshop_authkey = "keysecret".into();
        cfg.server.network = Network::Community;
        let args = Server::new(&cfg).srcds_args(&LaunchOptions::from_config(&cfg), "de_dust2", 0, 1).unwrap().join(" ");
        assert!(args.contains(SECRETS_CFG));
        for secret in ["rconsecret", "gsltsecret", "keysecret"] {
            assert!(!args.contains(secret), "{secret} in {args}");
        }
    }

    #[test]
    fn private_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let p = std::env::temp_dir().join(format!("csls-private-{}", std::process::id()));
        std::fs::write(&p, "old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&p, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn exposure_warning_says_if_anyone_can_join() {
        assert!(exposure_warning(27030, "").contains("without a password"));
        assert!(exposure_warning(27030, "x").contains("its password"));
    }
    #[test]
    fn cvar_value() {
        assert_eq!(super::cvar_value(r#""bot_quota" = "5" ( def. "0" )"#), Some("5"));
        assert_eq!(super::cvar_value("Unknown command"), None);
    }
}
