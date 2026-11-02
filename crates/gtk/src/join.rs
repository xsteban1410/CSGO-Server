//! Pestaña Unirse: abrir CS:GO Legacy conectando a un servidor, con puente VPN si hace falta.
use crate::i18n::t;
use crate::{button, AppRc};
use adw::prelude::*;
use csls_core::{net, server, steam_client};
use gtk::glib;

pub fn page(a: &AppRc) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Join"));
    let addr = adw::EntryRow::builder().title(t("Server address")).build();
    let vpn = adw::SwitchRow::builder().title(t("Through VPN bridge")).subtitle(t("For friends over Tailscale/ZeroTier with the server in VPN mode")).build();
    let state = adw::ActionRow::builder().title(t("Bridge running")).build();
    let stopb = gtk::Button::with_label(&t("Stop bridge"));
    stopb.set_valign(gtk::Align::Center);
    state.add_suffix(&stopb);
    state.set_visible(server::bridge_running());
    g.add(&addr);
    g.add(&vpn);
    g.add(&state);
    page.add(&g);

    let play = button(&t("Open CS:GO Legacy"), &["suggested-action", "pill"]);
    let cs2 = button(&t("Open CS2"), &["pill"]);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bx.set_halign(gtk::Align::Center);
    bx.append(&play);
    bx.append(&cs2);
    let gb = adw::PreferencesGroup::new();
    gb.add(&bx);
    page.add(&gb);

    stopb.connect_clicked(glib::clone!(#[weak] state, move |_| {
        server::stop_bridge();
        state.set_visible(false);
    }));
    cs2.connect_clicked(glib::clone!(#[strong] a, move |_| {
        if let Err(e) = steam_client::launch_cs2() {
            a.error(&e);
        }
    }));
    play.connect_clicked(glib::clone!(#[strong] a, #[weak] state, move |_| {
        let mut target = addr.text().trim().to_string();
        if !target.is_empty() && !target.contains(':') {
            target += ":27030";
        }
        let r = (|| -> anyhow::Result<()> {
            if vpn.is_active() && !target.is_empty() {
                // el servidor recibe a los amigos en PUERTO+1; aquí se escucha en un puerto libre
                let (ip, p) = target.rsplit_once(':').unwrap();
                let port: u16 = p.parse()?;
                let listen = [27030u16, 27040, 27050].into_iter().find(|p| net::udp_port_free(*p)).ok_or_else(|| anyhow::anyhow!("sin puerto libre"))?;
                server::start_friend_bridge(listen, &format!("{ip}:{}", port + 1))?;
                state.set_visible(true);
                let local = net::local_ip().ok_or_else(|| anyhow::anyhow!("sin IP local"))?;
                // Steam rechaza conectar a la IP de la VPN: se conecta a la IP local propia
                steam_client::launch_csgo(Some(&format!("{local}:{listen}")))
            } else {
                steam_client::launch_csgo((!target.is_empty()).then_some(target.as_str()))
            }
        })();
        if let Err(e) = r {
            a.error(&e);
        }
    }));
    page.upcast()
}
