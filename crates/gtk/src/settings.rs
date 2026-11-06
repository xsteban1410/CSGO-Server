//! Settings tab: config.toml, maintenance and diagnostics.
use crate::i18n::t;
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::config::Network;
use csls_core::server::{self, Server};
use csls_core::{deploy, install, net};
use gtk::glib;

pub fn page(a: &AppRc) -> gtk::Widget {
    let c = a.cfg.borrow().server.clone();
    let page = adw::PreferencesPage::new();

    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Server"));
    let name = adw::EntryRow::builder().title(t("Server name")).text(&c.hostname).build();
    let pass = adw::PasswordEntryRow::builder().title(t("Password (empty = none)")).text(&c.password).build();
    let nets = [Network::Lan, Network::Vpn, Network::Internet, Network::Community];
    let net_row = adw::ComboRow::builder()
        .title(t("Network"))
        .model(&gtk::StringList::new(&[&t("LAN only"), &t("Friends over VPN"), &t("Internet (open port)"), &t("Community (Steam server list)")]))
        .selected(nets.iter().position(|n| *n == c.network).unwrap_or(0) as u32)
        .build();
    let port = adw::SpinRow::with_range(1024.0, 65000.0, 1.0);
    port.set_title(&t("Port"));
    port.set_value(c.port as f64);
    let maxp = adw::SpinRow::with_range(2.0, 64.0, 1.0);
    maxp.set_title(&t("Max players"));
    maxp.set_value(c.max_players as f64);
    let tick = adw::ComboRow::builder().title(t("Tickrate")).model(&gtk::StringList::new(&["64", "128"])).selected(if c.tickrate >= 128 { 1 } else { 0 }).build();
    let gslt = adw::PasswordEntryRow::builder().title(t("GSLT token (community)")).text(&c.gslt).build();
    let wkey = adw::PasswordEntryRow::builder().title(t("Workshop Web API key")).text(&c.workshop_authkey).build();
    let upnp = adw::SwitchRow::builder().title(t("Open the port with UPnP")).subtitle(t("Internet and community profiles; needs upnpc (miniupnpc)")).active(c.upnp).build();
    net_row.bind_property("selected", &upnp, "visible").transform_to(|_, s: u32| Some(s >= 2)).sync_create().build();
    let skins = adw::SwitchRow::builder().title(t("Skins plugin")).subtitle(t("Can cost the GSLT on public servers")).active(c.skins).build();
    let folder = adw::ActionRow::builder().title(t("Server folder")).subtitle(c.install_dir.display().to_string()).build();
    folder.add_css_class("property");
    for r in [name.upcast_ref::<gtk::Widget>(), pass.upcast_ref(), net_row.upcast_ref(), upnp.upcast_ref(), port.upcast_ref(), maxp.upcast_ref(), tick.upcast_ref(), gslt.upcast_ref(), wkey.upcast_ref(), skins.upcast_ref(), folder.upcast_ref()] {
        g.add(r);
    }
    let save = button(&t("Save"), &["suggested-action"]);
    save.set_valign(gtk::Align::Center);
    g.set_header_suffix(Some(&save));
    page.add(&g);

    save.connect_clicked(glib::clone!(#[strong] a, move |_| {
        let mut cfg = a.cfg.borrow().clone();
        let old = cfg.server.clone();
        let s = &mut cfg.server;
        s.hostname = name.text().to_string();
        s.password = pass.text().to_string();
        s.network = nets[net_row.selected().min(3) as usize];
        s.port = port.value() as u16;
        s.max_players = maxp.value() as u32;
        s.tickrate = if tick.selected() == 1 { 128 } else { 64 };
        s.gslt = gslt.text().trim().to_string();
        s.workshop_authkey = wkey.text().trim().to_string();
        s.skins = skins.is_active();
        s.upnp = upnp.is_active();
        let restart = (s.network, s.port, s.tickrate, s.max_players, &s.gslt) != (old.network, old.port, old.tickrate, old.max_players, &old.gslt);
        match cfg.save() {
            Ok(()) => {
                *a.cfg.borrow_mut() = cfg;
                a.toast(&if restart && a.live.borrow().running { t("Restart the server to apply network changes") } else { t("Saved") });
            }
            Err(e) => a.error(&e),
        }
    }));

    let m = adw::PreferencesGroup::new();
    m.set_title(&t("Maintenance"));
    let redeploy = adw::ButtonRow::builder().title(t("Recompile plugins")).build();
    let update = adw::ButtonRow::builder().title(t("Update server and SourceMod")).build();
    m.add(&redeploy);
    m.add(&update);
    page.add(&m);
    redeploy.connect_activated(glib::clone!(#[strong] a, move |_| {
        let cfg = a.cfg.borrow().clone();
        bg(move || deploy::deploy(&cfg, true, true), glib::clone!(#[strong] a, move |r: anyhow::Result<deploy::Report>| match r {
            Ok(r) => match r.error_text() {
                Some(e) => a.error(&anyhow::anyhow!(e)),
                None => a.toast(&format!("{}: {}", t("Recompile plugins"), r.compiled.join(", "))),
            },
            Err(e) => a.error(&e),
        }));
    }));
    update.connect_activated(glib::clone!(#[strong] a, move |_| {
        if a.live.borrow().running {
            a.error(&anyhow::anyhow!(t("Stop the server?")));
            return;
        }
        match server::spawn_install(true) {
            Ok(()) => a.toast(&t("Installing…")),
            Err(e) => a.error(&e),
        }
    }));

    let d = adw::PreferencesGroup::new();
    d.set_title(&t("Diagnosis"));
    let missing = install::missing_deps();
    let deps = adw::ActionRow::builder()
        .title(t("Dependencies"))
        .subtitle(if missing.is_empty() { t("installed") } else { format!("{}: {} — {}", t("missing"), missing.join(", "), install::deps_command()) })
        .subtitle_selectable(true)
        .build();
    deps.add_css_class("property");
    d.add(&deps);
    let p = a.cfg.borrow().server.port;
    if let Some(h) = net::firewall_hint(p) {
        let fw = adw::ActionRow::builder().title(t("Firewall")).subtitle(&h).subtitle_selectable(true).build();
        fw.add_css_class("property");
        d.add(&fw);
    }
    let sd = adw::ActionRow::builder().title("systemd --user").subtitle(if server::systemd_available() { t("installed") } else { t("missing") }).build();
    sd.add_css_class("property");
    d.add(&sd);
    let installed = Server::new(&a.cfg.borrow()).is_installed();
    let inst = adw::ActionRow::builder().title(t("Server folder")).subtitle(if installed { t("Installed") } else { t("Not installed") }).build();
    inst.add_css_class("property");
    d.add(&inst);
    page.add(&d);
    page.upcast()
}
