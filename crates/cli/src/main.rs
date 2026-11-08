//! csgo-legacy-server: CLI for headless servers (the desktop app uses the same core).
use anyhow::{bail, Context, Result};
use clap::{Args, CommandFactory, Parser, Subcommand};
use csls_core::config::{Config, Kind};
use csls_core::launch::{parse_cvar, LaunchOptions, Site, Teams};
use csls_core::modes::Modes;
use csls_core::server::{Server, State};
use csls_core::{backup, bridge, deploy, doctor, install, modes, net, players, steam_client, workshop};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "csgo-legacy-server", version, about = "CS:GO Legacy dedicated server manager")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Game {
    /// Game mode (see `modes`)
    mode: Option<String>,
    /// Humans to wait for before the match starts: a number or `alone` (default: Valve's timed warmup)
    humans: Option<String>,
    #[arg(short, long)]
    map: Option<String>,
    /// competitive | short | casual | wingman | dm | ffa | tdm | armsrace | demolition | guardian | dz
    #[arg(short = 't', long = "type")]
    kind: Option<String>,
    /// Bot difficulty 0-3
    #[arg(short, long)]
    difficulty: Option<u8>,
    #[arg(short, long)]
    bots: Option<u32>,
    /// Humans only: no bots fill the empty seats
    #[arg(long, conflicts_with = "bots")]
    no_bots: bool,
    #[arg(short, long)]
    rounds: Option<u32>,
    /// Retake/post-plant site: a | b | random
    #[arg(short, long)]
    site: Option<String>,
    /// Competitive modes, where humans start: versus (one on each side) | together | free
    #[arg(long)]
    teams: Option<String>,
    /// Competitive and Premier: overtime until someone wins
    #[arg(long)]
    no_draw: bool,
    /// Infinite accuracy: no spread and no recoil for everyone
    #[arg(short, long)]
    precision: bool,
    /// No automatic kicks or bans
    #[arg(long)]
    anarchy: bool,
    /// Workshop map id
    #[arg(short, long)]
    workshop: Option<u64>,
    /// Any server variable, repeatable: --set cvar=value
    #[arg(long = "set")]
    set: Vec<String>,
    /// Start from a saved preset (other flags override it)
    #[arg(long)]
    preset: Option<String>,
    /// Save these options as a preset with this name
    #[arg(long)]
    save_preset: Option<String>,
    /// `mode` only: print the rcon commands as JSON without applying them
    #[arg(long, hide = true)]
    plan: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download and set up the dedicated server (~33 GB). Safe to repeat; resumes
    Install {
        /// Validate files and update SourceMod too
        #[arg(long)]
        update: bool,
    },
    /// Check the system and say how to fix what is missing
    Doctor {
        /// Text to paste in an issue, without passwords, IPs or SteamIDs
        #[arg(long)]
        report: bool,
    },
    /// Open CS:GO Legacy (or CS2) on this PC, optionally connecting: play [cs2] [IP:PORT|local]
    Play { target: Vec<String> },
    /// Start the server
    Start(Game),
    /// Stop the server
    Stop,
    /// Show whether it is running, map and players
    Status {
        /// Print JSON
        #[arg(long)]
        json: bool,
    },
    /// Change mode/map with the server running
    Mode(Game),
    /// List available modes
    Modes,
    /// List installed maps
    Maps,
    /// List players
    Players,
    /// Kick a player by userid
    Kick { userid: u32, reason: Vec<String> },
    /// Ban a player by userid (minutes, 0 = permanent)
    Ban { userid: u32, minutes: u32, reason: Vec<String> },
    /// Unban a SteamID (STEAM_1:x:y)
    Unban { steamid: String },
    /// Run rcon commands
    Rcon { commands: Vec<String> },
    /// Compile plugins and copy configs (only what changed)
    Deploy {
        #[arg(long)]
        force: bool,
        /// Keep watching plugin and mode folders and redeploy on changes
        #[arg(long)]
        watch: bool,
    },
    /// Workshop maps (CS:GO only; CS2 maps cannot run on CS:GO Legacy)
    #[command(subcommand)]
    Workshop(Ws),
    /// Save or restore your config, modes, plugins, presets and skins (never the server)
    #[command(subcommand)]
    Backup(Backup),
    /// Share custom modes: export <mode> > file.toml, import file.toml, delete <mode>
    #[command(subcommand)]
    ModeFile(ModeFile),
    /// Game modes, plugins and packs from CSGO-Hub
    #[command(subcommand)]
    Hub(HubCmd),
    /// UDP bridge for friends over VPN. Without arguments: server side (PORT+1 -> server)
    Bridge {
        /// Friend side: local port to listen on
        listen: Option<u16>,
        /// Friend side: server bridge address (VPN_IP:PORT+1)
        to: Option<String>,
        /// Check that a server answers at IP:PORT (through a bridge too) and exit
        #[arg(long, value_name = "IP:PORT")]
        test: Option<String>,
        /// Print the clients and traffic of the running bridge as JSON and exit
        #[arg(long)]
        status: bool,
    },
    /// Generate the bot navigation mesh of the current map (workshop maps without .nav)
    Nav {
        /// Also regenerate maps that already have one (official maps have better hand-made ones)
        #[arg(long)]
        force: bool,
    },
    /// For tools/testkit: prints how to start a test instance. Prints the rcon password, so only for a pipe.
    #[command(hide = true)]
    TestInstance {
        #[arg(long)]
        port: u16,
    },
    /// The helper that keeps history and runs scheduled tasks
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Console aliases: list them, set one (`alias r mp_restartgame 1`) or delete one (`alias r --delete`)
    Alias {
        name: Option<String>,
        command: Vec<String>,
        #[arg(long)]
        delete: bool,
        #[arg(long)]
        json: bool,
    },
    /// Plugins on the server
    #[command(subcommand)]
    Plugins(PluginCmd),
    /// Is there a newer release? Asks GitHub, and installs nothing
    Updates {
        #[arg(long)]
        json: bool,
    },
    /// For the agent. Prints the rcon password, so it refuses to write to a terminal
    #[command(hide = true, subcommand)]
    Internal(Internal),
    /// Add or remove bots with the server running: bots +2 | bots -1
    Bots {
        #[arg(allow_hyphen_values = true)]
        delta: i32,
    },
    /// Restart the round (or end the warmup with --warmup)
    Round {
        #[arg(long)]
        warmup: bool,
    },
    /// Saved presets: list or delete <name>
    Presets { delete: Option<String> },
    /// Regenerate the skins list from the game files (after a game update)
    Skins,
    /// Print the config file path
    Config,
    /// Print shell completions: bash, zsh or fish
    Completions { shell: clap_complete::Shell },
}

#[derive(Subcommand)]
enum Ws {
    /// Search maps (no API key needed)
    Search { text: Vec<String> },
    /// Install by id (a map or a whole collection)
    Install { ids: Vec<u64> },
    /// Installed workshop maps
    List,
    /// Remove an installed map by its folder (id or `local`)
    Remove { dir: String },
    /// Import a local .bsp file
    Import { file: std::path::PathBuf },
}

#[derive(Subcommand)]
enum HubCmd {
    /// What the hub has; `--installed` only what is installed
    List {
        #[arg(long)]
        installed: bool,
        #[arg(long)]
        json: bool,
    },
    /// Search names, descriptions and tags
    Search { text: Vec<String> },
    /// Details of one item
    Info { id: String },
    /// Install items and what they need
    Install {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Update the installed items with a newer version
    Update,
    /// Remove installed items
    Remove {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Check item folders as the hub's CI does
    Check {
        #[arg(required = true)]
        dirs: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AgentAction {
    Start,
    Stop,
    Status,
    /// One request to the agent (status, players, sanctions, summary, tasks, task_history...), answered in JSON
    Rpc {
        method: String,
        /// The method's fields, as a JSON object
        #[arg(default_value = "{}")]
        fields: String,
    },
}

#[derive(Subcommand)]
enum PluginCmd {
    List {
        #[arg(long)]
        json: bool,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    /// Installs a .sp (compiled by `deploy`) or a .smx
    Install {
        file: std::path::PathBuf,
    },
    Reload {
        name: String,
    },
}

#[derive(Subcommand)]
enum Internal {
    AgentConfig,
}

#[derive(Subcommand)]
enum Backup {
    /// Make a backup now
    Create {
        /// Count it as automatic: only the newest few stay (`backup_keep`)
        #[arg(long)]
        auto: bool,
    },
    /// List backups, newest first
    List,
    /// Put back the files of a backup; it saves the current ones first
    Restore {
        file: PathBuf,
        /// Only show what would change
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum ModeFile {
    Export { mode: String },
    Import { file: std::path::PathBuf },
    Delete { mode: String },
}

fn hub_cmd(cfg: &Config, c: HubCmd) -> Result<()> {
    use csls_core::hub;
    let lang = csls_core::lang();
    let line = |it: &hub::Item| format!("{:<22} {:<7} {:<9} {}", it.id, it.kind.label(), it.version, it.name.get(lang));
    // deploy only when there is a server to deploy to
    let redeploy = || -> Result<()> {
        if cfg.install_dir().join("csgo/addons/sourcemod").is_dir() {
            if let Some(e) = deploy::deploy(cfg, false, true)?.error_text() {
                eprintln!("compile errors:\n{e}");
            }
        }
        Ok(())
    };
    match c {
        HubCmd::List { installed: true, json } => {
            let items: Vec<hub::Item> = hub::installed().into_iter().map(|i| i.item).collect();
            if json {
                println!("{}", serde_json::to_string(&items)?);
            } else {
                items.iter().for_each(|it| println!("{}", line(it)));
            }
        }
        HubCmd::List { installed: false, json } => {
            let ix = hub::fetch_index()?;
            if json {
                println!("{}", serde_json::to_string(&ix.items)?);
            } else {
                let have: Vec<String> = hub::installed().into_iter().map(|i| i.item.id).collect();
                for e in &ix.items {
                    let mark = if have.contains(&e.item.id) { "*" } else { " " };
                    println!("{mark} {}", line(&e.item));
                }
            }
        }
        HubCmd::Search { text } => {
            let q = text.join(" ").to_lowercase();
            for e in hub::fetch_index()?.items {
                let it = &e.item;
                let hay =
                    format!("{} {} {} {} {}", it.id, it.name.en, it.name.es, it.description.get(lang), it.tags.join(" ")).to_lowercase();
                if hay.contains(&q) {
                    println!("{}", line(it));
                }
            }
        }
        HubCmd::Info { id } => {
            let ix = hub::fetch_index()?;
            let e = ix.get(&id)?;
            let it = &e.item;
            println!("{} ({} {}) by {}, {}", it.name.get(lang), it.kind.label(), it.version, it.author, it.license);
            println!("{}", it.description.get(lang));
            if !it.requires.is_empty() {
                println!("Needs: {}", it.requires.join(", "));
            }
            if e.has_code {
                println!("It brings SourceMod plugin code, compiled and run on your server.");
            }
        }
        HubCmd::Install { ids } => {
            let ix = hub::fetch_index()?;
            for id in ids {
                for done in hub::install(&ix, &id)? {
                    println!("Installed {done}");
                }
            }
            redeploy()?;
        }
        HubCmd::Update => {
            let done = hub::update_all(&hub::fetch_index()?)?;
            if done.is_empty() {
                println!("Everything is up to date.");
            } else {
                println!("Updated {}", done.join(", "));
                redeploy()?;
            }
        }
        HubCmd::Remove { ids } => {
            for id in ids {
                hub::remove(&id)?;
                println!("Removed {id}");
            }
            redeploy()?;
        }
        HubCmd::Check { dirs } => {
            let mut bad = 0;
            for d in dirs {
                match hub::check_dir(&d) {
                    Ok(it) => println!("ok   {}", it.id),
                    Err(e) => {
                        bad += 1;
                        println!("FAIL {}: {e:#}", d.display());
                    }
                }
            }
            if bad > 0 {
                bail!("{bad} item(s) failed");
            }
        }
    }
    Ok(())
}

fn watch_loop(cfg: &Config) -> Result<()> {
    // shortcut: polls every 2 s instead of inotify (no dependencies); deploy only compiles what changed
    let mut last = String::new();
    loop {
        let r = deploy::deploy(cfg, false, true)?;
        let msg = match r.error_text() {
            Some(e) => format!("compile errors:\n{e}"),
            None if !r.compiled.is_empty() => {
                format!("Compiled{}: {}", if r.reloaded { " and reloaded" } else { "" }, r.compiled.join(", "))
            }
            None if !r.removed.is_empty() => format!("Removed: {}", r.removed.join(", ")),
            None => String::new(),
        };
        if !msg.is_empty() && msg != last {
            println!("{msg}");
        }
        last = msg;
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn options(g: &Game, cfg: &Config) -> Result<LaunchOptions> {
    let mut o = match &g.preset {
        Some(p) => csls_core::presets::Presets::load()?.get(p)?.clone(),
        None => LaunchOptions::from_config(cfg),
    };
    if let Some(m) = &g.mode {
        o.mode = m.clone();
    }
    if let Some(h) = &g.humans {
        o.humans = if h == "alone" { 1 } else { h.parse().with_context(|| format!("humans must be a number or `alone`, got '{h}'"))? };
    }
    if g.map.is_some() {
        o.map = g.map.clone();
    }
    if let Some(k) = &g.kind {
        o.kind =
            Kind::parse(k).context("--type is competitive, short, casual, wingman, dm, ffa, tdm, armsrace, demolition, guardian or dz")?;
    }
    if let Some(d) = g.difficulty {
        o.difficulty = d;
    }
    o.bots = if g.no_bots { Some(0) } else { g.bots.or(o.bots) };
    o.no_draw |= g.no_draw;
    o.rounds = g.rounds.or(o.rounds);
    if let Some(s) = &g.site {
        o.site = Some(Site::parse(s).context("--site is a, b or random")?);
    }
    if let Some(t) = &g.teams {
        o.teams = Some(Teams::parse(t).context("--teams is together, versus or free")?);
    }
    o.precision |= g.precision;
    o.anarchy |= g.anarchy;
    o.workshop = g.workshop.or(o.workshop);
    for s in &g.set {
        o.cvars.push(parse_cvar(s)?);
    }
    if let Some(n) = &g.save_preset {
        csls_core::presets::Presets::put(n, &o)?;
        println!("Saved preset '{n}'.");
    }
    Ok(o)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Before anything reads the config: packaging runs this with no home.
    if let Cmd::Completions { shell } = cli.cmd {
        clap_complete::generate(shell, &mut Cli::command(), "csgo-legacy-server", &mut std::io::stdout());
        return Ok(());
    }
    let cfg = Config::load()?;
    let srv = Server::new(&cfg);
    let lang = csls_core::lang();
    match cli.cmd {
        Cmd::Install { update } => {
            let cancel = AtomicBool::new(false);
            let mut last = String::new();
            // the app reads progress from this file (the install continues if it is closed)
            let pf = csls_core::server::install_progress_file();
            let r = install::install(&cfg, update, &cancel, &mut |p| {
                let _ = std::fs::write(&pf, format!("{}|{}", p.fraction.map(|f| f.to_string()).unwrap_or_default(), p.message));
                let m = match p.fraction {
                    Some(f) if p.step == "server" => format!("{} {:5.1}%", p.message, f * 100.0),
                    _ => p.message.clone(),
                };
                if m != last {
                    println!("==> {m}");
                    last = m;
                }
            });
            match &r {
                Ok(()) => std::fs::write(&pf, "done|")?,
                Err(e) => std::fs::write(&pf, format!("error|{e:#}"))?,
            }
            r?;
            println!("Ready. Start with: csgo-legacy-server start awp alone");
            println!("Players need CS2 with the \"csgo_legacy\" beta and connect with: connect {}", srv.connect_address());
        }
        Cmd::Doctor { report } => {
            if report {
                print!("{}", doctor::report(&cfg));
                return Ok(());
            }
            for c in doctor::run(&cfg) {
                let mark = match c.status {
                    doctor::Status::Ok => "ok",
                    doctor::Status::Problem => "!!",
                    doctor::Status::Info => "--",
                };
                println!("{mark} {}: {}", c.name, c.detail);
            }
        }
        Cmd::Play { target } => {
            let t: Vec<&str> = target.iter().map(String::as_str).collect();
            let addr = match t.as_slice() {
                ["cs2"] => {
                    steam_client::launch_cs2()?;
                    println!("Opening...");
                    return Ok(());
                }
                [] | ["csgo"] => None,
                ["local"] | ["csgo", "local"] => Some(srv.connect_address()),
                [a] | ["csgo", a] => Some(a.to_string()),
                _ => bail!("usage: play [cs2 | csgo] [IP:PORT | local]"),
            };
            let connecting = steam_client::launch_csgo(addr.as_deref())?;
            println!("Opening...");
            if let (Some(h), Some(a)) = (connecting, &addr) {
                println!("It joins {a} once the menu loads.");
                if h.join().unwrap_or(false) {
                    println!("Connecting.");
                } else {
                    bail!("the game did not join; if it is still open, type in its console: connect {a}");
                }
            }
        }
        Cmd::Start(g) => {
            let modes = Modes::load()?;
            let o = options(&g, &cfg)?;
            let s = srv.start(&modes, &o)?;
            println!("Mode {} on {}. Connect with: connect {}", o.mode, s.map, s.connect);
            for w in &s.warnings {
                eprintln!("warning: {w}");
            }
            if srv.public() {
                if let Some(p) = srv.public_address() {
                    println!("From the internet: connect {p}");
                }
            }
            if !s.deploy.compiled.is_empty() {
                println!("Compiled: {}", s.deploy.compiled.join(", "));
            }
            srv.wait_rcon(Duration::from_secs(90))?;
            println!("Server ready.");
        }
        Cmd::Stop => {
            srv.stop()?;
            println!("Server stopped.");
        }
        Cmd::Status { json } => {
            if json {
                println!("{}", srv.status_json()?);
                return Ok(());
            }
            if srv.state() == State::Stopped {
                if let Some(s) = csls_core::server::last_stop() {
                    println!("{}", s.describe());
                }
                println!("Stopped.");
                return Ok(());
            }
            let mut r = srv.rcon()?;
            let st = players::parse_status(&r.exec("status")?);
            let mode = r.exec("modo_nombre").unwrap_or_default();
            let mode = mode.split('"').nth(3).unwrap_or("?");
            println!("Running: {} | mode {mode} | map {} | {} humans, {} bots / {}", st.hostname, st.map, st.humans, st.bots, st.max);
            println!("Connect with: connect {}", srv.connect_address());
        }
        Cmd::Mode(g) => {
            if g.mode.is_none() {
                bail!("which mode? (see `csgo-legacy-server modes`)");
            }
            let modes = Modes::load()?;
            let o = options(&g, &cfg)?;
            if g.plan {
                o.validate(&modes, &cfg)?;
                let map = o.pick_map(&modes, &cfg, None, &srv.maps_dir())?;
                let mut commands = csls_core::server::mode_commands(&o, modes.get(&o.mode)?);
                commands.push(csls_core::server::load_command(&o, &map));
                println!("{}", serde_json::json!({ "map": map, "commands": commands }));
                return Ok(());
            }
            let map = srv.change(&modes, &o)?;
            println!("Mode {} on {map}. Settings: {}", o.mode, o.extra(modes.get(&o.mode)?));
        }
        Cmd::Modes => {
            let modes = Modes::load()?;
            for m in modes.0.values() {
                println!("{:<12} {:<11} {}", m.id, m.meta.category, m.meta.description.get(lang));
            }
            for p in &modes.1 {
                eprintln!("skipped: {p}");
            }
            if modes.0.is_empty() {
                eprintln!("No modes installed. Get them from the hub: csgo-legacy-server hub install pack-all");
            }
        }
        Cmd::Maps => {
            for m in csls_core::launch::installed_maps(&srv.maps_dir()) {
                println!("{m}");
            }
        }
        Cmd::Players => {
            let st = players::parse_status(&srv.rcon()?.exec("status")?);
            println!("{:>4}  {:<24} {:<20} {:>5}", "id", "name", "steamid", "ping");
            for p in st.players {
                println!("{:>4}  {:<24} {:<20} {:>5}", p.userid, p.name, p.steamid, p.ping);
            }
        }
        Cmd::Kick { userid, reason } => println!("{}", srv.rcon()?.exec(&players::kick_cmd(userid, &reason.join(" ")))?),
        Cmd::Ban { userid, minutes, reason } => {
            println!("{}", srv.rcon()?.exec(&players::ban_cmd(userid, minutes, &reason.join(" ")))?)
        }
        Cmd::Unban { steamid } => println!("{}", srv.rcon()?.exec(&players::unban_cmd(&steamid)?)?),
        Cmd::Rcon { commands } => {
            let mut r = srv.rcon()?;
            let aliases = csls_core::aliases::load()?;
            for c in commands {
                print!("{}", r.exec(&csls_core::aliases::expand(&aliases, &c))?);
            }
        }
        Cmd::Deploy { watch: true, .. } => watch_loop(&cfg)?,
        Cmd::Deploy { force, .. } => {
            let r = deploy::deploy(&cfg, force, true)?;
            if !r.compiled.is_empty() {
                println!("Compiled: {}", r.compiled.join(", "));
            }
            if let Some(e) = r.error_text() {
                bail!("compile errors:\n{e}");
            }
            println!("{}", if r.reloaded { "Deployed and reloaded." } else { "Deployed." });
        }
        Cmd::Bridge { test: Some(addr), .. } => {
            let a = addr.parse().context("expected IP:PORT")?;
            match net::a2s_info(a, std::time::Duration::from_secs(8)) {
                Some(name) => println!("The server answers: {name}"),
                None => bail!("no answer from {addr}"),
            }
        }
        Cmd::Bridge { status: true, .. } => {
            let s = bridge::read_stats().context("no bridge has published its numbers")?;
            println!("{}", serde_json::to_string(&s)?);
        }
        Cmd::Bridge { listen, to, .. } => {
            let log = |m: String| println!("{m}");
            let max = cfg.server.bridge_max_clients as usize;
            match (listen, to) {
                (Some(p), Some(t)) => {
                    let target = t.parse().context("expected VPN_IP:PORT")?;
                    bridge::run(p, target, "0.0.0.0".parse()?, bridge::Limits::new(max, bridge::friend_allow()), log)?;
                }
                (None, None) => {
                    let ip = net::local_ip().context("no local IP")?;
                    let port = cfg.server.port;
                    let allow = if cfg.server.bridge_allow.is_empty() {
                        bridge::default_server_allow()
                    } else {
                        cfg.server.bridge_allow.iter().map(|n| bridge::Cidr::parse(n)).collect::<Result<_>>()?
                    };
                    bridge::run(port + 1, (ip, port).into(), ip, bridge::Limits::new(max, allow), log)?;
                }
                _ => bail!("friend side needs both: bridge <listen-port> <VPN_IP:PORT>"),
            }
        }
        Cmd::Workshop(w) => {
            let maps = srv.maps_dir();
            match w {
                Ws::Search { text } => {
                    for it in workshop::search(&text.join(" "), None, workshop::Sort::Trend, 1)? {
                        let c = match it.compat() {
                            workshop::Compat::Csgo => "csgo",
                            workshop::Compat::Cs2 => "CS2!",
                            _ => "?",
                        };
                        println!("{:>11}  {c:<4}  {:>6} MB  {}", it.id, it.file_size >> 20, it.title);
                    }
                }
                Ws::Install { ids } => {
                    for it in workshop::details(&ids)? {
                        let items = if it.collection { workshop::details(&workshop::collection(it.id)?)? } else { vec![it] };
                        for it in items {
                            match workshop::install(&maps, &it) {
                                Ok(m) => println!("Installed {} -> {m}", it.title),
                                Err(e) => println!("Skipped: {e:#}"),
                            }
                        }
                    }
                }
                Ws::List => {
                    for i in workshop::installed(&maps) {
                        println!(
                            "{:<40} {:<12} {}{}",
                            i.map,
                            i.dir,
                            i.title,
                            if i.has_nav { "" } else { "  (no .nav: load it and run `nav`)" }
                        );
                    }
                }
                Ws::Remove { dir } => workshop::remove(&maps, &dir)?,
                Ws::Import { file } => println!("Imported as {}", workshop::import_local(&maps, &file)?),
            }
        }
        Cmd::Backup(b) => match b {
            Backup::Create { auto } => {
                let out = if auto { backup::create_auto(&cfg)? } else { backup::create(&cfg)? };
                println!("{}", out.display());
            }
            Backup::List => {
                for p in backup::list()? {
                    println!("{}", p.display());
                }
            }
            Backup::Restore { file, dry_run } => {
                let changes = if dry_run { backup::preview(&cfg, &file)? } else { backup::restore(&cfg, &file)? };
                for c in &changes {
                    println!("{} {}", if c.new { "new    " } else { "replace" }, c.path.display());
                }
                println!("{} file(s) {}", changes.len(), if dry_run { "would change" } else { "restored" });
            }
        },
        Cmd::ModeFile(m) => {
            let dir = csls_core::paths::user_modes_dir();
            match m {
                ModeFile::Export { mode } => print!("{}", modes::export_mode(Modes::load()?.get(&mode)?)?),
                ModeFile::Import { file } => println!("Imported mode {}", modes::import_mode(&dir, &std::fs::read_to_string(file)?)?),
                ModeFile::Delete { mode } => modes::delete_user_mode(&dir, &mode)?,
            }
        }
        Cmd::Hub(c) => hub_cmd(&cfg, c)?,
        Cmd::Nav { force } => {
            println!("Generating .nav (seconds to minutes)...");
            let m = srv.generate_nav(Duration::from_secs(600), force)?;
            println!("Saved {m}.nav and reloaded the map.");
        }
        Cmd::Agent { action } => match action {
            AgentAction::Start => {
                if !csls_core::server::start_agent()? {
                    bail!("the agent is not installed next to this program");
                }
            }
            AgentAction::Stop => csls_core::server::stop_agent(),
            AgentAction::Status => {
                println!("{}", if csls_core::server::agent_running() { "Running." } else { "Stopped." });
            }
            AgentAction::Rpc { method, fields } => {
                let extra = serde_json::from_str(&fields).context("the fields must be a JSON object")?;
                println!("{}", csls_core::agent::call(&method, extra)?);
            }
        },
        Cmd::Alias { name: None, json, .. } => {
            let list = csls_core::aliases::load()?;
            if json {
                println!("{}", serde_json::to_string(&list)?);
            } else {
                for (k, v) in list {
                    println!("{k} = {v}");
                }
            }
        }
        Cmd::Alias { name: Some(name), delete: true, .. } => csls_core::aliases::remove(&name)?,
        Cmd::Alias { name: Some(name), command, .. } => csls_core::aliases::set(&name, &command.join(" "))?,
        Cmd::Plugins(c) => match c {
            PluginCmd::List { json } => {
                let list = csls_core::plugins::list(&cfg)?;
                if json {
                    println!("{}", serde_json::to_string(&list)?);
                } else {
                    for p in list {
                        let state = if p.enabled { "on" } else { "off" };
                        println!("{:<28} {state:<4} {:<8} {} {}", p.name, p.origin, p.title, p.version);
                    }
                }
            }
            PluginCmd::Enable { name } => csls_core::plugins::set_enabled(&cfg, &name, true)?,
            PluginCmd::Disable { name } => csls_core::plugins::set_enabled(&cfg, &name, false)?,
            PluginCmd::Install { file } => println!("{}", csls_core::plugins::install(&cfg, &file)?),
            PluginCmd::Reload { name } => print!("{}", srv.rcon()?.exec(&csls_core::plugins::reload_command(&name)?)?),
        },
        Cmd::Updates { json } => {
            let r = csls_core::updates::latest()?;
            let current = env!("CARGO_PKG_VERSION");
            let newer = csls_core::updates::is_newer(&r.version, current);
            if json {
                println!("{}", serde_json::json!({ "current": current, "latest": r.version, "newer": newer, "page": r.page }));
            } else if newer {
                println!("{} is out (this is {current}): {}", r.version, r.page);
            } else {
                println!("Up to date ({current}).");
            }
        }
        Cmd::Internal(Internal::AgentConfig) => {
            if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                bail!("this prints the rcon password: it is only for the agent");
            }
            println!("{}", srv.agent_config_json()?);
        }
        Cmd::TestInstance { port } => println!("{}", srv.test_instance_json(&Modes::load()?, port)?),
        Cmd::Bots { delta } => println!("bot_quota {}", srv.add_bots(delta)?),
        Cmd::Round { warmup } => {
            if warmup {
                srv.end_warmup()?
            } else {
                srv.restart_round()?
            }
            println!("OK");
        }
        Cmd::Presets { delete } => match delete {
            Some(n) => {
                csls_core::presets::Presets::remove(&n)?;
                println!("Deleted preset '{n}'.");
            }
            None => {
                for (n, o) in csls_core::presets::Presets::load()?.0 {
                    println!("{n}: {} {}", o.mode, o.map.as_deref().unwrap_or(""));
                }
            }
        },
        Cmd::Skins => {
            let (w, k, n) = csls_core::skins_gen::write_catalogs(&srv.csgo_dir())?;
            println!("{w} weapons, {k} knives, {n} skins -> {}", csls_core::paths::data_dir().display());
            deploy::deploy(&cfg, false, true)?;
        }
        Cmd::Config => println!("{}", csls_core::config::config_path().display()),
        Cmd::Completions { .. } => unreachable!("handled before loading the config"),
    }
    Ok(())
}
