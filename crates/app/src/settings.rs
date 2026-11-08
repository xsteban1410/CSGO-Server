//! Settings tab: config.toml, maintenance and diagnostics.
use crate::i18n::{t, t_msg};
use crate::{bg, model, AppRc, KeyValue};
use csls_core::config::{AcAction, Language, Network};
use csls_core::server;
use csls_core::{deploy, doctor};

const ACTIONS: [AcAction; 3] = [AcAction::Log, AcAction::Kick, AcAction::Ban];
const NETS: [Network; 4] = [Network::Lan, Network::Vpn, Network::Internet, Network::Community];

fn fill(a: &AppRc) {
    let cfg = a.cfg.borrow();
    let c = &cfg.server;
    let s = a.s();
    s.set_set_language(i32::from(cfg.language == Language::Es));
    s.set_set_name(c.hostname.as_str().into());
    s.set_set_pass(c.password.as_str().into());
    s.set_set_net(NETS.iter().position(|n| *n == c.network).unwrap_or(0) as i32);
    s.set_set_port(i32::from(c.port));
    s.set_set_max(c.max_players as i32);
    s.set_set_tick(i32::from(c.tickrate >= 128));
    s.set_set_gslt(c.gslt.as_str().into());
    s.set_set_wkey(c.workshop_authkey.as_str().into());
    s.set_set_upnp(c.upnp);
    s.set_set_skins(c.skins);
    s.set_set_ac(cfg.anticheat.enabled);
    s.set_set_ac_action(ACTIONS.iter().position(|x| *x == cfg.anticheat.action).unwrap_or(0) as i32);
    s.set_set_ac_ban(cfg.anticheat.ban_minutes as i32);
    s.set_set_folder(c.install_dir.display().to_string().into());
    let list: Vec<KeyValue> = doctor::run(&cfg)
        .iter()
        .map(|c| KeyValue { key: t(c.name).into(), value: t_msg(&c.detail).into(), warn: c.status == doctor::Status::Problem })
        .collect();
    s.set_set_doctor(model(list));
}

fn save(a: &AppRc) {
    let s = a.s();
    let mut cfg = a.cfg.borrow().clone();
    let old = cfg.server.clone();
    let old_language = cfg.language;
    cfg.language = if s.get_set_language() == 1 { Language::Es } else { Language::En };
    let new_language = cfg.language != old_language;
    let c = &mut cfg.server;
    c.hostname = s.get_set_name().to_string();
    c.password = s.get_set_pass().to_string();
    c.network = NETS[s.get_set_net().clamp(0, 3) as usize];
    c.port = s.get_set_port().clamp(1024, 65000) as u16;
    c.max_players = s.get_set_max().max(2) as u32;
    c.tickrate = if s.get_set_tick() == 1 { 128 } else { 64 };
    c.gslt = s.get_set_gslt().trim().to_string();
    c.workshop_authkey = s.get_set_wkey().trim().to_string();
    c.skins = s.get_set_skins();
    c.upnp = s.get_set_upnp();
    cfg.anticheat.enabled = s.get_set_ac();
    cfg.anticheat.action = ACTIONS[s.get_set_ac_action().clamp(0, 2) as usize];
    cfg.anticheat.ban_minutes = s.get_set_ac_ban().max(0) as u32;
    let c = &cfg.server;
    let restart =
        (c.network, c.port, c.tickrate, c.max_players, &c.gslt) != (old.network, old.port, old.tickrate, old.max_players, &old.gslt);
    match cfg.save() {
        Ok(()) => {
            *a.cfg.borrow_mut() = cfg.clone();
            crate::restart_agent(a);
            // skins and the anticheat settings reach a running server through deploy
            if server::Server::new(&cfg).is_installed() {
                let a = a.clone();
                bg(
                    move || deploy::deploy(&cfg, false, true),
                    move |r| {
                        if let Err(e) = r {
                            a.error(&e);
                        }
                    },
                );
            }
            if new_language {
                a.toast(&t("Restart the app to apply the language"));
                return;
            }
            a.toast(&if restart && a.live.borrow().running { t("Restart the server to apply network changes") } else { t("Saved") });
        }
        Err(e) => a.error(&e),
    }
}

pub fn init(a: &AppRc) {
    fill(a);
    let s = a.s();
    let app = a.clone();
    s.on_set_save(move || save(&app));
    let app = a.clone();
    s.on_set_redeploy(move || {
        let cfg = app.cfg.borrow().clone();
        let a = app.clone();
        bg(
            move || deploy::deploy(&cfg, true, true),
            move |r: anyhow::Result<deploy::Report>| match r {
                Ok(r) => match r.error_text() {
                    Some(e) => a.error(&anyhow::anyhow!(e)),
                    None => a.toast(&format!("{}: {}", t("Recompile plugins"), r.compiled.join(", "))),
                },
                Err(e) => a.error(&e),
            },
        );
    });
    let app = a.clone();
    s.on_set_update(move || {
        if app.live.borrow().running {
            app.error(&anyhow::anyhow!(t("Stop the server before updating it")));
            return;
        }
        match server::spawn_install(true) {
            Ok(()) => app.toast(&t("Installing…")),
            Err(e) => app.error(&e),
        }
    });
}
