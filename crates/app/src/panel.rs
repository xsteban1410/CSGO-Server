//! Server tab: status, connect address and Start / Change / Stop / Play.
use crate::i18n::t;
use crate::{bg, btn, model, row, strings, wizard, AppRc, ConsoleLine};
use csls_core::config::Network;
use csls_core::server::Server;
use csls_core::steam_client;
use std::cell::Cell;

fn uptime_text(secs: u64) -> String {
    if secs >= 3600 {
        format!("{} h {} min", secs / 3600, secs / 60 % 60)
    } else {
        format!("{} min", secs / 60)
    }
}

/// The address to give, without the LAN note.
fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

fn fill_presets(a: &AppRc) {
    let on = a.live.borrow().rcon_ok;
    let list = csls_core::presets::Presets::load().unwrap_or_default();
    let rows = list.0.iter().map(|(name, o)| {
        let sub = format!("{}{}", o.mode, o.map.as_deref().map(|m| format!(" · {m}")).unwrap_or_default());
        row(name, name, &sub, vec![btn("go", &if on { t("Apply") } else { t("Start") }), btn("del", &t("Delete"))])
    });
    a.s().set_presets(model(rows.collect()));
}

fn set_addr(a: &AppRc) {
    let cfg = a.cfg.borrow().clone();
    let srv = Server::new(&cfg);
    let local = srv.connect_address();
    let s = a.s();
    s.set_st_addr(local.clone().into());
    let is_vpn = cfg.server.network == Network::Vpn;
    s.set_st_is_vpn(is_vpn);
    if is_vpn {
        s.set_st_vpn_addr(srv.vpn_address().unwrap_or_else(|| t("No Tailscale address found: use this PC's VPN IP")).into());
    }
    if srv.public() {
        let a = a.clone();
        bg(
            move || Server::new(&cfg).public_address(),
            move |p: Option<String>| {
                if let Some(p) = p {
                    a.s().set_st_addr(format!("{p}  ·  LAN {local}").into());
                }
            },
        );
    }
}

/// The lines that say something happened, from the agent's log.
fn fill_events(a: &AppRc) {
    let app = a.clone();
    bg(
        || csls_core::agent::call("lines", serde_json::json!({ "since": 0, "limit": 300 })),
        move |r: anyhow::Result<serde_json::Value>| {
            let list = r.unwrap_or_default();
            let events: Vec<ConsoleLine> = list
                .as_array()
                .into_iter()
                .flatten()
                .filter(|l| matches!(l["category"].as_str(), Some("player" | "error")))
                .map(|l| ConsoleLine {
                    text: l["text"].as_str().unwrap_or("").into(),
                    category: l["category"].as_str().unwrap_or("").into(),
                })
                .collect();
            let skip = events.len().saturating_sub(6);
            app.s().set_st_events(model(events.into_iter().skip(skip).collect()));
            app.s().set_st_agent(
                if csls_core::server::agent_running() { t("Running") } else { t("Not running: no history or scheduled tasks") }.into(),
            );
        },
    );
}

fn fill_warnings(a: &AppRc) {
    let cfg = a.cfg.borrow().clone();
    let app = a.clone();
    bg(
        move || csls_core::security::preflight(&cfg),
        move |list| {
            let v: Vec<String> = list.iter().map(|f| f.text.clone()).collect();
            app.s().set_st_warnings(strings(&v));
        },
    );
}

pub fn ask_stop(a: &AppRc) {
    let app = a.clone();
    a.ask(&t("Stop the server?"), &t("Players will be disconnected."), &t("Stop"), &t("Cancel"), true, move |yes| {
        if !yes {
            return;
        }
        app.stop_requested.set(true);
        let cfg = app.cfg.borrow().clone();
        let a = app.clone();
        bg(
            move || Server::new(&cfg).stop(),
            move |r| {
                match r {
                    Ok(()) => a.toast(&t("Server stopped")),
                    Err(e) => a.error(&e),
                }
                a.refresh();
            },
        );
    });
}

pub fn init(a: &AppRc) {
    let s = a.s();
    let app = a.clone();
    s.on_copy_addr(move || {
        let addr = app.s().get_st_addr();
        app.copy(format!("connect {}", first_word(&addr)));
    });
    let app = a.clone();
    s.on_copy_vpn(move || {
        let addr = app.s().get_st_vpn_addr().to_string();
        app.copy(addr);
    });
    let app = a.clone();
    s.on_bridge_start(move || {
        let cfg = app.cfg.borrow().clone();
        let a = app.clone();
        bg(
            move || Server::new(&cfg).start_bridge(),
            move |r: anyhow::Result<()>| {
                if let Err(e) = r {
                    a.error(&e);
                }
                a.refresh();
            },
        );
    });
    let app = a.clone();
    s.on_open_wizard(move |live| wizard::start_or_install(&app, live));
    let app = a.clone();
    s.on_stop_server(move || ask_stop(&app));
    let app = a.clone();
    s.on_play(move || {
        let c = Server::new(&app.cfg.borrow()).connect_address();
        match steam_client::launch_csgo(Some(&c)) {
            Ok(_) => {
                app.copy(format!("connect {c}"));
                app.toast(&t("Opening. If it does not join by itself, paste the copied command in the console"));
            }
            Err(e) => app.error(&e),
        }
    });
    let app = a.clone();
    s.on_quick(move |what| {
        let cfg = app.cfg.borrow().clone();
        let a = app.clone();
        let what = what.to_string();
        bg(
            move || -> anyhow::Result<String> {
                let srv = Server::new(&cfg);
                match what.as_str() {
                    "restart" => srv.restart_round().map(|_| t("Round restarted")),
                    "warmup" => srv.end_warmup().map(|_| t("Warmup ended")),
                    "bot+" => srv.add_bots(1).map(|q| format!("bot_quota {q}")),
                    _ => srv.add_bots(-1).map(|q| format!("bot_quota {q}")),
                }
            },
            move |r| match r {
                Ok(m) => {
                    a.toast(&m);
                    a.refresh();
                }
                Err(e) => a.error(&e),
            },
        );
    });
    let app = a.clone();
    s.on_preset_action(move |name, action| {
        if action == "del" {
            match csls_core::presets::Presets::remove(&name) {
                Ok(()) => app.modes_changed(),
                Err(e) => app.error(&e),
            }
            return;
        }
        let list = csls_core::presets::Presets::load().unwrap_or_default();
        if let Some(o) = list.0.get(name.as_str()) {
            let live = app.live.borrow().rcon_ok;
            wizard::run_preset(&app, o.clone(), live);
        }
    });

    fill_presets(a);
    let app = a.clone();
    a.on_modes_changed(move || fill_presets(&app));
    set_addr(a);
    fill_warnings(a);
    fill_events(a);
    let app = a.clone();
    a.on_agent_changed(move || fill_events(&app));
    let app = a.clone();
    a.every(std::time::Duration::from_secs(10), move || fill_events(&app));
    let was_on = Cell::new(None);
    let app = a.clone();
    a.on_live(move |l| {
        let s = app.s();
        s.set_running(l.running);
        s.set_rcon_ok(l.rcon_ok);
        s.set_st_bridge(l.bridge);
        if was_on.replace(Some(l.rcon_ok)) != Some(l.rcon_ok) {
            fill_presets(&app);
            set_addr(&app);
        }
        s.set_st_state(
            if !l.running {
                t("Stopped")
            } else if l.rcon_ok {
                t("Running")
            } else {
                t("Starting…")
            }
            .into(),
        );
        s.set_st_rcon(
            if l.rcon_ok {
                t("Answering")
            } else if l.running {
                t("Not answering yet")
            } else {
                String::new()
            }
            .into(),
        );
        s.set_st_mode(l.mode.as_str().into());
        s.set_st_resources(
            l.stats
                .map_or(String::new(), |x| format!("CPU {}% · RAM {} MB · {}", x.cpu_percent, x.rss_mb, uptime_text(x.uptime_secs)))
                .into(),
        );
        s.set_st_error(l.last_error.as_deref().unwrap_or("").into());
        s.set_st_map(l.status.map.as_str().into());
        s.set_st_players(
            if l.running { format!("{} + {} bots / {}", l.status.humans, l.status.bots, l.status.max) } else { String::new() }.into(),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_and_address() {
        assert_eq!(uptime_text(59), "0 min");
        assert_eq!(uptime_text(3720), "1 h 2 min");
        assert_eq!(first_word("1.2.3.4:27030  ·  LAN 192.168.1.2:27030"), "1.2.3.4:27030");
    }
}
