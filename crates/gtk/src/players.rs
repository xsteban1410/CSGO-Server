//! Players tab: connected (kick, ban, skins), saved skins and bans.
use crate::i18n::t;
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::players::{self, Player};
use csls_core::server::Server;
use csls_core::skins;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

fn rcon(a: &AppRc, cmd: String) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    bg(move || Server::new(&cfg).rcon()?.exec(&cmd), move |r: anyhow::Result<String>| {
        match r {
            Ok(o) if !o.trim().is_empty() => a.toast(o.trim()),
            Ok(_) => {}
            Err(e) => a.error(&e),
        }
        a.refresh();
    });
}

fn skins_dialog(a: &AppRc, title: &str, steamid64: String) {
    let cfg = a.cfg.borrow().clone();
    let a = a.clone();
    let title = title.to_string();
    bg(
        move || -> anyhow::Result<Vec<skins::Entry>> {
            let csgo = Server::new(&cfg).csgo_dir();
            let cat = skins::Catalog::load(&csgo.join("addons/sourcemod/configs/skins.cfg"))?;
            skins::player_skins(&skins::db_path(&csgo), &cat, &steamid64)
        },
        move |r| {
            let page = adw::PreferencesPage::new();
            let g = adw::PreferencesGroup::new();
            match r {
                Ok(v) if v.is_empty() => g.add(&adw::ActionRow::builder().title(t("Nothing saved")).build()),
                Ok(v) => {
                    for e in v {
                        let row = adw::ActionRow::builder().title(t(&e.weapon)).subtitle(&e.value).build();
                        row.add_css_class("property");
                        g.add(&row);
                    }
                }
                Err(e) => g.add(&adw::ActionRow::builder().title(t("Error")).subtitle(format!("{e:#}")).build()),
            }
            page.add(&g);
            let tv = adw::ToolbarView::new();
            tv.add_top_bar(&adw::HeaderBar::new());
            tv.set_content(Some(&page));
            let d = adw::Dialog::builder().title(format!("{} · {title}", t("Skins"))).content_width(440).content_height(520).child(&tv).build();
            d.present(Some(&a.win));
        },
    );
}

fn ban_dialog(a: &AppRc, p: &Player) {
    let d = adw::AlertDialog::new(Some(&format!("{} {}", t("Ban"), p.name)), None);
    let bx = gtk::ListBox::new();
    bx.add_css_class("boxed-list");
    let mins = adw::SpinRow::with_range(0.0, 525600.0, 30.0);
    mins.set_title(&t("Minutes (0 = permanent)"));
    mins.set_value(60.0);
    let reason = adw::EntryRow::builder().title(t("Reason")).build();
    bx.append(&mins);
    bx.append(&reason);
    d.set_extra_child(Some(&bx));
    d.add_response("cancel", &t("Cancel"));
    d.add_response("ban", &t("Ban"));
    d.set_response_appearance("ban", adw::ResponseAppearance::Destructive);
    let uid = p.userid;
    d.connect_response(None, glib::clone!(#[strong] a, move |_, r| {
        if r == "ban" {
            rcon(&a, players::ban_cmd(uid, mins.value() as u32, &reason.text()));
        }
    }));
    d.present(Some(&a.win));
}

pub fn page(a: &AppRc) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let now = adw::PreferencesGroup::new();
    now.set_title(&t("Connected now"));
    let now_list = gtk::ListBox::new();
    now_list.add_css_class("boxed-list");
    now_list.set_selection_mode(gtk::SelectionMode::None);
    now.add(&now_list);
    page.add(&now);

    let saved = adw::PreferencesGroup::new();
    saved.set_title(&t("Saved skins"));
    saved.set_description(Some(&t("Players with skins saved, also offline")));
    let saved_list = gtk::ListBox::new();
    saved_list.add_css_class("boxed-list");
    saved_list.set_selection_mode(gtk::SelectionMode::None);
    saved.add(&saved_list);
    page.add(&saved);

    let bans = adw::PreferencesGroup::new();
    bans.set_title(&t("Bans"));
    let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh.add_css_class("flat");
    refresh.set_tooltip_text(Some(&t("Refresh")));
    bans.set_header_suffix(Some(&refresh));
    let bans_list = gtk::ListBox::new();
    bans_list.add_css_class("boxed-list");
    bans_list.set_selection_mode(gtk::SelectionMode::None);
    bans.add(&bans_list);
    page.add(&bans);

    let reload_offline = Rc::new(glib::clone!(#[strong] a, #[weak] saved_list, #[weak] bans_list, move || {
        let cfg = a.cfg.borrow().clone();
        let a2 = a.clone();
        bg(
            move || {
                let csgo = Server::new(&cfg).csgo_dir();
                (skins::players(&skins::db_path(&csgo)).unwrap_or_default(), players::read_bans(&csgo))
            },
            move |(ids, banned)| {
                saved_list.remove_all();
                bans_list.remove_all();
                for id in ids {
                    let row = adw::ActionRow::builder().title(&id).build();
                    let b = gtk::Button::with_label(&t("Skins"));
                    b.set_valign(gtk::Align::Center);
                    let idc = id.clone();
                    b.connect_clicked(glib::clone!(#[strong] a2, move |_| skins_dialog(&a2, &idc, idc.clone())));
                    row.add_suffix(&b);
                    saved_list.append(&row);
                }
                for s in banned {
                    let row = adw::ActionRow::builder().title(&s).build();
                    let b = gtk::Button::with_label(&t("Unban"));
                    b.set_valign(gtk::Align::Center);
                    let sc = s.clone();
                    b.connect_clicked(glib::clone!(#[strong] a2, move |_| match players::unban_cmd(&sc) {
                        Ok(c) => rcon(&a2, c),
                        Err(e) => a2.error(&e),
                    }));
                    row.add_suffix(&b);
                    bans_list.append(&row);
                }
            },
        );
    }));
    refresh.connect_clicked(glib::clone!(#[strong] reload_offline, move |_| reload_offline()));
    reload_offline();

    let last: Rc<RefCell<Vec<Player>>> = Rc::new(RefCell::new(vec![Player { userid: u32::MAX, name: String::new(), steamid: String::new(), bot: false, connected: String::new(), ping: 0, loss: 0, addr: String::new() }]));
    a.on_live(glib::clone!(#[strong] a, move |l| {
        let ps = &l.status.players;
        // only rebuilt when something changes (otherwise the buttons flicker every 3 s)
        let key: Vec<Player> = ps.iter().map(|p| Player { connected: String::new(), ping: p.ping / 20, ..p.clone() }).collect();
        if *last.borrow() == key {
            return;
        }
        *last.borrow_mut() = key;
        now_list.remove_all();
        if ps.is_empty() {
            now_list.append(&adw::ActionRow::builder().title(t("No players")).build());
        }
        for p in ps {
            let sub = if p.bot { "BOT".to_string() } else { format!("{} · {} ms · {}", p.steamid, p.ping, p.connected) };
            let row = adw::ActionRow::builder().title(glib::markup_escape_text(&p.name)).subtitle(sub).build();
            if let Some(id64) = p.steamid64() {
                let s = button(&t("Skins"), &["flat"]);
                s.set_valign(gtk::Align::Center);
                let n = p.name.clone();
                s.connect_clicked(glib::clone!(#[strong] a, move |_| skins_dialog(&a, &n, id64.to_string())));
                row.add_suffix(&s);
            }
            let k = button(&t("Kick"), &["flat"]);
            k.set_valign(gtk::Align::Center);
            let uid = p.userid;
            k.connect_clicked(glib::clone!(#[strong] a, move |_| rcon(&a, players::kick_cmd(uid, ""))));
            row.add_suffix(&k);
            if !p.bot {
                let b = button(&t("Ban"), &["flat", "error"]);
                b.set_valign(gtk::Align::Center);
                let pc = p.clone();
                b.connect_clicked(glib::clone!(#[strong] a, move |_| ban_dialog(&a, &pc)));
                row.add_suffix(&b);
            }
            now_list.append(&row);
        }
    }));
    page.upcast()
}
