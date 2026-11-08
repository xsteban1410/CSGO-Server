//! Admins in the Players tab: who may use admin commands in game, with a role each.
//! Roles that give the server console also need `sm_login <password>`; the password is copied, never shown.
use crate::i18n::t;
use crate::{bg, btn, model, row, strings, AppRc};
use csls_core::admins::User;
use csls_core::config::Config;
use csls_core::server::Server;
use std::cell::RefCell;
use std::rc::Rc;

fn fill(a: &AppRc) {
    let cfg = a.cfg.borrow();
    let rows = cfg
        .admins
        .users
        .iter()
        .map(|u| {
            let login = if u.password.is_empty() { String::new() } else { format!(" · {}", t("console with sm_login")) };
            let mut del = btn("del", &t("Remove"));
            del.danger = true;
            let mut b = vec![btn("edit", &t("Edit"))];
            if !u.password.is_empty() {
                b.push(btn("pass", &t("New password")));
            }
            b.push(del);
            row(&u.steamid, &u.name, &format!("{} · {}{login}", u.steamid, u.role), b)
        })
        .collect();
    let roles: Vec<String> = cfg
        .admins
        .roles
        .iter()
        .map(|r| format!("{} ({})", r.name, r.permissions.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")))
        .collect();
    let s = a.s();
    s.set_ad_rows(model(rows));
    s.set_ad_roles(strings(&roles));
}

/// Saves a changed config, deploys the admins file if there is a server, and copies a new password.
fn save(a: &AppRc, cfg: Config, password: Option<String>) {
    if let Err(e) = cfg.save() {
        return a.error(&e);
    }
    *a.cfg.borrow_mut() = cfg.clone();
    fill(a);
    if let Some(p) = password {
        a.copy(p);
        a.toast(&t("Password copied. Give it to them privately: they type sm_login <password> in the game console"));
    }
    if Server::new(&cfg).is_installed() {
        let a = a.clone();
        bg(
            move || csls_core::deploy::deploy(&cfg, false, true),
            move |r| {
                if let Err(e) = r {
                    a.error(&e);
                }
            },
        );
    }
}

fn open(a: &AppRc, steamid: &str, name: &str, role: &str) {
    let s = a.s();
    let roles = &a.cfg.borrow().admins.roles;
    let i = roles.iter().position(|r| r.name == role).or_else(|| roles.iter().position(|r| r.name == "admin")).unwrap_or(0);
    s.set_ad_steamid(steamid.into());
    s.set_ad_name(name.into());
    s.set_ad_role(i as i32);
    s.set_ad_open(true);
}

pub fn init(a: &AppRc) {
    fill(a);
    let s = a.s();
    // the SteamID being edited, empty when adding
    let editing: Rc<RefCell<String>> = Rc::default();
    let (app, ed) = (a.clone(), editing.clone());
    s.on_ad_add(move || {
        ed.borrow_mut().clear();
        app.s().set_ad_title(t("Add admin").into());
        open(&app, "", "", "admin");
    });
    let (app, ed) = (a.clone(), editing.clone());
    s.on_ad_make(move |steamid, name| {
        let existing = app.cfg.borrow().admins.find(&steamid).map(|i| app.cfg.borrow().admins.users[i].clone());
        match existing {
            Some(u) => {
                *ed.borrow_mut() = u.steamid.clone();
                app.s().set_ad_title(t("Edit admin").into());
                open(&app, &u.steamid, &u.name, &u.role);
            }
            None => {
                ed.borrow_mut().clear();
                app.s().set_ad_title(t("Add admin").into());
                open(&app, &steamid, &name, "admin");
            }
        }
    });
    let (app, ed) = (a.clone(), editing.clone());
    s.on_ad_action(move |id, action| {
        let mut cfg = app.cfg.borrow().clone();
        let Some(i) = cfg.admins.find(&id) else { return };
        match action.as_str() {
            "edit" => {
                *ed.borrow_mut() = id.to_string();
                let u = cfg.admins.users[i].clone();
                app.s().set_ad_title(t("Edit admin").into());
                open(&app, &u.steamid, &u.name, &u.role);
            }
            "pass" => match csls_core::config::random_password() {
                Ok(p) => {
                    cfg.admins.users[i].password = p.clone();
                    save(&app, cfg, Some(p));
                }
                Err(e) => app.error(&e),
            },
            "del" => {
                let (a, name) = (app.clone(), cfg.admins.users[i].name.clone());
                app.ask(
                    &format!("{} {name}?", t("Remove")),
                    &t("They lose their admin commands on the server."),
                    &t("Remove"),
                    &t("Cancel"),
                    true,
                    move |yes| {
                        if yes {
                            cfg.admins.users.remove(i);
                            save(&a, cfg, None);
                        }
                    },
                );
            }
            _ => {}
        }
    });
    let app = a.clone();
    s.on_ad_save(move || {
        let s = app.s();
        let mut cfg = app.cfg.borrow().clone();
        let Some(role) = cfg.admins.roles.get(s.get_ad_role().max(0) as usize).cloned() else { return };
        let steamid = s.get_ad_steamid().trim().to_string();
        let name = s.get_ad_name().trim().to_string();
        let old = editing.borrow().clone();
        let i = if old.is_empty() { None } else { cfg.admins.find(&old) };
        if cfg.admins.find(&steamid).is_some_and(|j| Some(j) != i) {
            return app.error(&anyhow::anyhow!("{steamid}: {}", t("already an admin")));
        }
        let mut u = i.map(|i| cfg.admins.users[i].clone()).unwrap_or_default();
        let mut password = None;
        if role.permissions.iter().any(|p| p.needs_login()) && u.password.is_empty() {
            match csls_core::config::random_password() {
                Ok(p) => {
                    u.password = p.clone();
                    password = Some(p);
                }
                Err(e) => return app.error(&e),
            }
        }
        u = User { steamid, name, role: role.name, ..u };
        match i {
            Some(i) => cfg.admins.users[i] = u,
            None => cfg.admins.users.push(u),
        }
        // the same checks as the config file, before closing the dialog
        if let Err(e) = cfg.admins.validate() {
            return app.error(&e);
        }
        s.set_ad_open(false);
        save(&app, cfg, password);
    });
}
