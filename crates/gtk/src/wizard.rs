//! Asistente: modo -> mapa -> opciones -> servidor (solo al iniciar) -> iniciar/aplicar.
use crate::i18n::t;
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::config::{Kind, Network};
use csls_core::launch::{installed_maps, parse_cvar, LaunchOptions, Site};
use csls_core::modes::Mode;
use csls_core::server::Server;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const CATEGORIES: &[(&str, &str)] = &[
    ("classic", "Classic"),
    ("weapons", "Weapons"),
    ("deathmatch", "Deathmatch"),
    ("fun", "Fun"),
    ("objective", "Objective"),
    ("pve", "PvE"),
    ("custom", "Custom"),
];

struct W {
    a: AppRc,
    live: bool,
    nav: adw::NavigationView,
    dialog: adw::Dialog,
    opts: RefCell<LaunchOptions>,
}

type Wr = Rc<W>;

/// Inicia (o aplica en caliente) un preset guardado sin pasar por el asistente.
pub fn run_preset(a: &AppRc, opts: LaunchOptions, live: bool) {
    let nav = adw::NavigationView::new();
    let dialog = adw::Dialog::new();
    let w = Rc::new(W { a: a.clone(), live, nav, dialog, opts: RefCell::new(opts) });
    apply(&w);
}

pub fn open(a: &AppRc, live: bool) {
    a.reload_modes();
    let nav = adw::NavigationView::new();
    let title = if live { t("Change mode or map") } else { t("Start server") };
    let dialog = adw::Dialog::builder().title(&title).content_width(560).content_height(640).child(&nav).build();
    let w = Rc::new(W { a: a.clone(), live, nav, dialog, opts: RefCell::new(LaunchOptions::from_config(&a.cfg.borrow())) });
    w.nav.push(&modes_page(&w));
    w.dialog.present(Some(&a.win));
}

fn wrap(title: &str, content: &impl IsA<gtk::Widget>, bottom: Option<&gtk::Button>) -> adw::NavigationPage {
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(content));
    if let Some(b) = bottom {
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bx.set_halign(gtk::Align::Center);
        bx.set_margin_top(8);
        bx.set_margin_bottom(12);
        bx.append(b);
        tv.add_bottom_bar(&bx);
    }
    adw::NavigationPage::new(&tv, title)
}

fn modes_page(w: &Wr) -> adw::NavigationPage {
    let page = adw::PreferencesPage::new();
    let lang = csls_core::lang();
    let modes = w.a.modes.borrow();
    let mut cats: Vec<(String, String)> = CATEGORIES.iter().map(|(k, l)| (k.to_string(), t(l))).collect();
    for m in modes.0.values() {
        if !cats.iter().any(|(k, _)| *k == m.meta.category) {
            cats.push((m.meta.category.clone(), m.meta.category.clone()));
        }
    }
    for (cat, label) in cats {
        let list: Vec<&Mode> = modes.0.values().filter(|m| m.meta.category == cat).collect();
        if list.is_empty() {
            continue;
        }
        let g = adw::PreferencesGroup::new();
        g.set_title(&label);
        for m in list {
            let row = adw::ActionRow::builder().title(m.name(lang)).subtitle(m.meta.description.get(lang)).activatable(true).build();
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let id = m.id.clone();
            row.connect_activated(glib::clone!(#[strong] w, move |_| {
                w.opts.borrow_mut().mode = id.clone();
                w.nav.push(&map_page(&w));
            }));
            g.add(&row);
        }
        page.add(&g);
    }
    wrap(&t("Choose a mode"), &page, None)
}

fn map_page(w: &Wr) -> adw::NavigationPage {
    let modes = w.a.modes.borrow();
    let mode = modes.get(&w.opts.borrow().mode).unwrap().clone();
    let excl = modes.exclusive_prefixes();
    let maps_dir = Server::new(&w.a.cfg.borrow()).maps_dir();
    let maps: Vec<String> = installed_maps(&maps_dir)
        .into_iter()
        .filter(|m| if mode.meta.map_prefixes.is_empty() { !excl.iter().any(|p| m.starts_with(p.as_str())) } else { mode.accepts_map(m) })
        .collect();

    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    let def = adw::ActionRow::builder().title(t("Default map for this mode")).subtitle(t("Keep the current one or use the mode's default")).activatable(true).build();
    def.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    def.connect_activated(glib::clone!(#[strong] w, move |_| {
        let mut o = w.opts.borrow_mut();
        o.map = None;
        o.workshop = None;
        drop(o);
        w.nav.push(&options_page(&w));
    }));
    g.add(&def);
    let ws = adw::EntryRow::builder().title(t("Workshop map ID")).show_apply_button(true).build();
    ws.set_input_purpose(gtk::InputPurpose::Digits);
    ws.connect_apply(glib::clone!(#[strong] w, move |e| {
        match e.text().trim().parse::<u64>() {
            Ok(id) => {
                let mut o = w.opts.borrow_mut();
                o.workshop = Some(id);
                o.map = None;
                drop(o);
                w.nav.push(&options_page(&w));
            }
            Err(_) => e.add_css_class("error"),
        }
    }));
    g.add(&ws);
    page.add(&g);

    let search = gtk::SearchEntry::builder().placeholder_text(t("Search maps")).build();
    let gs = adw::PreferencesGroup::new();
    gs.add(&search);
    page.add(&gs);

    // por prefijo: de_, cs_, ar_, gd_, workshop/…
    let mut groups: Vec<(String, adw::PreferencesGroup)> = Vec::new();
    let rows: Rc<RefCell<Vec<(String, adw::ActionRow)>>> = Rc::default();
    for m in maps {
        let pre = if m.starts_with("workshop/") || m.starts_with("csls/") { "Workshop".to_string() } else { m.split('_').next().unwrap_or("").to_string() + "_" };
        let grp = match groups.iter().find(|(p, _)| *p == pre) {
            Some((_, g)) => g.clone(),
            None => {
                let g = adw::PreferencesGroup::new();
                g.set_title(&pre);
                page.add(&g);
                groups.push((pre, g.clone()));
                g
            }
        };
        let row = adw::ActionRow::builder().title(&m).activatable(true).build();
        let mm = m.clone();
        row.connect_activated(glib::clone!(#[strong] w, move |_| {
            let mut o = w.opts.borrow_mut();
            o.map = Some(mm.clone());
            o.workshop = None;
            drop(o);
            w.nav.push(&options_page(&w));
        }));
        grp.add(&row);
        rows.borrow_mut().push((m, row));
    }
    search.connect_search_changed(glib::clone!(#[strong] rows, move |s| {
        let q = s.text().to_lowercase();
        for (m, r) in rows.borrow().iter() {
            r.set_visible(m.contains(&q));
        }
    }));
    wrap(&t("Choose a map"), &page, None)
}

fn options_page(w: &Wr) -> adw::NavigationPage {
    let modes = w.a.modes.borrow();
    let o = w.opts.borrow().clone();
    let mode = modes.get(&o.mode).unwrap().clone();
    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&mode.name(csls_core::lang()));

    let humans = adw::SpinRow::with_range(1.0, 64.0, 1.0);
    humans.set_title(&t("Humans needed to start"));
    humans.set_value(o.humans as f64);
    let bots_on = adw::SwitchRow::builder().title(t("Bots")).subtitle(t("Default")).active(false).build();
    let bots = adw::SpinRow::with_range(0.0, 63.0, 1.0);
    bots.set_title(&t("Number of bots"));
    bots.set_value(5.0);
    bots_on.bind_property("active", &bots, "sensitive").sync_create().build();
    let diff = adw::ComboRow::builder().title(t("Bot difficulty")).model(&gtk::StringList::new(&[&t("Easy"), &t("Normal"), &t("Hard"), &t("Expert")])).selected(o.difficulty as u32).build();
    let rounds = adw::SpinRow::with_range(0.0, 99.0, 1.0);
    rounds.set_title(&t("Max rounds (0 = default)"));
    // solo los tipos que encajan con el modo elegido
    let kinds: Vec<Kind> = Kind::ALL.into_iter().filter(|k| mode.allows_kind(*k)).collect();
    let kind_names: Vec<String> = kinds.iter().map(|k| t(k.label())).collect();
    let kind = adw::ComboRow::builder().title(t("Match type")).subtitle(t("Arms Race, Guardian and Danger Zone use their own maps")).model(&gtk::StringList::new(&kind_names.iter().map(String::as_str).collect::<Vec<_>>())).build();
    kind.set_selected(kinds.iter().position(|k| *k == o.kind).unwrap_or(0) as u32);
    kind.set_visible(mode.meta.game_type.is_none());
    let site = adw::ComboRow::builder().title(t("Bomb site")).model(&gtk::StringList::new(&[&t("Random"), "A", "B"])).build();
    site.set_visible(mode.has_option("site"));
    let precision = adw::SwitchRow::builder().title(t("Infinite accuracy")).subtitle(t("No spread and no recoil for everyone")).build();
    let anarchy = adw::SwitchRow::builder().title(t("Anarchy")).subtitle(t("Nobody is kicked or banned automatically")).build();
    let extra = adw::EntryRow::builder().title(t("Extra variables")).build();
    extra.set_tooltip_text(Some(&t("cvar=value separated by spaces")));
    let preset = adw::EntryRow::builder().title(t("Save as preset (name, optional)")).build();
    for r in [humans.upcast_ref::<gtk::Widget>(), bots_on.upcast_ref(), bots.upcast_ref(), diff.upcast_ref(), rounds.upcast_ref(), kind.upcast_ref(), site.upcast_ref(), precision.upcast_ref(), anarchy.upcast_ref(), extra.upcast_ref(), preset.upcast_ref()] {
        g.add(r);
    }
    page.add(&g);

    let mode_game_type = mode.meta.game_type;
    let next = button(&if w.live { t("Apply") } else { t("Next") }, &["suggested-action", "pill"]);
    next.connect_clicked(glib::clone!(#[strong] w, move |_| {
        {
            let mut o = w.opts.borrow_mut();
            o.humans = humans.value() as u32;
            o.bots = bots_on.is_active().then(|| bots.value() as u32);
            o.difficulty = diff.selected() as u8;
            o.rounds = (rounds.value() > 0.0).then(|| rounds.value() as u32);
            o.kind = kinds.get(kind.selected() as usize).copied().unwrap_or_default();
            // el tipo se elige después del mapa: si el mapa no le sirve, se usa el suyo
            if let (Some((prefix, _)), None) = (o.kind.own_maps(), mode_game_type) {
                if o.map.as_deref().is_some_and(|m| !m.starts_with(prefix)) {
                    o.map = None;
                }
            }
            o.site = site.is_visible().then(|| [Site::Random, Site::A, Site::B][site.selected().min(2) as usize]);
            o.precision = precision.is_active();
            o.anarchy = anarchy.is_active();
            o.cvars.clear();
            for p in extra.text().split_whitespace() {
                match parse_cvar(p) {
                    Ok(c) => o.cvars.push(c),
                    Err(e) => {
                        drop(o);
                        w.a.error(&e);
                        return;
                    }
                }
            }
            let name = preset.text();
            if !name.trim().is_empty() {
                if let Err(e) = csls_core::presets::Presets::put(&name, &o) {
                    drop(o);
                    w.a.error(&e);
                    return;
                }
                w.a.toast(&t("Preset saved"));
                w.a.modes_changed();
            }
        }
        if w.live { apply(&w) } else { w.nav.push(&server_page(&w)) }
    }));
    wrap(&t("Options"), &page, Some(&next))
}

fn server_page(w: &Wr) -> adw::NavigationPage {
    let c = w.a.cfg.borrow().server.clone();
    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("Server"));
    let nets = [Network::Lan, Network::Vpn, Network::Internet, Network::Community];
    let net = adw::ComboRow::builder()
        .title(t("Network"))
        .model(&gtk::StringList::new(&[&t("LAN only"), &t("Friends over VPN"), &t("Internet (open port)"), &t("Community (Steam server list)")]))
        .selected(nets.iter().position(|n| *n == c.network).unwrap_or(0) as u32)
        .build();
    let name = adw::EntryRow::builder().title(t("Server name")).text(&c.hostname).build();
    let pass = adw::PasswordEntryRow::builder().title(t("Password (empty = none)")).text(&c.password).build();
    let maxp = adw::SpinRow::with_range(2.0, 64.0, 1.0);
    maxp.set_title(&t("Max players"));
    maxp.set_value(c.max_players as f64);
    let gslt = adw::PasswordEntryRow::builder().title(t("GSLT token (community)")).text(&c.gslt).build();
    net.bind_property("selected", &gslt, "visible").transform_to(|_, s: u32| Some(s == 3)).sync_create().build();
    for r in [net.upcast_ref::<gtk::Widget>(), name.upcast_ref(), pass.upcast_ref(), maxp.upcast_ref(), gslt.upcast_ref()] {
        g.add(r);
    }
    page.add(&g);
    let go = button(&t("Start"), &["suggested-action", "pill"]);
    go.connect_clicked(glib::clone!(#[strong] w, move |_| {
        let mut cfg = w.a.cfg.borrow().clone();
        cfg.server.network = nets[net.selected().min(3) as usize];
        cfg.server.hostname = name.text().to_string();
        cfg.server.password = pass.text().to_string();
        cfg.server.max_players = maxp.value() as u32;
        cfg.server.gslt = gslt.text().trim().to_string();
        if let Err(e) = cfg.save() {
            w.a.error(&e);
            return;
        }
        *w.a.cfg.borrow_mut() = cfg;
        apply(&w);
    }));
    wrap(&t("Server"), &page, Some(&go))
}

fn apply(w: &Wr) {
    let cfg = w.a.cfg.borrow().clone();
    let modes = w.a.modes.borrow().clone();
    let opts = w.opts.borrow().clone();
    let live = w.live;
    let mode_id = opts.mode.clone();
    w.dialog.close();
    w.a.toast(&t("Starting…"));
    let a = w.a.clone();
    bg(
        move || -> anyhow::Result<(String, Vec<String>)> {
            let srv = Server::new(&cfg);
            if live {
                return Ok((srv.change(&modes, &opts)?, Vec::new()));
            }
            let s = srv.start(&modes, &opts)?;
            srv.wait_rcon(Duration::from_secs(90))?;
            Ok((s.map, s.warnings))
        },
        move |r| {
            match r {
                Ok((map, warnings)) => {
                    a.toast(&format!("{} · {} · {map}", if live { t("Mode changed") } else { t("Server started") }, mode_id));
                    if !warnings.is_empty() {
                        a.error(&anyhow::anyhow!(warnings.join("\n")));
                    }
                }
                Err(e) => a.error(&e),
            }
            a.refresh();
        },
    );
}
