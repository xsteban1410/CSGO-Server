//! Join tab: open CS:GO Legacy connecting to a server, with a VPN bridge if needed.
use crate::i18n::t;
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::{net, server, steam_client};
use gtk::glib;
use std::time::Duration;

/// "IP" or "IP:PORT", with the default port if absent.
fn with_port(addr: &str) -> String {
    let a = addr.trim();
    if a.is_empty() || a.contains(':') { a.to_string() } else { format!("{a}:27030") }
}

pub fn page(a: &AppRc) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Join"));
    let addr = adw::EntryRow::builder().title(t("Server address")).build();
    let vpn = adw::SwitchRow::builder().title(t("Through VPN bridge")).subtitle(t("Turns on by itself with a Tailscale address (100.x). The server must use the VPN profile")).build();
    // the real address the game connects to: by hand in the console if auto-connect fails
    let target = adw::ActionRow::builder().title(t("Type in the game console")).subtitle_selectable(true).build();
    target.add_css_class("property");
    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_valign(gtk::Align::Center);
    copy.add_css_class("flat");
    copy.set_tooltip_text(Some(&t("Copy")));
    target.add_suffix(&copy);
    target.set_visible(false);
    let state = adw::ActionRow::builder().title(t("Bridge running")).build();
    let stopb = gtk::Button::with_label(&t("Stop bridge"));
    stopb.set_valign(gtk::Align::Center);
    state.add_suffix(&stopb);
    state.set_visible(server::bridge_running());
    g.add(&addr);
    g.add(&vpn);
    g.add(&target);
    g.add(&state);
    page.add(&g);

    addr.connect_changed(glib::clone!(#[weak] vpn, move |e| {
        let text = e.text();
        if net::is_vpn_ip(text.trim().split(':').next().unwrap_or("")) {
            vpn.set_active(true);
        }
    }));

    let play = button(&t("Open CS:GO Legacy"), &["suggested-action", "pill"]);
    let test = button(&t("Test connection"), &["pill"]);
    let cs2 = button(&t("Open CS2"), &["pill"]);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bx.set_halign(gtk::Align::Center);
    bx.append(&play);
    bx.append(&test);
    bx.append(&cs2);
    let gb = adw::PreferencesGroup::new();
    gb.add(&bx);
    page.add(&gb);

    copy.connect_clicked(glib::clone!(#[weak] target, #[strong] a, move |b| {
        b.clipboard().set_text(&target.subtitle().unwrap_or_default());
        a.toast(&t("Copied"));
    }));
    stopb.connect_clicked(glib::clone!(#[weak] state, move |_| {
        server::stop_bridge();
        state.set_visible(false);
    }));
    cs2.connect_clicked(glib::clone!(#[strong] a, move |_| {
        if let Err(e) = steam_client::launch_cs2() {
            a.error(&e);
        }
    }));

    // Prepares the connection: with a bridge, starts it and returns the address to connect to.
    let prepare = glib::clone!(#[weak] addr, #[weak] vpn, #[weak] target, #[weak] state, #[upgrade_or] Ok(String::new()), move || -> anyhow::Result<String> {
        let dest = with_port(&addr.text());
        if dest.is_empty() {
            target.set_visible(false);
            return Ok(dest);
        }
        let connect = if vpn.is_active() { server::join_through_bridge(&dest)? } else { dest };
        state.set_visible(server::bridge_running());
        target.set_subtitle(&format!("connect {connect}"));
        target.set_visible(true);
        Ok(connect)
    });

    // A2S_INFO through the bridge: checks the whole path without opening the game.
    let check = std::rc::Rc::new(glib::clone!(#[strong] a, move |connect: String| {
        let Ok(sock) = connect.parse() else { return };
        bg(move || net::a2s_info(sock, Duration::from_secs(8)), glib::clone!(#[strong] a, move |r: Option<String>| {
            match r {
                Some(name) => a.toast(&format!("{}: {name}", t("The server answers"))),
                None => a.toast(&t("No answer: is the server on, with the VPN profile, and is your VPN connected?")),
            }
        }));
    }));

    test.connect_clicked(glib::clone!(#[strong] a, #[strong] prepare, #[strong] check, move |_| match prepare() {
        Ok(c) if !c.is_empty() => check(c),
        Ok(_) => a.toast(&t("Write the server address first")),
        Err(e) => a.error(&e),
    }));
    play.connect_clicked(glib::clone!(#[strong] a, #[strong] prepare, #[strong] check, move |b| {
        let r = prepare().and_then(|c| {
            steam_client::launch_csgo((!c.is_empty()).then_some(c.as_str())).map(drop)?;
            Ok(c)
        });
        match r {
            Ok(c) if !c.is_empty() => {
                b.clipboard().set_text(&format!("connect {c}"));
                a.toast(&t("Opening. If it does not join by itself, paste the copied command in the console"));
                check(c);
            }
            Ok(_) => {}
            Err(e) => a.error(&e),
        }
    }));
    page.upcast()
}
