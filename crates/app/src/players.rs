//! Players tab: connected (kick, ban, skins), saved skins and bans.
use crate::i18n::t;
use crate::{bg, btn, model, row, ActionRow, AppRc, KeyValue};
use csls_core::players;
use csls_core::server::Server;
use csls_core::skins;
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

pub fn init(a: &AppRc) {
    let s = a.s();
    reload_offline(a);
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
