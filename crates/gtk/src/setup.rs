//! First run: create a server (choose folder, dependencies, background download) or just join.
use crate::i18n::{t, t_msg};
use crate::{button, AppRc};
use adw::prelude::*;
use csls_core::install;
use csls_core::server::{self, Server};
use gtk::{gio, glib};

fn free_text(dir: &std::path::Path) -> String {
    install::free_bytes(dir).map(|b| format!("{} GB", b >> 30)).unwrap_or_else(|| "?".into())
}

pub fn page(a: &AppRc, root: &gtk::Stack, tabs: &adw::ViewStack) -> gtk::Widget {
    let nav = adw::NavigationView::new();

    // 1. welcome
    let welcome = adw::StatusPage::builder().icon_name("network-server-symbolic").title(t("Welcome")).description(t("Set up a CS:GO Legacy server to play with friends and bots, or join one")).build();
    let create = button(&t("Create a server"), &["suggested-action", "pill"]);
    let join = button(&t("Only join servers"), &["pill"]);
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 12);
    bx.set_halign(gtk::Align::Center);
    bx.append(&create);
    bx.append(&join);
    welcome.set_child(Some(&bx));
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(&welcome));
    nav.add(&adw::NavigationPage::new(&tv, "CS:GO Legacy Server"));

    join.connect_clicked(glib::clone!(#[weak] root, #[weak] tabs, move |_| {
        tabs.set_visible_child_name("join");
        root.set_visible_child_name("main");
    }));

    // 2. folder + dependencies + download
    let pp = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Download the server"));
    g.set_description(Some(&t("About 33 GB. You can close the app: the download continues and resumes if cut.")));
    let dir = a.cfg.borrow().server.install_dir.clone();
    let folder = adw::ActionRow::builder().title(t("Server folder")).subtitle(format!("{} · {} {}", dir.display(), t("Free space"), free_text(&dir))).build();
    let change = gtk::Button::with_label(&t("Change"));
    change.set_valign(gtk::Align::Center);
    folder.add_suffix(&change);
    let missing = install::missing_deps();
    let deps = adw::ActionRow::builder()
        .title(t("Dependencies"))
        .subtitle(if missing.is_empty() { t("installed") } else { format!("{}: {}\n{}", t("missing"), missing.join(", "), install::deps_command()) })
        .subtitle_selectable(true)
        .build();
    g.add(&folder);
    g.add(&deps);
    let progress = gtk::ProgressBar::builder().show_text(true).margin_top(18).build();
    progress.set_visible(false);
    let download = button(&t("Download"), &["suggested-action", "pill"]);
    download.set_halign(gtk::Align::Center);
    download.set_margin_top(18);
    download.set_sensitive(missing.is_empty());
    let g2 = adw::PreferencesGroup::new();
    g2.add(&download);
    g2.add(&progress);
    pp.add(&g);
    pp.add(&g2);
    let tv2 = adw::ToolbarView::new();
    tv2.add_top_bar(&adw::HeaderBar::new());
    tv2.set_content(Some(&pp));
    let install_page = adw::NavigationPage::new(&tv2, &t("Create a server"));

    create.connect_clicked(glib::clone!(#[weak] nav, #[strong] install_page, move |_| nav.push(&install_page)));

    change.connect_clicked(glib::clone!(#[strong] a, #[weak] folder, move |_| {
        let fd = gtk::FileDialog::builder().title(t("Server folder")).modal(true).build();
        fd.select_folder(Some(&a.win), None::<&gio::Cancellable>, glib::clone!(#[strong] a, #[weak] folder, move |r| {
            let Ok(f) = r else { return };
            let Some(p) = f.path() else { return };
            // a folder with srcds_run is used as is; otherwise it installs into <folder>/server
            let p = if p.join("srcds_run").is_file() || p.ends_with("server") { p } else { p.join("server") };
            let mut cfg = a.cfg.borrow().clone();
            cfg.server.install_dir = p.clone();
            match cfg.save() {
                Ok(()) => {
                    *a.cfg.borrow_mut() = cfg;
                    folder.set_subtitle(&format!("{} · {} {}", p.display(), t("Free space"), free_text(&p)));
                }
                Err(e) => a.error(&e),
            }
        }));
    }));

    let watch = glib::clone!(#[strong] a, #[weak] progress, #[weak] download, #[weak] root, move || {
        progress.set_visible(true);
        download.set_sensitive(false);
        glib::timeout_add_local(std::time::Duration::from_millis(700), glib::clone!(#[strong] a, #[weak] progress, #[weak] download, #[weak] root, #[upgrade_or] glib::ControlFlow::Break, move || {
            let txt = std::fs::read_to_string(server::install_progress_file()).unwrap_or_default();
            let (f, msg) = txt.split_once('|').unwrap_or(("", &txt));
            match f {
                "done" => {
                    a.toast(&t("Installed"));
                    root.set_visible_child_name("main");
                    a.refresh();
                    return glib::ControlFlow::Break;
                }
                "error" => {
                    a.error(&anyhow::anyhow!("{}: {}", t("Installation failed"), t_msg(msg)));
                    download.set_sensitive(true);
                    return glib::ControlFlow::Break;
                }
                _ => {}
            }
            match f.parse::<f64>() {
                Ok(x) => progress.set_fraction(x),
                Err(_) => progress.pulse(),
            }
            let label = if msg.is_empty() { t("Installing…") } else { t_msg(msg) };
            progress.set_text(Some(&label));
            if !server::running(server::INSTALL_UNIT) && !Server::new(&a.cfg.borrow()).is_installed() && !txt.is_empty() && f != "done" {
                // the process died without writing the end
                download.set_sensitive(true);
            }
            glib::ControlFlow::Continue
        }));
    });
    let w2 = watch.clone();
    download.connect_clicked(glib::clone!(#[strong] a, move |_| match server::spawn_install(false) {
        Ok(()) => w2(),
        Err(e) => a.error(&e),
    }));
    // install already running (app reopened)
    if server::running(server::INSTALL_UNIT) {
        nav.push(&install_page);
        watch();
    }
    nav.upcast()
}
