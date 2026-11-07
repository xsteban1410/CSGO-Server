//! Updates tab: the app (asks GitHub only on request or with the option on) and the pinned SourceMod and Metamod.
use crate::i18n::t;
use crate::{bg, AppRc};
use csls_core::updates::{self, Channel, Release};
use csls_core::{backup, install, server};
use std::cell::RefCell;
use std::rc::Rc;

const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn versions(a: &AppRc) {
    let m = install::read_manifest();
    let (sm, mm) = install::pinned().map(|p| (p.sourcemod.version, p.metamod.version)).unwrap_or_default();
    let line = |have: &str, want: &str| {
        if have.is_empty() {
            format!("{} {want}", t("Tested with"))
        } else if have == want {
            have.to_string()
        } else {
            format!("{have} → {want}")
        }
    };
    let s = a.s();
    s.set_up_sourcemod(line(&m.sourcemod, &sm).into());
    s.set_up_metamod(line(&m.metamod, &mm).into());
}

fn check(a: &AppRc, latest: &Rc<RefCell<Option<Release>>>) {
    let (a, latest) = (a.clone(), latest.clone());
    a.s().set_up_latest(t("Checking…").into());
    bg(updates::latest, move |r| {
        let s = a.s();
        match r {
            Ok(rel) => {
                let newer = updates::is_newer(&rel.version, CURRENT);
                s.set_up_latest(
                    if newer { format!("{} · {}", rel.version, t("New")) } else { format!("{} · {}", rel.version, t("Up to date")) }.into(),
                );
                let channel = updates::channel();
                s.set_up_command(if newer { updates::command_for(&channel).unwrap_or("") } else { "" }.into());
                s.set_up_can_apply(newer && matches!(channel, Channel::AppImage(_)));
                *latest.borrow_mut() = Some(rel);
            }
            Err(e) => s.set_up_latest(format!("{e:#}").into()),
        }
    });
}

pub fn init(a: &AppRc) {
    let s = a.s();
    s.set_up_current(CURRENT.into());
    s.set_up_auto(a.cfg.borrow().server.check_updates);
    versions(a);
    let latest: Rc<RefCell<Option<Release>>> = Rc::default();
    // only with the option on: this is a request to a third party
    if a.cfg.borrow().server.check_updates && std::env::var_os("CSLS_SCREENSHOT").is_none() {
        check(a, &latest);
    }
    let (app, l) = (a.clone(), latest.clone());
    s.on_up_check(move || check(&app, &l));
    let (app, l) = (a.clone(), latest);
    s.on_up_apply(move || {
        let Channel::AppImage(target) = updates::channel() else { return };
        let Some(rel) = l.borrow().clone() else { return };
        let a2 = app.clone();
        app.ask(
            &t("Update"),
            &format!("{}: {}", t("The app will download and check version"), rel.version),
            &t("Update"),
            &t("Cancel"),
            false,
            move |yes| {
                if !yes {
                    return;
                }
                a2.toast(&t("Installing…"));
                let a3 = a2.clone();
                bg(
                    move || updates::update_appimage(&rel, &target),
                    move |r| match r {
                        Ok(()) => a3.toast(&t("Updated: restart the app")),
                        Err(e) => a3.error(&e),
                    },
                );
            },
        );
    });
    let app = a.clone();
    s.on_up_auto_changed(move || {
        let mut cfg = app.cfg.borrow().clone();
        cfg.server.check_updates = app.s().get_up_auto();
        match cfg.save() {
            Ok(()) => *app.cfg.borrow_mut() = cfg,
            Err(e) => app.error(&e),
        }
    });
    let app = a.clone();
    s.on_up_game(move || {
        if app.live.borrow().running {
            app.warn(&t("Stop the server before updating it"));
            return;
        }
        let a2 = app.clone();
        app.ask(
            &t("Update server and SourceMod"),
            &t("A backup is made first. The download can take a while."),
            &t("Update"),
            &t("Cancel"),
            false,
            move |yes| {
                if !yes {
                    return;
                }
                let cfg = a2.cfg.borrow().clone();
                if let Err(e) = backup::create_auto(&cfg).and_then(|_| server::spawn_install(true)) {
                    a2.error(&e);
                    return;
                }
                a2.toast(&t("Installing…"));
            },
        );
    });
}
