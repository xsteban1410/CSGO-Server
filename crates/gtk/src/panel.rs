//! Pestaña Servidor: estado, dirección para conectar e Iniciar / Cambiar / Detener / Jugar.
use crate::i18n::t;
use crate::{bg, button, wizard, AppRc};
use adw::prelude::*;
use csls_core::server::Server;
use csls_core::steam_client;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

pub fn page(a: &AppRc) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Server"));
    let state = adw::ActionRow::builder().title(t("Status")).build();
    let mode = adw::ActionRow::builder().title(t("Mode")).build();
    let map = adw::ActionRow::builder().title(t("Map")).build();
    let players = adw::ActionRow::builder().title(t("Players")).build();
    let addr = adw::ActionRow::builder().title(t("Connect address")).build();
    addr.add_css_class("property");
    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_valign(gtk::Align::Center);
    copy.add_css_class("flat");
    copy.set_tooltip_text(Some(&t("Copy")));
    addr.add_suffix(&copy);
    // perfil VPN: lo que hay que pasar a los amigos y si el puente está en marcha
    let vpn_addr = adw::ActionRow::builder().title(t("Address for friends over VPN")).build();
    let vpn_copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    vpn_copy.set_valign(gtk::Align::Center);
    vpn_copy.add_css_class("flat");
    vpn_copy.set_tooltip_text(Some(&t("Copy")));
    vpn_addr.add_suffix(&vpn_copy);
    let bridge = adw::ActionRow::builder().title(t("VPN bridge")).build();
    let bridge_start = gtk::Button::with_label(&t("Start"));
    bridge_start.set_valign(gtk::Align::Center);
    bridge.add_suffix(&bridge_start);
    for r in [&state, &mode, &map, &players, &addr, &vpn_addr, &bridge] {
        r.add_css_class("property");
        g.add(r);
    }
    page.add(&g);

    let start = button(&t("Start server"), &["suggested-action", "pill"]);
    let change = button(&t("Change mode or map"), &["pill"]);
    let stop = button(&t("Stop"), &["destructive-action", "pill"]);
    let play = button(&t("Play"), &["pill"]);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bx.set_halign(gtk::Align::Center);
    bx.set_margin_top(12);
    for b in [&start, &change, &stop, &play] {
        bx.append(b);
    }
    // controles de partida (con el servidor encendido)
    let gl = adw::PreferencesGroup::new();
    gl.set_title(&t("Match"));
    let lb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    lb.set_halign(gtk::Align::Center);
    let restart = button(&t("Restart round"), &["pill"]);
    let warm = button(&t("End warmup"), &["pill"]);
    let addb = button(&t("+ Bot"), &["pill"]);
    let delb = button(&t("− Bot"), &["pill"]);
    for b in [&restart, &warm, &addb, &delb] {
        lb.append(b);
    }
    gl.add(&lb);
    let quick = |b: &gtk::Button, f: fn(&Server) -> anyhow::Result<String>| {
        b.connect_clicked(glib::clone!(#[strong] a, move |_| {
            let cfg = a.cfg.borrow().clone();
            bg(move || f(&Server::new(&cfg)), glib::clone!(#[strong] a, move |r: anyhow::Result<String>| match r {
                Ok(m) => {
                    a.toast(&m);
                    a.refresh();
                }
                Err(e) => a.error(&e),
            }));
        }));
    };
    quick(&restart, |s| s.restart_round().map(|_| t("Round restarted")));
    quick(&warm, |s| s.end_warmup().map(|_| t("Warmup ended")));
    quick(&addb, |s| s.add_bots(1).map(|q| format!("bot_quota {q}")));
    quick(&delb, |s| s.add_bots(-1).map(|q| format!("bot_quota {q}")));

    // presets guardados
    let gp = adw::PreferencesGroup::new();
    gp.set_title(&t("Presets"));
    gp.set_description(Some(&t("Save one from the last step of “Start server”")));
    let prows: Rc<RefCell<Vec<adw::ActionRow>>> = Rc::default();
    let fill = Rc::new(glib::clone!(#[strong] a, #[weak] gp, #[strong] prows, move || {
        for r in prows.borrow_mut().drain(..) {
            gp.remove(&r);
        }
        let on = a.live.borrow().rcon_ok;
        let list = csls_core::presets::Presets::load().unwrap_or_default();
        for (name, o) in list.0 {
            let sub = format!("{}{}", o.mode, o.map.as_deref().map(|m| format!(" · {m}")).unwrap_or_default());
            let row = adw::ActionRow::builder().title(&name).subtitle(&sub).build();
            let go = button(&if on { t("Apply") } else { t("Start") }, &["flat"]);
            go.set_valign(gtk::Align::Center);
            let del = gtk::Button::from_icon_name("user-trash-symbolic");
            del.set_valign(gtk::Align::Center);
            del.add_css_class("flat");
            del.set_tooltip_text(Some(&t("Delete")));
            go.connect_clicked(glib::clone!(#[strong] a, move |_| {
                let live = a.live.borrow().rcon_ok;
                wizard::run_preset(&a, o.clone(), live);
            }));
            let n = name.clone();
            del.connect_clicked(glib::clone!(#[strong] a, move |_| match csls_core::presets::Presets::remove(&n) {
                Ok(()) => a.modes_changed(),
                Err(e) => a.error(&e),
            }));
            row.add_suffix(&go);
            row.add_suffix(&del);
            gp.add(&row);
            prows.borrow_mut().push(row);
        }
        gp.set_visible(!prows.borrow().is_empty());
    }));
    fill();
    a.on_modes_changed(glib::clone!(#[strong] fill, move || fill()));

    let gb = adw::PreferencesGroup::new();
    gb.add(&bx);
    let note = gtk::Label::new(Some(&t("The server keeps running when you close the app.")));
    note.add_css_class("dim-label");
    note.add_css_class("caption");
    note.set_margin_top(12);
    gb.add(&note);
    page.add(&gb);
    page.add(&gl);
    page.add(&gp);

    copy.connect_clicked(glib::clone!(#[weak] addr, #[strong] a, move |b| {
        let sub = addr.subtitle().unwrap_or_default();
        b.clipboard().set_text(&format!("connect {}", sub.split_whitespace().next().unwrap_or("")));
        a.toast(&t("Copied"));
    }));
    vpn_copy.connect_clicked(glib::clone!(#[weak] vpn_addr, #[strong] a, move |b| {
        b.clipboard().set_text(&vpn_addr.subtitle().unwrap_or_default());
        a.toast(&t("Copied"));
    }));
    bridge_start.connect_clicked(glib::clone!(#[strong] a, move |_| {
        let cfg = a.cfg.borrow().clone();
        bg(move || Server::new(&cfg).start_bridge(), glib::clone!(#[strong] a, move |r: anyhow::Result<()>| {
            if let Err(e) = r {
                a.error(&e);
            }
            a.refresh();
        }));
    }));
    start.connect_clicked(glib::clone!(#[strong] a, move |_| wizard::open(&a, false)));
    change.connect_clicked(glib::clone!(#[strong] a, move |_| wizard::open(&a, true)));
    stop.connect_clicked(glib::clone!(#[strong] a, move |_| {
        let d = adw::AlertDialog::new(Some(&t("Stop the server?")), Some(&t("Players will be disconnected.")));
        d.add_response("cancel", &t("Cancel"));
        d.add_response("stop", &t("Stop"));
        d.set_response_appearance("stop", adw::ResponseAppearance::Destructive);
        d.connect_response(None, glib::clone!(#[strong] a, move |_, r| {
            if r != "stop" {
                return;
            }
            let cfg = a.cfg.borrow().clone();
            bg(move || Server::new(&cfg).stop(), glib::clone!(#[strong] a, move |r| {
                match r {
                    Ok(()) => a.toast(&t("Server stopped")),
                    Err(e) => a.error(&e),
                }
                a.refresh();
            }));
        }));
        d.present(Some(&a.win));
    }));
    play.connect_clicked(glib::clone!(#[strong] a, move |play| {
        let c = Server::new(&a.cfg.borrow()).connect_address();
        match steam_client::launch_csgo(Some(&c)) {
            Ok(_) => {
                play.clipboard().set_text(&format!("connect {c}"));
                a.toast(&t("Opening. If it does not join by itself, paste the copied command in the console"));
            }
            Err(e) => a.error(&e),
        }
    }));

    // dirección: la local y, con perfil de internet, también la pública (petición en segundo plano)
    let set_addr = Rc::new(glib::clone!(#[strong] a, #[weak] addr, #[weak] vpn_addr, move || {
        let cfg = a.cfg.borrow().clone();
        let local = Server::new(&cfg).connect_address();
        addr.set_subtitle(&local);
        let is_vpn = cfg.server.network == csls_core::config::Network::Vpn;
        vpn_addr.set_visible(is_vpn);
        if is_vpn {
            vpn_addr.set_subtitle(&Server::new(&cfg).vpn_address().unwrap_or_else(|| t("No Tailscale address found: use this PC's VPN IP")));
        }
        if Server::new(&cfg).public() {
            bg(move || Server::new(&cfg).public_address(), glib::clone!(#[weak] addr, move |p: Option<String>| {
                if let Some(p) = p {
                    addr.set_subtitle(&format!("{p}  ·  LAN {local}"));
                }
            }));
        }
    }));
    set_addr();
    let was_on = std::cell::Cell::new(None);
    let on_live_a = a.clone();
    a.on_live(move |l| {
        let on = l.running;
        let is_vpn = on_live_a.cfg.borrow().server.network == csls_core::config::Network::Vpn;
        bridge.set_visible(is_vpn && on);
        bridge.set_subtitle(&if l.bridge { t("Running") } else { t("Not running: friends cannot join") });
        bridge_start.set_visible(!l.bridge);
        gl.set_visible(l.rcon_ok);
        if was_on.replace(Some(l.rcon_ok)) != Some(l.rcon_ok) {
            fill();
            set_addr();
        }
        state.set_subtitle(&if !on { t("Stopped") } else if l.rcon_ok { t("Running") } else { t("Starting…") });
        mode.set_subtitle(if l.mode.is_empty() { "—" } else { &l.mode });
        map.set_subtitle(if l.status.map.is_empty() { "—" } else { &l.status.map });
        players.set_subtitle(&if on { format!("{} + {} bots / {}", l.status.humans, l.status.bots, l.status.max) } else { "—".into() });
        start.set_visible(!on);
        change.set_visible(on && l.rcon_ok);
        stop.set_visible(on);
    });
    page.upcast()
}
