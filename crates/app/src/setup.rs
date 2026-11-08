//! First run: create a server (choose folder, dependencies, background download) or just join.
use crate::i18n::{t, t_msg};
use crate::AppRc;
use csls_core::install;
use csls_core::server::{self, Server};
use slint::{Timer, TimerMode};
use std::rc::Rc;
use std::time::Duration;

fn free_text(dir: &std::path::Path) -> String {
    install::free_bytes(dir).map(|b| format!("{} GB", b >> 30)).unwrap_or_else(|| "?".into())
}

fn show_folder(a: &AppRc) {
    let dir = a.cfg.borrow().server.install_dir.clone();
    a.s().set_setup_folder(format!("{} · {} {}", dir.display(), t("Free space"), free_text(&dir)).into());
}

fn watch(a: &AppRc) {
    let s = a.s();
    s.set_setup_busy(true);
    s.set_setup_progress(-1.0);
    let timer = Rc::new(Timer::default());
    let app = a.clone();
    let keep = timer.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(700), move || {
        let s = app.s();
        let txt = std::fs::read_to_string(server::install_progress_file()).unwrap_or_default();
        let (f, msg) = txt.split_once('|').unwrap_or(("", &txt));
        match f {
            "done" => {
                app.toast(&t("Installed"));
                s.set_setup("".into());
                s.set_setup_busy(false);
                app.refresh();
                keep.stop();
                return;
            }
            "error" => {
                app.error(&anyhow::anyhow!("{}: {}", t("Installation failed"), t_msg(msg)));
                s.set_setup_busy(false);
                keep.stop();
                return;
            }
            _ => {}
        }
        s.set_setup_progress(f.parse::<f32>().unwrap_or(-1.0));
        s.set_setup_progress_text(if msg.is_empty() { t("Installing…") } else { t_msg(msg) }.into());
        if !server::running(server::INSTALL_UNIT) && !Server::new(&app.cfg.borrow()).is_installed() && !txt.is_empty() && f != "done" {
            // the process died without writing the end
            s.set_setup_busy(false);
            keep.stop();
        }
    });
}

pub fn init(a: &AppRc) {
    let s = a.s();
    let need = {
        let cfg = a.cfg.borrow();
        !Server::new(&cfg).is_installed() || std::env::var_os("CSLS_FORCE_SETUP").is_some()
    };
    if need {
        s.set_setup("welcome".into());
    }
    show_folder(a);
    let missing = install::missing_deps();
    s.set_setup_deps_ok(missing.is_empty());
    s.set_setup_deps(
        if missing.is_empty() { t("installed") } else { format!("{}: {}\n{}", t("missing"), missing.join(", "), install::deps_command()) }
            .into(),
    );
    let app = a.clone();
    s.on_setup_choose_folder(move || {
        let a = app.clone();
        crate::bg(
            || rfd::FileDialog::new().set_title(t("Server folder")).pick_folder(),
            move |p| {
                let Some(p) = p else { return };
                // a folder with srcds_run is used as is; otherwise it installs into <folder>/server
                let p = if p.join("srcds_run").is_file() || p.ends_with("server") { p } else { p.join("server") };
                let mut cfg = a.cfg.borrow().clone();
                cfg.server.install_dir = p;
                match cfg.save() {
                    Ok(()) => {
                        *a.cfg.borrow_mut() = cfg;
                        show_folder(&a);
                    }
                    Err(e) => a.error(&e),
                }
            },
        );
    });
    let app = a.clone();
    s.on_setup_download(move || match server::spawn_install(false) {
        Ok(()) => watch(&app),
        Err(e) => app.error(&e),
    });
    // install already running (app reopened)
    if server::running(server::INSTALL_UNIT) {
        s.set_setup("install".into());
        watch(a);
    }
}
