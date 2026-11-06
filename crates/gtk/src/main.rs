//! App GTK de csgo-legacy-server.
mod console;
mod i18n;
mod join;
mod modes_tab;
mod panel;
mod players;
mod settings;
mod setup;
mod wizard;
mod workshop_tab;

use adw::prelude::*;
use csls_core::config::Config;
use csls_core::modes::Modes;
use csls_core::players::{parse_status, Status};
use csls_core::server::{Server, State};
use gtk::{gio, glib};
use i18n::t;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub const POLL_BEGIN: &str = "csls_poll_begin";
pub const POLL_END: &str = "csls_poll_end";
pub const APP_ID: &str = "io.github.xsteban1410.CsgoLegacyServer";

/// Estado del servidor leído por rcon cada pocos segundos.
#[derive(Clone, Default, PartialEq)]
pub struct Live {
    pub running: bool,
    pub rcon_ok: bool,
    pub status: Status,
    pub mode: String,
    /// Perfil VPN: ¿está el puente en marcha?
    pub bridge: bool,
}

fn fetch(cfg: &Config) -> Live {
    let srv = Server::new(cfg);
    let mut l = Live { running: srv.state() == State::Running, ..Default::default() };
    if l.running {
        l.bridge = csls_core::server::bridge_running();
        if let Ok(mut r) = srv.rcon() {
            // srcds escribe la salida en console.log: marcas para que la consola la oculte
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

pub struct App {
    pub win: adw::ApplicationWindow,
    pub toasts: adw::ToastOverlay,
    pub cfg: RefCell<Config>,
    pub modes: RefCell<Modes>,
    pub live: RefCell<Live>,
    listeners: RefCell<Vec<Box<dyn Fn(&Live)>>>,
    modes_listeners: RefCell<Vec<Box<dyn Fn()>>>,
    polling: Cell<bool>,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    deploy_pending: RefCell<Option<glib::SourceId>>,
}

pub type AppRc = Rc<App>;

impl App {
    pub fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::new(msg));
    }

    pub fn error(&self, e: &anyhow::Error) {
        let d = adw::AlertDialog::new(Some(&t("Error")), Some(&format!("{e:#}")));
        d.add_response("ok", "OK");
        d.present(Some(&self.win));
    }

    pub fn on_live(&self, f: impl Fn(&Live) + 'static) {
        f(&self.live.borrow());
        self.listeners.borrow_mut().push(Box::new(f));
    }

    /// Vuelve a leer el estado ya (tras iniciar/parar/cambiar).
    pub fn refresh(self: &Rc<Self>) {
        if self.polling.replace(true) {
            return;
        }
        let cfg = self.cfg.borrow().clone();
        let app = self.clone();
        bg(move || fetch(&cfg), move |l| {
            app.polling.set(false);
            if *app.live.borrow() != l {
                *app.live.borrow_mut() = l;
                for f in app.listeners.borrow().iter() {
                    f(&app.live.borrow());
                }
            }
        });
    }

    pub fn on_modes_changed(&self, f: impl Fn() + 'static) {
        self.modes_listeners.borrow_mut().push(Box::new(f));
    }

    /// Avisa a quien muestra modos o presets de que hay que redibujar.
    pub fn modes_changed(&self) {
        for f in self.modes_listeners.borrow().iter() {
            f();
        }
    }

    /// Vigila las carpetas de modos y plugins: al cambiar algo se recompila/copia y se recarga en caliente.
    fn watch(self: &Rc<Self>) {
        let mut dirs = vec![csls_core::paths::user_modes_dir(), csls_core::paths::user_plugins_dir()];
        if let Ok(r) = csls_core::paths::resources_dir() {
            dirs.push(r.join("plugin"));
            dirs.push(r.join("cfg"));
        }
        for d in dirs {
            let _ = std::fs::create_dir_all(&d);
            let Ok(m) = gio::File::for_path(&d).monitor_directory(gio::FileMonitorFlags::NONE, None::<&gio::Cancellable>) else { continue };
            let app = Rc::downgrade(self);
            m.connect_changed(move |_, f, _, ev| {
                if !matches!(ev, gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::Deleted | gio::FileMonitorEvent::MovedIn | gio::FileMonitorEvent::Created) {
                    return;
                }
                let n = f.basename().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
                if !(n.ends_with(".sp") || n.ends_with(".inc") || n.ends_with(".cfg") || n.ends_with(".toml") || n.ends_with(".txt")) {
                    return;
                }
                if let Some(a) = app.upgrade() {
                    a.schedule_deploy();
                }
            });
            self.monitors.borrow_mut().push(m);
        }
    }

    /// Agrupa ráfagas de cambios (los editores guardan varias veces) y despliega una vez.
    fn schedule_deploy(self: &Rc<Self>) {
        if let Some(id) = self.deploy_pending.borrow_mut().take() {
            id.remove();
        }
        let app = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
            let Some(a) = app.upgrade() else { return };
            a.deploy_pending.borrow_mut().take();
            a.reload_modes();
            a.modes_changed();
            let cfg = a.cfg.borrow().clone();
            if !Server::new(&cfg).is_installed() {
                return;
            }
            bg(move || csls_core::deploy::deploy(&cfg, false, true), move |r| match r {
                Ok(r) => {
                    if let Some(e) = r.error_text() {
                        a.error(&anyhow::anyhow!("{}\n{e}", t("Plugins that do not compile (the server keeps the previous version):")));
                    } else if !r.compiled.is_empty() {
                        a.toast(&format!("{}: {}", if r.reloaded { t("Compiled and reloaded") } else { t("Compiled") }, r.compiled.join(", ")));
                    } else if !r.removed.is_empty() {
                        a.toast(&format!("{}: {}", t("Removed"), r.removed.join(", ")));
                    }
                }
                Err(e) => a.error(&e),
            });
        });
        *self.deploy_pending.borrow_mut() = Some(id);
    }

    pub fn reload_modes(&self) {
        match Modes::load() {
            Ok(m) => *self.modes.borrow_mut() = m,
            Err(e) => self.error(&e),
        }
    }
}

/// Ejecuta `f` en un hilo y luego `done` en el hilo de la interfaz.
pub fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    glib::spawn_future_local(async move {
        if let Ok(v) = gio::spawn_blocking(f).await {
            done(v);
        }
    });
}

/// Botón con estilo.
pub fn button(label: &str, class: &[&str]) -> gtk::Button {
    let b = gtk::Button::with_label(label);
    for c in class {
        b.add_css_class(c);
    }
    b
}

fn build(app: &adw::Application) {
    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            let w = adw::ApplicationWindow::builder().application(app).build();
            let d = adw::AlertDialog::new(Some(&t("Error")), Some(&format!("{e:#}")));
            d.add_response("ok", "OK");
            d.connect_response(None, glib::clone!(#[weak] w, move |_, _| w.close()));
            w.present();
            d.present(Some(&w));
            return;
        }
    };
    let modes = Modes::load().unwrap_or_default();
    gtk::Window::set_default_icon_name(APP_ID);
    let win = adw::ApplicationWindow::builder().application(app).title("CS:GO Legacy Server").default_width(1040).default_height(700).build();
    let toasts = adw::ToastOverlay::new();
    let a = Rc::new(App {
        win: win.clone(),
        toasts: toasts.clone(),
        cfg: RefCell::new(cfg),
        modes: RefCell::new(modes),
        live: RefCell::new(Live::default()),
        listeners: RefCell::new(Vec::new()),
        modes_listeners: RefCell::new(Vec::new()),
        polling: Cell::new(false),
        monitors: RefCell::new(Vec::new()),
        deploy_pending: RefCell::new(None),
    });
    a.watch();

    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(&panel::page(&a), Some("server"), &t("Server"), "network-server-symbolic");
    stack.add_titled_with_icon(&players::page(&a), Some("players"), &t("Players"), "system-users-symbolic");
    stack.add_titled_with_icon(&modes_tab::page(&a), Some("modes"), &t("Modes"), "applications-games-symbolic");
    stack.add_titled_with_icon(&workshop_tab::page(&a), Some("workshop"), &t("Workshop"), "folder-download-symbolic");
    stack.add_titled_with_icon(&console::page(&a), Some("console"), &t("Console"), "utilities-terminal-symbolic");
    stack.add_titled_with_icon(&join::page(&a), Some("join"), &t("Join"), "input-gaming-symbolic");
    stack.add_titled_with_icon(&settings::page(&a), Some("settings"), &t("Settings"), "emblem-system-symbolic");

    let header = adw::HeaderBar::new();
    let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
    header.set_title_widget(Some(&switcher));
    let bar = adw::ViewSwitcherBar::builder().stack(&stack).build();
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(&stack));
    tv.add_bottom_bar(&bar);
    // en ventanas estrechas las pestañas bajan a la barra inferior
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(adw::BreakpointConditionLengthType::MaxWidth, 600.0, adw::LengthUnit::Sp));
    bp.add_setter(&switcher, "visible", Some(&false.to_value()));
    bp.add_setter(&bar, "reveal", Some(&true.to_value()));
    win.add_breakpoint(bp);

    toasts.set_child(Some(&tv));
    // sin servidor instalado: asistente de primer arranque encima de todo
    let root = gtk::Stack::new();
    root.add_named(&toasts, Some("main"));
    let need_setup = !Server::new(&a.cfg.borrow()).is_installed() || std::env::var_os("CSLS_FORCE_SETUP").is_some();
    if need_setup {
        root.add_named(&setup::page(&a, &root, &stack), Some("setup"));
        root.set_visible_child_name("setup");
    }
    win.set_content(Some(&root));

    glib::timeout_add_seconds_local(3, glib::clone!(#[strong] a, move || {
        a.refresh();
        glib::ControlFlow::Continue
    }));
    a.refresh();
    win.present();
    if let Ok(tab) = std::env::var("CSLS_TAB") {
        stack.set_visible_child_name(&tab);
    }
    if let Ok(id) = std::env::var("CSLS_OPEN_EDITOR") {
        modes_tab::debug_open_editor(&a, &id);
    }
    if std::env::var_os("CSLS_OPEN_WIZARD").is_some() {
        wizard::open(&a, false);
    }
    debug_screenshot(&win);
}

/// Depuración: CSLS_SCREENSHOT=archivo.png renderiza la ventana a los 4 s y cierra.
fn debug_screenshot(win: &adw::ApplicationWindow) {
    let Some(path) = std::env::var_os("CSLS_SCREENSHOT") else { return };
    let win = win.clone();
    let delay = std::env::var("CSLS_SCREENSHOT_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(4);
    glib::timeout_add_seconds_local_once(delay, move || {
        let p = gtk::WidgetPaintable::new(Some(&win));
        let snap = gtk::Snapshot::new();
        p.snapshot(&snap, win.width() as f64, win.height() as f64);
        let node = snap.to_node();
        if let (Some(node), Some(r)) = (node, win.native().and_then(|n| n.renderer())) {
            if let Err(e) = r.render_texture(node, None).save_to_png(&path) {
                eprintln!("screenshot: {e}");
            }
        }
        if let Some(app) = win.application() {
            app.quit();
        }
    });
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build);
    app.run()
}
