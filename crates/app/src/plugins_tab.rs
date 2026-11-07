//! Plugins tab: what is on the server, on and off, install, reload and go back to an earlier version.
use crate::i18n::t;
use crate::{bg, btn, model, row, ActionRow, AppRc, Btn};
use csls_core::deploy;
use csls_core::plugins::{self, Info};
use csls_core::server::Server;

fn subtitle(p: &Info) -> String {
    let mut parts = vec![
        if p.enabled { t("On") } else { t("Off") },
        t(match p.origin.as_str() {
            "builtin" => "Built-in",
            "user" => "Yours",
            _ => "On the server",
        }),
    ];
    if !p.version.is_empty() {
        parts.push(p.version.clone());
    }
    if !p.author.is_empty() {
        parts.push(p.author.clone());
    }
    if !p.missing.is_empty() {
        parts.push(format!("{}: {}", t("Missing"), p.missing.join(", ")));
    }
    if let Some(c) = &p.conflict {
        parts.push(format!("{}: {c}", t("Same name as")));
    }
    parts.join(" · ")
}

fn rows(list: &[Info], live: bool) -> Vec<ActionRow> {
    list.iter()
        .map(|p| {
            let mut b: Vec<Btn> = Vec::new();
            b.push(if p.enabled { btn("off", &t("Turn off")) } else { btn("on", &t("Turn on")) });
            if live && p.enabled {
                b.push(btn("reload", &t("Reload")));
            }
            if p.origin == "user" && plugins::history(&p.name).is_ok_and(|h| h.len() > 1) {
                b.push(btn("back", &t("Previous version")));
            }
            let title = if p.title.is_empty() { p.name.clone() } else { format!("{} ({})", p.title, p.name) };
            row(&p.name, &title, &subtitle(p), b)
        })
        .collect()
}

fn fill(a: &AppRc) {
    let cfg = a.cfg.borrow().clone();
    let live = a.live.borrow().rcon_ok;
    let a = a.clone();
    bg(
        move || plugins::list(&cfg),
        move |r| match r {
            Ok(list) => a.s().set_pg_rows(model(rows(&list, live))),
            Err(e) => a.error(&e),
        },
    );
}

/// Compiles what changed, and shows the compiler's errors.
fn deploy_now(a: &AppRc) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    bg(
        move || deploy::deploy(&cfg, false, true),
        move |r: anyhow::Result<deploy::Report>| {
            match r {
                Ok(r) => match r.error_text() {
                    Some(e) => {
                        a.error(&anyhow::anyhow!("{}\n{e}", t("Plugins that do not compile (the server keeps the previous version):")))
                    }
                    None if r.compiled.is_empty() => a.toast(&t("Saved")),
                    None => a.toast(&format!("{}: {}", t("Compiled"), r.compiled.join(", "))),
                },
                Err(e) => a.error(&e),
            }
            fill(&a);
        },
    );
}

fn rcon(a: &AppRc, cmd: String) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    bg(
        move || Server::new(&cfg).rcon()?.exec(&cmd),
        move |r: anyhow::Result<String>| match r {
            Ok(o) if !o.trim().is_empty() => a.toast(o.trim()),
            Ok(_) => {}
            Err(e) => a.error(&e),
        },
    );
}

pub fn init(a: &AppRc) {
    fill(a);
    let s = a.s();
    let app = a.clone();
    s.on_pg_refresh(move || fill(&app));
    let app = a.clone();
    s.on_pg_install(move || {
        let a = app.clone();
        bg(
            || rfd::FileDialog::new().set_title(t("Install")).add_filter("SourceMod", &["sp", "smx"]).pick_file(),
            move |p| {
                let Some(p) = p else { return };
                let cfg = a.cfg.borrow().clone();
                match plugins::install(&cfg, &p) {
                    Ok(_) => deploy_now(&a),
                    Err(e) => a.error(&e),
                }
            },
        );
    });
    let app = a.clone();
    s.on_pg_action(move |name, action| {
        let cfg = app.cfg.borrow().clone();
        let result = match action.as_str() {
            "on" => plugins::set_enabled(&cfg, &name, true).map(|()| plugins::reload_command(&name).ok()),
            "off" => plugins::set_enabled(&cfg, &name, false).map(|()| Some(format!("sm plugins unload {name}"))),
            "reload" => plugins::reload_command(&name).map(Some),
            "back" => {
                // the newest is what is there now
                let r = plugins::history(&name).and_then(|h| match h.get(1) {
                    Some(v) => plugins::rollback(&name, v),
                    None => Ok(()),
                });
                if r.is_ok() {
                    deploy_now(&app);
                }
                r.map(|()| None)
            }
            _ => Ok(None),
        };
        match result {
            Ok(cmd) => {
                if let Some(cmd) = cmd.filter(|_| app.live.borrow().rcon_ok) {
                    rcon(&app, cmd);
                }
                fill(&app);
            }
            Err(e) => app.error(&e),
        }
    });
    // Reload depends on the server answering
    let was = std::cell::Cell::new(a.live.borrow().rcon_ok);
    let app = a.clone();
    a.on_live(move |l| {
        if was.replace(l.rcon_ok) != l.rcon_ok {
            fill(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_offer_what_applies() {
        let p = Info { name: "x".into(), enabled: true, origin: "server".into(), missing: vec!["dep".into()], ..Default::default() };
        let r = rows(std::slice::from_ref(&p), true);
        assert_eq!(r[0].title, "x");
        assert!(r[0].subtitle.contains("dep"));
        use slint::Model;
        let ids: Vec<String> = r[0].buttons.iter().map(|b| b.id.to_string()).collect();
        assert_eq!(ids, ["off", "reload"]);
        let off = Info { enabled: false, ..p };
        let ids: Vec<String> = rows(&[off], true)[0].buttons.iter().map(|b| b.id.to_string()).collect();
        assert_eq!(ids, ["on"]);
    }
}
