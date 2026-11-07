//! System tray: open, status, start and stop, console, and quit asking about the server.
use crate::i18n::t;
use crate::{bg, AppRc, Tray};
use csls_core::server::Server;
use slint::ComponentHandle;

/// Is there a tray to go to? Without one, closing the window has to close the app, or nobody could open it again.
/// shortcut: asks busctl, a tool every systemd desktop has; a D-Bus crate is a lot for one question.
pub fn available() -> bool {
    std::process::Command::new("busctl")
        .args(["--user", "--timeout=1", "status", "org.kde.StatusNotifierWatcher"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn show(a: &AppRc, tab: Option<i32>) {
    if let Some(tab) = tab {
        a.s().set_tab(tab);
    }
    if let Err(e) = a.ui.show() {
        a.error(&e.into());
    }
}

pub fn init(a: &AppRc) -> anyhow::Result<Tray> {
    let tray = Tray::new()?;
    let weak = std::rc::Rc::downgrade(a);
    let with = move |f: fn(&AppRc)| {
        let weak = weak.clone();
        move || {
            if let Some(a) = weak.upgrade() {
                f(&a);
            }
        }
    };
    tray.on_open(with(|a| show(a, None)));
    tray.on_console(with(|a| show(a, Some(4))));
    tray.on_change(with(|a| {
        show(a, Some(0));
        crate::wizard::open(a, true);
    }));
    tray.on_start(with(|a| {
        show(a, Some(0));
        crate::wizard::open(a, false);
    }));
    tray.on_stop(with(|a| {
        show(a, Some(0));
        crate::panel::ask_stop(a);
    }));
    tray.on_quit(with(quit));
    let t2 = tray.as_weak();
    a.on_live(move |l| {
        if let Some(tr) = t2.upgrade() {
            tr.set_running(l.running);
            tr.set_live(l.rcon_ok);
            tr.set_status(if l.running {
                format!("CS:GO Legacy Server · {} · {}", l.mode, l.status.map).into()
            } else {
                format!("CS:GO Legacy Server · {}", t("Stopped")).into()
            });
        }
    });
    tray.show()?;
    Ok(tray)
}

/// Leaves for good. The server outlives the app, so if it is running the choice is the user's.
fn quit(a: &AppRc) {
    if !a.live.borrow().running {
        let _ = slint::quit_event_loop();
        return;
    }
    show(a, None);
    let app = a.clone();
    a.ask(&t("Quit"), &t("The server is running. Stop it too?"), &t("Stop and quit"), &t("Leave it running"), true, move |stop| {
        if !stop {
            let _ = slint::quit_event_loop();
            return;
        }
        app.stop_requested.set(true);
        let cfg = app.cfg.borrow().clone();
        let a2 = app.clone();
        bg(
            move || Server::new(&cfg).stop(),
            move |r| match r {
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(e) => a2.error(&e),
            },
        );
    });
}
