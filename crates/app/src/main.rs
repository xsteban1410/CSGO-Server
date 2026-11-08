//! Desktop app for csgo-legacy-server.
mod backups_tab;
mod console;
mod headless;
mod i18n;
mod join;
mod modes_tab;
mod panel;
mod players;
mod plugins_tab;
mod schedule_tab;
mod settings;
mod setup;
mod single;
mod stats_tab;
mod tray;
mod updates_tab;
mod wizard;
mod workshop_tab;

use csls_core::config::Config;
use csls_core::modes::Modes;
use csls_core::players::{parse_status, Status};
use csls_core::server::{Server, State};
pub use i18n::t;
use i18n::t_msg;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

pub const POLL_BEGIN: &str = "csls_poll_begin";
pub const POLL_END: &str = "csls_poll_end";

/// Server state read over rcon every few seconds.
#[derive(Clone, Default, PartialEq)]
pub struct Live {
    pub running: bool,
    pub rcon_ok: bool,
    pub status: Status,
    pub mode: String,
    /// VPN profile: is the bridge running?
    pub bridge: bool,
    pub stats: Option<csls_core::server::ProcStats>,
    pub last_error: Option<String>,
}

/// The agent already asks the server every few seconds: reading its answer spares the rcon and `/proc` work.
fn fetch_from_agent(cfg: &Config) -> Option<Live> {
    let v = csls_core::agent::call("status", serde_json::json!({})).ok()?;
    let running = v["running"].as_bool()?;
    let s = &v["server"];
    let text = |x: &serde_json::Value| x.as_str().unwrap_or("").to_string();
    let num = |x: &serde_json::Value| x.as_u64().unwrap_or(0) as u32;
    let players = s["players"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| csls_core::players::Player {
            userid: num(&p["userid"]),
            name: text(&p["name"]),
            steamid: text(&p["steamid"]),
            bot: p["bot"].as_bool().unwrap_or(false),
            connected: text(&p["connected"]),
            ping: num(&p["ping"]),
            loss: 0,
            addr: String::new(),
        })
        .collect();
    Some(Live {
        running,
        rcon_ok: v["rcon_ok"].as_bool().unwrap_or(false),
        status: Status {
            hostname: text(&s["hostname"]),
            map: text(&s["map"]),
            humans: num(&s["humans"]),
            bots: num(&s["bots"]),
            max: num(&s["max"]),
            players,
        },
        mode: text(&v["mode"]),
        bridge: running && csls_core::server::bridge_running(),
        stats: running.then(|| csls_core::server::ProcStats {
            cpu_percent: v["cpu"].as_f64().unwrap_or(0.0) as u32,
            rss_mb: v["rss_kb"].as_u64().unwrap_or(0) / 1024,
            uptime_secs: v["uptime"].as_u64().unwrap_or(0),
        }),
        last_error: if running { csls_core::doctor::last_error(cfg) } else { None },
    })
}

/// Starts the agent, if it is installed. Without it the app works the same, without history or tasks.
pub fn start_agent(a: &AppRc) {
    let app = a.clone();
    bg(csls_core::server::start_agent, move |r| {
        if let Err(e) = r {
            eprintln!("agent: {e:#}");
        }
        app.agent_changed();
    });
}

/// After a config change: the agent reads the config once, when it starts.
pub fn restart_agent(a: &AppRc) {
    if csls_core::server::agent_running() {
        csls_core::server::stop_agent();
        start_agent(a);
    }
}

fn fetch(cfg: &Config) -> Live {
    if let Some(l) = fetch_from_agent(cfg) {
        return l;
    }
    let srv = Server::new(cfg);
    let mut l = Live { running: srv.state() == State::Running, ..Default::default() };
    if l.running {
        l.bridge = csls_core::server::bridge_running();
        l.stats = csls_core::server::process_stats(cfg.server.port);
        l.last_error = csls_core::doctor::last_error(cfg);
        if let Ok(mut r) = srv.rcon() {
            // srcds writes output to console.log: markers so the console can hide it
            let _ = r.exec(&format!("echo {POLL_BEGIN}"));
            if let Ok(s) = r.exec("status") {
                l.rcon_ok = true;
                l.status = parse_status(&s);
            }
            if let Ok(m) = r.exec("modo_nombre") {
                l.mode = m.split('"').nth(3).unwrap_or("").to_string();
            }
            let _ = r.exec(&format!("echo {POLL_END}"));
        }
    }
    l
}

type LiveListener = Box<dyn Fn(&Live)>;
type Answer = Box<dyn FnOnce(bool)>;

pub struct App {
    pub ui: AppWindow,
    pub cfg: RefCell<Config>,
    pub modes: RefCell<Modes>,
    pub live: RefCell<Live>,
    listeners: RefCell<Vec<LiveListener>>,
    modes_listeners: RefCell<Vec<Box<dyn Fn()>>>,
    agent_listeners: RefCell<Vec<Box<dyn Fn()>>>,
    polling: Cell<bool>,
    /// The user asked to stop, so the next stop is no surprise.
    pub stop_requested: Cell<bool>,
    answer: RefCell<Option<Answer>>,
    toast_timer: Timer,
    deploy_timer: Timer,
    timers: RefCell<Vec<Timer>>,
    /// Filled by each tab that needs its own state.
    pub wizard: RefCell<wizard::State>,
    pub editor: RefCell<modes_tab::State>,
}

pub type AppRc = Rc<App>;

impl App {
    pub fn s(&self) -> S<'_> {
        self.ui.global::<S>()
    }

    pub fn toast(self: &Rc<Self>, msg: &str) {
        self.s().set_toast(msg.into());
        let app = Rc::downgrade(self);
        self.toast_timer.start(TimerMode::SingleShot, Duration::from_secs(5), move || {
            if let Some(a) = app.upgrade() {
                a.s().set_toast("".into());
            }
        });
    }

    /// A question or a notice. Without `no` there is one button, and the answer is always true.
    pub fn ask(&self, title: &str, text: &str, yes: &str, no: &str, danger: bool, answer: impl FnOnce(bool) + 'static) {
        let s = self.s();
        s.set_alert_title(title.into());
        s.set_alert_text(text.into());
        s.set_alert_yes(yes.into());
        s.set_alert_no(no.into());
        s.set_alert_danger(danger);
        *self.answer.borrow_mut() = Some(Box::new(answer));
        s.set_alert_open(true);
    }

    pub fn warn(&self, msg: &str) {
        let text = msg.lines().map(t_msg).collect::<Vec<_>>().join("\n");
        self.ask(&t("Warning"), &text, "OK", "", false, |_| {});
    }

    pub fn error(&self, e: &anyhow::Error) {
        self.ask(&t("Error"), &t_msg(&format!("{e:#}")), "OK", "", false, |_| {});
    }

    pub fn copy(self: &Rc<Self>, text: String) {
        let r = arboard::Clipboard::new().and_then(|mut c| c.set_text(text));
        match r {
            Ok(()) => self.toast(&t("Copied")),
            Err(e) => self.error(&anyhow::anyhow!("{e}")),
        }
    }

    pub fn on_live(&self, f: impl Fn(&Live) + 'static) {
        f(&self.live.borrow());
        self.listeners.borrow_mut().push(Box::new(f));
    }

    /// Reads the state again now (after start/stop/change).
    pub fn refresh(self: &Rc<Self>) {
        if self.polling.replace(true) {
            return;
        }
        let cfg = self.cfg.borrow().clone();
        let app = self.clone();
        bg(
            move || fetch(&cfg),
            move |l| {
                app.polling.set(false);
                if app.live.borrow().running && !l.running && !app.stop_requested.replace(false) {
                    app.toast(&t("The server has stopped. The Console tab may say why"));
                }
                if *app.live.borrow() != l {
                    *app.live.borrow_mut() = l;
                    for f in app.listeners.borrow().iter() {
                        f(&app.live.borrow());
                    }
                }
            },
        );
    }

    /// Whoever reads from the agent asks again.
    pub fn on_agent_changed(&self, f: impl Fn() + 'static) {
        self.agent_listeners.borrow_mut().push(Box::new(f));
    }

    pub fn agent_changed(&self) {
        for f in self.agent_listeners.borrow().iter() {
            f();
        }
    }

    pub fn on_modes_changed(&self, f: impl Fn() + 'static) {
        self.modes_listeners.borrow_mut().push(Box::new(f));
    }

    /// Tells whoever shows modes or presets to redraw.
    pub fn modes_changed(&self) {
        for f in self.modes_listeners.borrow().iter() {
            f();
        }
    }

    /// Keeps a repeating timer alive with the app.
    pub fn every(self: &Rc<Self>, period: Duration, f: impl FnMut() + 'static) {
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, period, f);
        self.timers.borrow_mut().push(timer);
    }

    /// Watches the mode and plugin folders: on change it recompiles/copies and hot-reloads.
    fn watch(self: &Rc<Self>) {
        let mut dirs = vec![csls_core::paths::user_modes_dir(), csls_core::paths::user_plugins_dir()];
        if let Ok(r) = csls_core::paths::resources_dir() {
            dirs.push(r.join("plugin"));
            dirs.push(r.join("cfg"));
        }
        for d in &dirs {
            let _ = std::fs::create_dir_all(d);
        }
        let app = Rc::downgrade(self);
        let state = Rc::new(RefCell::new(None));
        // shortcut: polls the folders every 2 s; inotify would need a dependency for a few small folders
        self.every(Duration::from_secs(2), move || {
            let (dirs, app, state) = (dirs.clone(), app.clone(), state.clone());
            bg(
                move || signature(&dirs),
                move |sig| {
                    let old = state.replace(Some(sig));
                    if old.is_some() && old != *state.borrow() {
                        if let Some(a) = app.upgrade() {
                            a.schedule_deploy();
                        }
                    }
                },
            );
        });
    }

    /// Groups bursts of changes (editors save several times) and deploys once.
    fn schedule_deploy(self: &Rc<Self>) {
        let app = Rc::downgrade(self);
        self.deploy_timer.start(TimerMode::SingleShot, Duration::from_millis(1200), move || {
            let Some(a) = app.upgrade() else { return };
            a.reload_modes();
            a.modes_changed();
            let cfg = a.cfg.borrow().clone();
            if !Server::new(&cfg).is_installed() {
                return;
            }
            bg(
                move || csls_core::deploy::deploy(&cfg, false, true),
                move |r| match r {
                    Ok(r) => {
                        if let Some(e) = r.error_text() {
                            a.error(&anyhow::anyhow!("{}\n{e}", t("Plugins that do not compile (the server keeps the previous version):")));
                        } else if !r.compiled.is_empty() {
                            a.toast(&format!(
                                "{}: {}",
                                if r.reloaded { t("Compiled and reloaded") } else { t("Compiled") },
                                r.compiled.join(", ")
                            ));
                        } else if !r.removed.is_empty() {
                            a.toast(&format!("{}: {}", t("Removed"), r.removed.join(", ")));
                        }
                    }
                    Err(e) => a.error(&e),
                },
            );
        });
    }

    pub fn reload_modes(&self) {
        match Modes::load() {
            Ok(m) => *self.modes.borrow_mut() = m,
            Err(e) => self.error(&e),
        }
    }
}

/// Names and times of the source files in some folders.
fn signature(dirs: &[std::path::PathBuf]) -> Vec<(String, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = dirs.to_vec();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if ["sp", "inc", "cfg", "toml", "txt"].iter().any(|x| p.extension().is_some_and(|y| y == *x)) {
                if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                    out.push((p.to_string_lossy().into_owned(), m));
                }
            }
        }
    }
    out.sort();
    out
}

type Done = Box<dyn FnOnce(Box<dyn Any + Send>)>;

thread_local! {
    static PENDING: RefCell<HashMap<u64, Done>> = RefCell::default();
    static NEXT: Cell<u64> = const { Cell::new(0) };
}

/// Runs `f` on a thread and then `done` on the UI thread.
pub fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    let id = NEXT.with(|n| n.replace(n.get() + 1));
    let boxed = Box::new(move |v: Box<dyn Any + Send>| done(*v.downcast::<T>().expect("same type")));
    PENDING.with(|p| p.borrow_mut().insert(id, boxed));
    std::thread::spawn(move || {
        let v: Box<dyn Any + Send> = Box::new(f());
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(done) = PENDING.with(|p| p.borrow_mut().remove(&id)) {
                done(v);
            }
        });
    });
}

pub fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

pub fn strings(v: &[String]) -> ModelRc<SharedString> {
    model(v.iter().map(|s| SharedString::from(s.as_str())).collect())
}

/// A moment in local time. shortcut: asks `date` for the offset once, so a change of clock while the app is open shows the old one.
pub fn when(unix: i64) -> String {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    let offset = *OFFSET.get_or_init(|| {
        let o = std::process::Command::new("date").arg("+%z").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        o.ok().and_then(|z| parse_offset(&z)).unwrap_or(0)
    });
    if unix <= 0 {
        return "—".into();
    }
    civil(unix + offset)
}

fn parse_offset(z: &str) -> Option<i64> {
    let (sign, rest) = z.split_at_checked(1)?;
    let (h, m) = rest.split_at_checked(2)?;
    let secs = h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60;
    Some(if sign == "-" { -secs } else { secs })
}

/// `YYYY-MM-DD HH:MM` from seconds since 1970, by the days-from-civil algorithm.
fn civil(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, secs / 60 % 60)
}

pub fn duration(secs: i64) -> String {
    if secs >= 3600 {
        format!("{} h {} min", secs / 3600, secs / 60 % 60)
    } else {
        format!("{} min", secs / 60)
    }
}

/// Bars for the charts, scaled to the largest value.
pub fn bars(list: &[csls_core::agent::Count]) -> Vec<Bar> {
    let max = list.iter().map(|c| c.count).max().unwrap_or(0).max(1) as f32;
    list.iter().map(|c| Bar { label: c.name.as_str().into(), value: c.count.to_string().into(), frac: c.count as f32 / max }).collect()
}

/// Asks where to save and writes there.
pub fn save_as(a: &AppRc, name: String, content: String) {
    let a = a.clone();
    bg(
        move || -> anyhow::Result<bool> {
            let Some(p) = rfd::FileDialog::new().set_file_name(name).save_file() else { return Ok(false) };
            std::fs::write(p, content)?;
            Ok(true)
        },
        move |r| match r {
            Ok(true) => a.toast(&t("Saved")),
            Ok(false) => {}
            Err(e) => a.error(&e),
        },
    );
}

pub fn btn(id: &str, label: &str) -> Btn {
    Btn { id: id.into(), label: label.into(), danger: false }
}

pub fn row(id: &str, title: &str, subtitle: &str, buttons: Vec<Btn>) -> ActionRow {
    ActionRow { id: id.into(), title: title.into(), subtitle: subtitle.into(), buttons: model(buttons) }
}

fn pick_language() {
    if csls_core::lang() == "es" {
        // the English text in the code is the base language
        let _ = slint::select_bundled_translation("es");
    }
}

fn build(cfg: Config) -> anyhow::Result<AppRc> {
    let ui = AppWindow::new()?;
    pick_language();
    let a = Rc::new(App {
        ui,
        modes: RefCell::new(Modes::load().unwrap_or_default()),
        cfg: RefCell::new(cfg),
        live: RefCell::default(),
        listeners: RefCell::default(),
        modes_listeners: RefCell::default(),
        agent_listeners: RefCell::default(),
        polling: Cell::new(false),
        stop_requested: Cell::new(false),
        answer: RefCell::default(),
        toast_timer: Timer::default(),
        deploy_timer: Timer::default(),
        timers: RefCell::default(),
        wizard: RefCell::default(),
        editor: RefCell::default(),
    });
    let ans = a.clone();
    a.s().on_alert_answer(move |yes| {
        if let Some(f) = ans.answer.borrow_mut().take() {
            f(yes);
        }
    });
    panel::init(&a);
    setup::init(&a);
    wizard::init(&a);
    players::init(&a);
    modes_tab::init(&a);
    workshop_tab::init(&a);
    console::init(&a);
    join::init(&a);
    settings::init(&a);
    plugins_tab::init(&a);
    schedule_tab::init(&a);
    backups_tab::init(&a);
    stats_tab::init(&a);
    updates_tab::init(&a);
    a.watch();
    let poll = a.clone();
    a.every(Duration::from_secs(3), move || poll.refresh());
    a.refresh();
    Ok(a)
}

fn debug_open(a: &AppRc) {
    if let Some(i) = std::env::var("CSLS_TAB").ok().and_then(|t| tab_index(&t)) {
        a.s().set_tab(i);
    }
    if let Ok(id) = std::env::var("CSLS_OPEN_EDITOR") {
        modes_tab::debug_open_editor(a, &id);
    }
    // CSLS_OPEN_WIZARD=1, or mode, mode:map or mode:map:next to reach each step
    if let Ok(path) = std::env::var("CSLS_OPEN_WIZARD") {
        wizard::debug_open(a, &path);
    }
}

const APP_ID: &str = "io.github.xsteban1410.CsgoLegacyServer";

fn tab_index(name: &str) -> Option<i32> {
    ["server", "players", "modes", "workshop", "console", "plugins", "tasks", "backups", "stats", "updates", "join", "settings"]
        .iter()
        .position(|t| *t == name)
        .map(|i| i as i32)
}

/// Debug: CSLS_SCREENSHOT=file.png draws the app after a moment, with no window on screen, and closes.
fn run_screenshot(path: std::ffi::OsString, cfg: Config) -> anyhow::Result<()> {
    let window = headless::install();
    let a = build(cfg)?;
    a.ui.show()?;
    debug_open(&a);
    let delay = std::env::var("CSLS_SCREENSHOT_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(2);
    Timer::single_shot(Duration::from_secs(delay), move || {
        if let Err(e) = headless::save(&window, std::path::Path::new(&path)) {
            eprintln!("screenshot: {e}");
        }
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop()?;
    drop(a);
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    if let Some(path) = std::env::var_os("CSLS_SCREENSHOT") {
        return run_screenshot(path, cfg);
    }
    // a second launch shows the first window and leaves
    let Some(listener) = single::claim()? else { return Ok(()) };
    let a = build(cfg)?;
    // docks and window lists find the app's name and icon through this, which must match its .desktop file
    slint::set_xdg_app_id(APP_ID)?;
    start_agent(&a);
    single::serve(listener, a.ui.as_weak());
    let quit_on_close = !tray::available();
    let tray_icon = if quit_on_close { None } else { Some(tray::init(&a)?) };
    if !quit_on_close {
        // closing the window leaves the app in the tray, where it can be opened again
        a.ui.window().on_close_requested(|| slint::CloseRequestResponse::HideWindow);
    }
    a.ui.show()?;
    debug_open(&a);
    if quit_on_close {
        slint::run_event_loop()?;
    } else {
        slint::run_event_loop_until_quit()?;
    }
    drop(tray_icon);
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        // before the window exists, or when it cannot open: this is the only place left to say it
        eprintln!("{}", t_msg(&format!("{e:#}")));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn the_app_id_names_its_desktop_file() {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../packaging/{APP_ID}.desktop"));
        assert!(file.is_file(), "{}", file.display());
    }

    #[test]
    fn dates_and_bars() {
        assert_eq!(civil(0), "1970-01-01 00:00");
        assert_eq!(civil(1_788_739_260), "2026-09-07 00:01");
        assert_eq!(parse_offset("-0130"), Some(-5400));
        assert_eq!(parse_offset("+0200"), Some(7200));
        assert_eq!(parse_offset(""), None);
        assert_eq!(when(0), "—");
        assert_eq!(duration(3720), "1 h 2 min");
        let b = bars(&[csls_core::agent::Count { name: "a".into(), count: 2 }, csls_core::agent::Count { name: "b".into(), count: 1 }]);
        assert_eq!((b[0].frac, b[1].frac), (1.0, 0.5));
        assert!(bars(&[]).is_empty());
    }

    #[test]
    fn signature_sees_new_and_changed_sources_only() {
        let d = std::env::temp_dir().join(format!("csls-sig-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        let dirs = vec![d.clone()];
        let empty = signature(&dirs);
        std::fs::write(d.join("note.md"), "x").unwrap();
        assert_eq!(signature(&dirs), empty, "other files do not count");
        std::fs::write(d.join("sub/a.sp"), "x").unwrap();
        assert_ne!(signature(&dirs), empty);
        std::fs::remove_dir_all(d).unwrap();
    }

    /// The tabs and flows wired together, on temporary folders with the hub test items. Only one test does this: the
    /// environment is shared by the whole process.
    #[test]
    fn the_app_runs_its_flows() {
        i_slint_backend_testing::init_no_event_loop();
        let tmp = std::env::temp_dir().join(format!("csls-app-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", tmp.join("config"));
        std::env::set_var("XDG_DATA_HOME", tmp.join("data"));
        std::env::set_var("XDG_RUNTIME_DIR", tmp.join("run"));
        std::env::set_var("CSLS_RESOURCES", concat!(env!("CARGO_MANIFEST_DIR"), "/../../game"));
        std::env::set_var("CSLS_HUB_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/fixtures/hub"));
        let a = build(Config::default()).unwrap();
        let s = a.s();
        for tab in 0..12 {
            s.set_tab(tab);
        }
        assert_eq!(s.get_setup(), "welcome", "nothing installed yet");
        assert!(s.get_md_builtin().row_count() >= 9, "the modes tab shows the installed modes");
        assert_eq!(s.get_md_mine().row_count(), 0);

        // wizard: mode, then map, then what was chosen on the options page reaches the launch options
        wizard::open(&a, false);
        assert!(s.get_wiz_open());
        s.invoke_wiz_pick_mode("competitivo".into());
        assert_eq!(s.get_wiz_step(), 1);
        s.invoke_wiz_pick_map("de_dust2".into());
        assert_eq!(s.get_wiz_step(), 2);
        assert!(s.get_wiz_nodraw_show(), "competitive has the no-draw option");
        s.set_wiz_no_bots(true);
        s.set_wiz_nodraw(true);
        s.invoke_wiz_options_next();
        assert_eq!(s.get_wiz_step(), 3, "the options are valid: {}", s.get_alert_text());
        let o = a.wizard.borrow().debug_options().unwrap();
        assert_eq!((o.mode.as_str(), o.map.as_deref(), o.bots, o.no_draw), ("competitivo", Some("de_dust2"), Some(0), true));
        s.set_wiz_open(false);

        // a bad cvar says so and stays on the page
        wizard::open(&a, false);
        s.invoke_wiz_pick_mode("competitivo".into());
        s.invoke_wiz_pick_map("".into());
        s.set_wiz_extra("sv_cheats=1;quit".into());
        s.invoke_wiz_options_next();
        assert_eq!(s.get_wiz_step(), 2);
        assert!(s.get_alert_open());
        s.invoke_alert_answer(true);
        s.set_alert_open(false);
        s.set_wiz_open(false);

        // editor: duplicating keeps the rules, and an injection is refused by the same check as a mode written by hand
        modes_tab::debug_open_editor(&a, "awp");
        assert!(s.get_ed_open() && !s.get_ed_editing());
        s.set_ed_extra("sv_cheats 1;quit".into());
        s.invoke_ed_save(false);
        assert!(s.get_ed_open() && s.get_alert_open(), "refused");
        s.set_alert_open(false);
        s.set_ed_extra("".into());
        s.invoke_ed_save(false);
        assert!(!s.get_ed_open(), "saved: {}", s.get_alert_text());
        assert_eq!(s.get_md_mine().row_count(), 1);
        let cfg = std::fs::read_to_string(csls_core::paths::user_modes_dir().join("awp_2.cfg")).unwrap();
        assert!(cfg.contains("modo_t_arma") && cfg.contains("weapon_awp"), "{cfg}");

        // deleting asks first
        s.invoke_md_mine_action("awp_2".into(), "del".into());
        assert_eq!(s.get_md_mine().row_count(), 1);
        s.invoke_alert_answer(true);
        assert_eq!(s.get_md_mine().row_count(), 0);

        // the console keeps secrets out of its history and shows only the command name
        s.set_con_entry("rcon_password hunter2".into());
        s.invoke_con_send();
        let shown: Vec<String> = s.get_con_lines().iter().map(|l| l.text.to_string()).collect();
        assert_eq!(shown, ["] rcon_password ***"]);
        s.invoke_con_history(-1);
        assert_eq!(s.get_con_entry(), "", "not in the history");

        // a second launch hands over to the first
        let first = single::claim().unwrap();
        assert!(first.is_some());
        assert!(single::claim().unwrap().is_none());
        drop(first);
        assert!(single::claim().unwrap().is_some(), "a socket left by a crash does not block the app");

        // tabs that read from the agent say it is not there, instead of failing
        assert!(s.get_ss_note() != "", "statistics without an agent");
        assert!(s.get_pl_note() != "", "history without an agent");
        assert!(s.get_sc_note() != "", "tasks without an agent");

        // aliases: added through the dialog and used by the console
        s.set_con_alias_name("r".into());
        s.set_con_alias_cmd("mp_restartgame 1".into());
        s.invoke_con_alias_add();
        assert_eq!(s.get_con_aliases().row_count(), 1);
        s.invoke_con_alias_action("r".into(), "del".into());
        assert_eq!(s.get_con_aliases().row_count(), 0);

        // settings persist
        s.set_set_name("Test".into());
        s.invoke_set_save();
        assert_eq!(Config::load().unwrap().server.hostname, "Test");
        drop(a);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
