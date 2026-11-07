//! Settings tab: config.toml, maintenance and diagnostics.
use crate::i18n::{t, t_msg};
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::config::{Language, Network};
use csls_core::server;
use csls_core::{backup, deploy, doctor};
use gtk::glib;

pub fn page(a: &AppRc) -> gtk::Widget {
    let c = a.cfg.borrow().server.clone();
    let page = adw::PreferencesPage::new();

    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Server"));
    let language = adw::ComboRow::builder()
        .title(t("Language"))
        .subtitle(t("App, command line and in-game messages"))
        .model(&gtk::StringList::new(&["English", "Español"]))
        .build();
    language.set_selected(u32::from(a.cfg.borrow().language == Language::Es));
    let name = adw::EntryRow::builder().title(t("Server name")).text(&c.hostname).build();
    let pass = adw::PasswordEntryRow::builder().title(t("Password (empty = none)")).text(&c.password).build();
    let nets = [Network::Lan, Network::Vpn, Network::Internet, Network::Community];
    let net_row = adw::ComboRow::builder()
        .title(t("Network"))
        .model(&gtk::StringList::new(&[
            &t("LAN only"),
            &t("Friends over VPN"),
            &t("Internet (open port)"),
            &t("Community (Steam server list)"),
        ]))
        .selected(nets.iter().position(|n| *n == c.network).unwrap_or(0) as u32)
        .build();
    let port = adw::SpinRow::with_range(1024.0, 65000.0, 1.0);
    port.set_title(&t("Port"));
    port.set_value(c.port as f64);
    let maxp = adw::SpinRow::with_range(2.0, 64.0, 1.0);
    maxp.set_title(&t("Max players"));
    maxp.set_value(c.max_players as f64);
    let tick = adw::ComboRow::builder()
        .title(t("Tickrate"))
        .model(&gtk::StringList::new(&["64", "128"]))
        .selected(if c.tickrate >= 128 { 1 } else { 0 })
        .build();
    let gslt = adw::PasswordEntryRow::builder().title(t("GSLT token (community)")).text(&c.gslt).build();
    let wkey = adw::PasswordEntryRow::builder().title(t("Workshop Web API key")).text(&c.workshop_authkey).build();
    let upnp = adw::SwitchRow::builder()
        .title(t("Open the port with UPnP"))
        .subtitle(t("Asks the router to open one UDP port while the server runs. Internet and community profiles; needs upnpc (miniupnpc)"))
        .active(c.upnp)
        .build();
    net_row.bind_property("selected", &upnp, "visible").transform_to(|_, s: u32| Some(s >= 2)).sync_create().build();
    let skins =
        adw::SwitchRow::builder().title(t("Skins plugin")).subtitle(t("Can cost the GSLT on public servers")).active(c.skins).build();
    let folder = adw::ActionRow::builder().title(t("Server folder")).subtitle(c.install_dir.display().to_string()).build();
    folder.add_css_class("property");
    for r in [
        language.upcast_ref::<gtk::Widget>(),
        name.upcast_ref(),
        pass.upcast_ref(),
        net_row.upcast_ref(),
        upnp.upcast_ref(),
        port.upcast_ref(),
        maxp.upcast_ref(),
        tick.upcast_ref(),
        gslt.upcast_ref(),
        wkey.upcast_ref(),
        skins.upcast_ref(),
        folder.upcast_ref(),
    ] {
        g.add(r);
    }
    let save = button(&t("Save"), &["suggested-action"]);
    save.set_valign(gtk::Align::Center);
    g.set_header_suffix(Some(&save));
    page.add(&g);

    save.connect_clicked(glib::clone!(
        #[strong]
        a,
        move |_| {
            let mut cfg = a.cfg.borrow().clone();
            let old = cfg.server.clone();
            let old_language = cfg.language;
            cfg.language = if language.selected() == 1 { Language::Es } else { Language::En };
            let new_language = cfg.language != old_language;
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
            let restart = (s.network, s.port, s.tickrate, s.max_players, &s.gslt)
                != (old.network, old.port, old.tickrate, old.max_players, &old.gslt);
            match cfg.save() {
                Ok(()) => {
                    *a.cfg.borrow_mut() = cfg;
                    if new_language {
                        a.toast(&t("Restart the app to apply the language"));
                        return;
                    }
                    a.toast(&if restart && a.live.borrow().running {
                        t("Restart the server to apply network changes")
                    } else {
                        t("Saved")
                    });
                }
                Err(e) => a.error(&e),
            }
        }
    ));

    let m = adw::PreferencesGroup::new();
    m.set_title(&t("Maintenance"));
    let redeploy = adw::ButtonRow::builder().title(t("Recompile plugins")).build();
    let update = adw::ButtonRow::builder().title(t("Update server and SourceMod")).build();
    m.add(&redeploy);
    m.add(&update);
    page.add(&m);
    page.add(&backups_group(a));
    redeploy.connect_activated(glib::clone!(
        #[strong]
        a,
        move |_| {
            let cfg = a.cfg.borrow().clone();
            bg(
                move || deploy::deploy(&cfg, true, true),
                glib::clone!(
                    #[strong]
                    a,
                    move |r: anyhow::Result<deploy::Report>| match r {
                        Ok(r) => match r.error_text() {
                            Some(e) => a.error(&anyhow::anyhow!(e)),
                            None => a.toast(&format!("{}: {}", t("Recompile plugins"), r.compiled.join(", "))),
                        },
                        Err(e) => a.error(&e),
                    }
                ),
            );
        }
    ));
    update.connect_activated(glib::clone!(
        #[strong]
        a,
        move |_| {
            if a.live.borrow().running {
                a.error(&anyhow::anyhow!(t("Stop the server?")));
                return;
            }
            match server::spawn_install(true) {
                Ok(()) => a.toast(&t("Installing…")),
                Err(e) => a.error(&e),
            }
        }
    ));

    let d = adw::PreferencesGroup::new();
    d.set_title(&t("Diagnosis"));
    for c in doctor::run(&a.cfg.borrow()) {
        let row = adw::ActionRow::builder().title(t(c.name)).subtitle(t_msg(&c.detail)).subtitle_selectable(true).build();
        row.add_css_class("property");
        if c.status == doctor::Status::Problem {
            row.add_css_class("warning");
        }
        d.add(&row);
    }
    page.add(&d);
    page.upcast()
}

/// Backups: make one, or restore one after seeing how many files it changes.
fn backups_group(a: &AppRc) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Backups"));
    g.set_description(Some(&t("Your config, modes, plugins, presets and skins. They hold your passwords: do not share them")));
    let create = adw::ButtonRow::builder().title(t("Create backup")).build();
    g.add(&create);
    create.connect_activated(glib::clone!(
        #[strong]
        a,
        move |_| {
            let cfg = a.cfg.borrow().clone();
            bg(
                move || backup::create(&cfg),
                glib::clone!(
                    #[strong]
                    a,
                    move |r: anyhow::Result<std::path::PathBuf>| match r {
                        Ok(p) => a.toast(&format!("{}: {}", t("Backup saved"), p.file_name().unwrap_or_default().to_string_lossy())),
                        Err(e) => a.error(&e),
                    }
                ),
            );
        }
    ));
    let restore = adw::ButtonRow::builder().title(t("Restore the latest backup")).build();
    g.add(&restore);
    restore.connect_activated(glib::clone!(
        #[strong]
        a,
        move |_| {
            let Some(file) = backup::list().ok().and_then(|l| l.into_iter().next()) else {
                a.toast(&t("There are no backups"));
                return;
            };
            let cfg = a.cfg.borrow().clone();
            let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let f = file.clone();
            bg(
                move || backup::preview(&cfg, &f),
                glib::clone!(
                    #[strong]
                    a,
                    move |r: anyhow::Result<Vec<backup::Change>>| match r {
                        Ok(c) if c.is_empty() => a.toast(&t("Nothing to restore: the files are the same")),
                        Ok(c) => {
                            let d = adw::AlertDialog::new(
                                Some(&t("Restore the latest backup")),
                                Some(&format!("{name}\n{}: {}", t("Files that will change"), c.len())),
                            );
                            d.add_response("cancel", &t("Cancel"));
                            d.add_response("ok", &t("Restore"));
                            d.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
                            d.connect_response(
                                None,
                                glib::clone!(
                                    #[strong]
                                    a,
                                    #[strong]
                                    file,
                                    move |_, resp| {
                                        if resp != "ok" {
                                            return;
                                        }
                                        let cfg = a.cfg.borrow().clone();
                                        let f = file.clone();
                                        bg(
                                            move || backup::restore(&cfg, &f),
                                            glib::clone!(
                                                #[strong]
                                                a,
                                                move |r: anyhow::Result<Vec<backup::Change>>| match r {
                                                    Ok(_) => a.toast(&t("Backup restored; restart the app")),
                                                    Err(e) => a.error(&e),
                                                }
                                            ),
                                        );
                                    }
                                ),
                            );
                            d.present(Some(&a.win));
                        }
                        Err(e) => a.error(&e),
                    }
                ),
            );
        }
    ));
    g
}
