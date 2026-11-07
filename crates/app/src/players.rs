//! Players tab: connected (kick, ban, skins), saved skins and bans.
use crate::i18n::t;
use crate::{bg, btn, model, row, ActionRow, AppRc, KeyValue};
use csls_core::server::Server;
use csls_core::skins;
use csls_core::{agent, players};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn rcon(a: &AppRc, cmd: String) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    bg(
        move || Server::new(&cfg).rcon()?.exec(&cmd),
        move |r: anyhow::Result<String>| {
            match r {
                Ok(o) if !o.trim().is_empty() => a.toast(o.trim()),
                Ok(_) => {}
                Err(e) => a.error(&e),
            }
            a.refresh();
        },
    );
}

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue { key: key.into(), value: value.into(), warn: false }
}

fn skins_dialog(a: &AppRc, title: &str, steamid64: String) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    let title = format!("{} · {title}", t("Skins"));
    bg(
        move || -> anyhow::Result<Vec<skins::Entry>> {
            let csgo = Server::new(&cfg).csgo_dir();
            let cat = skins::Catalog::load(&csgo.join("addons/sourcemod/configs/skins.cfg"))?;
            skins::player_skins(&skins::db_path(&csgo), &cat, &steamid64)
        },
        move |r| {
            let list = match r {
                Ok(v) if v.is_empty() => vec![kv(&t("Nothing saved"), "")],
                Ok(v) => v.iter().map(|e| kv(&t(&e.weapon), &e.value)).collect(),
                Err(e) => vec![kv(&t("Error"), &format!("{e:#}"))],
            };
            let s = a.s();
            s.set_skins_title(title.into());
            s.set_skins_list(model(list));
            s.set_skins_open(true);
        },
    );
}

fn reload_offline(a: &AppRc) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    bg(
        move || {
            let csgo = Server::new(&cfg).csgo_dir();
            (skins::players(&skins::db_path(&csgo)).unwrap_or_default(), players::read_bans(&csgo))
        },
        move |(ids, banned)| {
            let saved: Vec<ActionRow> = ids.iter().map(|id| row(id, id, "", vec![btn("skins", &t("Skins"))])).collect();
            let bans: Vec<ActionRow> = banned.iter().map(|b| row(b, b, "", vec![btn("unban", &t("Unban"))])).collect();
            a.s().set_pl_saved(model(saved));
            a.s().set_pl_bans(model(bans));
        },
    );
}

const KINDS: [&str; 4] = ["warning", "kick", "ban", "note"];

fn history_rows(list: &[agent::Player], query: &str) -> Vec<ActionRow> {
    let q = query.to_lowercase();
    list.iter()
        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q) || p.steamid.to_lowercase().contains(&q))
        .map(|p| {
            let sub = format!(
                "{} · {} {} · {} · {}: {}",
                p.steamid,
                p.connections,
                t("connections"),
                crate::duration(p.seconds),
                t("last seen"),
                crate::when(p.last_seen)
            );
            row(&p.steamid, &p.name, &sub, vec![btn("sanction", &t("Sanction"))])
        })
        .collect()
}

#[derive(Default)]
struct History {
    players: Vec<agent::Player>,
    sanctions: Vec<agent::Sanction>,
}

fn reload_history(a: &AppRc, h: &Rc<RefCell<History>>) {
    let (a, h) = (a.clone(), h.clone());
    bg(
        || agent::players().and_then(|p| Ok((p, agent::sanctions()?))),
        move |r| {
            let s = a.s();
            match r {
                Ok((players, sanctions)) => {
                    s.set_pl_note("".into());
                    s.set_pl_history(model(history_rows(&players, &s.get_pl_search())));
                    let rows = sanctions.iter().map(|x| {
                        let sub = format!("{} · {} · {} · {} min · {}", x.steamid, t(&x.kind), x.reason, x.minutes, crate::when(x.time));
                        row("", &x.name, &sub, vec![])
                    });
                    s.set_pl_sanctions(model(rows.collect()));
                    *h.borrow_mut() = History { players, sanctions };
                }
                Err(_) => s.set_pl_note(t("No history: the agent is not running, or statistics are off").into()),
            }
        },
    );
}

pub fn init(a: &AppRc) {
    let s = a.s();
    reload_offline(a);
    let history: Rc<RefCell<History>> = Rc::default();
    reload_history(a, &history);
    s.set_pl_note(t("No history: the agent is not running, or statistics are off").into());
    let (app, h) = (a.clone(), history.clone());
    a.on_agent_changed(move || reload_history(&app, &h));
    let (app, h) = (a.clone(), history.clone());
    s.on_pl_search_changed(move || {
        let rows = history_rows(&h.borrow().players, &app.s().get_pl_search());
        app.s().set_pl_history(model(rows));
    });
    let (app, h) = (a.clone(), history.clone());
    s.on_pl_export(move |format| {
        let h = h.borrow();
        let (players, sanctions) = (&h.players, &h.sanctions);
        let (name, text) = if format == "csv" {
            ("players.csv", format!("{}\n{}", agent::players_csv(players), agent::sanctions_csv(sanctions)))
        } else {
            ("players.json", serde_json::json!({ "players": players, "sanctions": sanctions }).to_string())
        };
        crate::save_as(&app, name.to_string(), text);
    });
    let target: Rc<RefCell<(String, String)>> = Rc::default();
    let (app, h, tg) = (a.clone(), history.clone(), target.clone());
    s.on_pl_history_action(move |id, _| {
        let name = h.borrow().players.iter().find(|p| p.steamid == id.as_str()).map(|p| p.name.clone()).unwrap_or_default();
        let s = app.s();
        s.set_san_title(format!("{} · {name}", t("Sanction")).into());
        s.set_san_kind(0);
        s.set_san_minutes(0);
        s.set_san_reason("".into());
        s.set_san_notes("".into());
        *tg.borrow_mut() = (id.to_string(), name);
        s.set_san_open(true);
    });
    let (app, h) = (a.clone(), history.clone());
    s.on_san_confirm(move || {
        let s = app.s();
        let (steamid, name) = target.borrow().clone();
        let x = agent::Sanction {
            steamid,
            name,
            kind: KINDS[s.get_san_kind().clamp(0, 3) as usize].into(),
            reason: s.get_san_reason().to_string(),
            notes: s.get_san_notes().to_string(),
            minutes: s.get_san_minutes().max(0) as u32,
            time: 0,
        };
        s.set_san_open(false);
        let (a2, h2) = (app.clone(), h.clone());
        bg(
            move || agent::add_sanction(&x),
            move |r| match r {
                Ok(()) => reload_history(&a2, &h2),
                Err(e) => a2.error(&e),
            },
        );
    });
    let app = a.clone();
    s.on_pl_refresh(move || reload_offline(&app));
    let app = a.clone();
    s.on_pl_saved_action(move |id, _| skins_dialog(&app, &id, id.to_string()));
    let app = a.clone();
    s.on_pl_bans_action(move |id, _| match players::unban_cmd(&id) {
        Ok(c) => rcon(&app, c),
        Err(e) => app.error(&e),
    });
    let ban_target = Rc::new(Cell::new(0u32));
    let app = a.clone();
    let target = ban_target.clone();
    s.on_pl_now_action(move |id, action| {
        let Ok(uid) = id.parse::<u32>() else { return };
        let live = app.live.borrow();
        let Some(p) = live.status.players.iter().find(|p| p.userid == uid).cloned() else { return };
        drop(live);
        match action.as_str() {
            "kick" => rcon(&app, players::kick_cmd(uid, "")),
            "ban" => {
                target.set(uid);
                let s = app.s();
                s.set_ban_title(format!("{} {}", t("Ban"), p.name).into());
                s.set_ban_minutes(60);
                s.set_ban_reason("".into());
                s.set_ban_open(true);
            }
            "skins" => {
                if let Some(id64) = p.steamid64() {
                    skins_dialog(&app, &p.name, id64.to_string());
                }
            }
            _ => {}
        }
    });
    let app = a.clone();
    s.on_ban_confirm(move || {
        let s = app.s();
        s.set_ban_open(false);
        rcon(&app, players::ban_cmd(ban_target.get(), s.get_ban_minutes().max(0) as u32, &s.get_ban_reason()));
    });

    // only rebuilt when something changes (otherwise the buttons flicker)
    type Shown = Vec<(u32, String, String, i64, bool)>;
    let keys: RefCell<Option<Shown>> = RefCell::default();
    let app = a.clone();
    a.on_live(move |l| {
        let ps = &l.status.players;
        let key: Vec<_> = ps.iter().map(|p| (p.userid, p.name.clone(), p.steamid.clone(), i64::from(p.ping / 20), p.bot)).collect();
        if keys.borrow().as_ref() == Some(&key) {
            return;
        }
        *keys.borrow_mut() = Some(key);
        let rows: Vec<ActionRow> = ps
            .iter()
            .map(|p| {
                let sub = if p.bot { "BOT".to_string() } else { format!("{} · {} ms · {}", p.steamid, p.ping, p.connected) };
                let mut b = Vec::new();
                if p.steamid64().is_some() {
                    b.push(btn("skins", &t("Skins")));
                }
                b.push(btn("kick", &t("Kick")));
                if !p.bot {
                    b.push(btn("ban", &t("Ban")));
                }
                row(&p.userid.to_string(), &p.name, &sub, b)
            })
            .collect();
        app.s().set_pl_now(model(rows));
    });
}
