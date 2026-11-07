//! Modes tab: custom modes (create, duplicate, edit, import, export, delete).
use crate::i18n::t;
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::launch::LaunchOptions;
use csls_core::modes::{self, CfgDoc, Localized, Mode, ModeMeta};
use csls_core::paths;
use csls_core::server::Server;
use gtk::{gio, glib};
use std::rc::Rc;

const PRIMARIES: &[&str] = &[
    "",
    "weapon_ak47",
    "weapon_m4a1",
    "weapon_m4a1_silencer",
    "weapon_awp",
    "weapon_ssg08",
    "weapon_scar20",
    "weapon_g3sg1",
    "weapon_sg556",
    "weapon_aug",
    "weapon_famas",
    "weapon_galilar",
    "weapon_nova",
    "weapon_xm1014",
    "weapon_mag7",
    "weapon_sawedoff",
    "weapon_negev",
    "weapon_m249",
    "weapon_p90",
    "weapon_mp9",
    "weapon_mac10",
    "weapon_mp7",
    "weapon_mp5sd",
    "weapon_ump45",
    "weapon_bizon",
];
const PISTOLS: &[&str] = &[
    "",
    "weapon_deagle",
    "weapon_revolver",
    "weapon_glock",
    "weapon_usp_silencer",
    "weapon_hkp2000",
    "weapon_p250",
    "weapon_fiveseven",
    "weapon_tec9",
    "weapon_cz75a",
    "weapon_elite",
];
const ITEMS: &[&str] = &[
    "",
    "weapon_hegrenade",
    "weapon_flashbang",
    "weapon_smokegrenade",
    "weapon_molotov",
    "weapon_incgrenade",
    "weapon_decoy",
    "weapon_taser",
    "weapon_healthshot",
    "item_defuser",
    "item_assaultsuit",
];
/// Valve base: (text, game_type, game_mode)
const BASES: &[(&str, Option<(u8, u8)>)] =
    &[("From the match type", None), ("Deathmatch", Some((1, 2))), ("Arms Race", Some((1, 0))), ("Demolition", Some((1, 1)))];

/// What the editor handles with controls; the rest goes to the extra variables box.
fn known(k: &str) -> bool {
    const K: &[&str] = &[
        "sv_gravity",
        "mp_respawn_on_death_t",
        "mp_respawn_on_death_ct",
        "mp_damage_headshot_only",
        "sv_infinite_ammo",
        "mp_buy_anywhere",
        "sv_airaccelerate",
        "modo_vampiro",
        "modo_unabala",
        "modo_aleatorio",
        "modo_jefe",
        "modo_anarquia",
        "modo_cheats",
        "modo_zombies",
    ];
    K.contains(&k)
        || ["modo_t_", "modo_ct_"]
            .iter()
            .any(|p| k.strip_prefix(p).is_some_and(|r| ["quitar", "arma", "pistola", "objeto", "vida", "velocidad", "color"].contains(&r)))
}

fn combo(title: &str, list: &[&str], value: &str) -> adw::ComboRow {
    let mut items: Vec<String> =
        list.iter().map(|s| if s.is_empty() { "—".to_string() } else { s.trim_start_matches("weapon_").to_string() }).collect();
    let mut sel = list.iter().position(|x| *x == value);
    if sel.is_none() && !value.is_empty() {
        items.push(value.to_string());
        sel = Some(items.len() - 1);
    }
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    adw::ComboRow::builder().title(title).model(&gtk::StringList::new(&refs)).selected(sel.unwrap_or(0) as u32).build()
}

fn combo_value(c: &adw::ComboRow, list: &[&str]) -> String {
    let i = c.selected() as usize;
    match list.get(i) {
        Some(v) => v.to_string(),
        // value that was not in the list (added at the end)
        None => c.selected_item().and_downcast::<gtk::StringObject>().map(|s| s.string().to_string()).unwrap_or_default(),
    }
}

fn spin(title: &str, min: f64, max: f64, step: f64, digits: u32, v: f64) -> adw::SpinRow {
    let s = adw::SpinRow::with_range(min, max, step);
    s.set_title(title);
    s.set_digits(digits);
    s.set_value(v);
    s
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

struct Team {
    quitar: adw::SwitchRow,
    arma: adw::ComboRow,
    pistola: adw::ComboRow,
    objeto: adw::ComboRow,
    vida: adw::SpinRow,
    vel: adw::SpinRow,
    color: adw::EntryRow,
}

fn team_rows(g: &adw::PreferencesGroup, d: &CfgDoc, p: &str) -> Team {
    let get = |k: &str| d.get(&format!("modo_{p}_{k}")).unwrap_or("").to_string();
    let t = Team {
        quitar: adw::SwitchRow::builder().title(t("Strip weapons on spawn")).active(get("quitar") == "1").build(),
        arma: combo(&t("Primary weapon"), PRIMARIES, &get("arma")),
        pistola: combo(&t("Pistol"), PISTOLS, &get("pistola")),
        objeto: combo(&t("Extra item"), ITEMS, &get("objeto")),
        vida: spin(&t("Health (0 = normal)"), 0.0, 10000.0, 25.0, 0, get("vida").parse().unwrap_or(0.0)),
        vel: spin(&t("Speed"), 0.1, 5.0, 0.1, 2, get("velocidad").parse().unwrap_or(1.0)),
        color: adw::EntryRow::builder().title(t("Color R,G,B (empty = normal)")).text(get("color")).build(),
    };
    for w in [
        t.quitar.upcast_ref::<gtk::Widget>(),
        t.arma.upcast_ref(),
        t.pistola.upcast_ref(),
        t.objeto.upcast_ref(),
        t.vida.upcast_ref(),
        t.vel.upcast_ref(),
        t.color.upcast_ref(),
    ] {
        g.add(w);
    }
    t
}

fn write_team(d: &mut CfgDoc, p: &str, t: &Team) {
    let mut put = |k: &str, v: String, def: &str| {
        if v != def {
            d.set(&format!("modo_{p}_{k}"), &v);
        }
    };
    put("quitar", if t.quitar.is_active() { "1".into() } else { "0".into() }, "0");
    put("arma", combo_value(&t.arma, PRIMARIES), "");
    put("pistola", combo_value(&t.pistola, PISTOLS), "");
    put("objeto", combo_value(&t.objeto, ITEMS), "");
    put("vida", fmt(t.vida.value()), "0");
    put("velocidad", fmt(t.vel.value()), "1");
    put("color", t.color.text().trim().to_string(), "");
}

/// Opens the editor. `from`: mode to edit (if the user's) or duplicate (if official).
fn editor(a: &AppRc, from: Option<&Mode>, reload: Rc<dyn Fn()>) {
    let lang = csls_core::lang();
    let editing = from.is_some_and(|m| m.custom);
    let doc = from.and_then(|m| std::fs::read_to_string(&m.cfg).ok()).map(|t| CfgDoc::parse(&t)).unwrap_or_default();
    let meta = from.map(|m| m.meta.clone()).unwrap_or_default();
    let id0 = match from {
        Some(m) if m.custom => m.id.clone(),
        Some(m) => format!("{}_2", m.id),
        None => String::new(),
    };

    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    g.set_title(&t("General"));
    let id = adw::EntryRow::builder().title(t("Id (lowercase, numbers and _)")).text(&id0).sensitive(!editing).build();
    let name = adw::EntryRow::builder().title(t("Name")).text(from.map(|m| m.name(lang)).unwrap_or_default()).build();
    let desc = adw::EntryRow::builder().title(t("Description")).text(meta.description.get(lang)).build();
    let base_names: Vec<String> = BASES.iter().map(|(n, _)| t(n)).collect();
    let base_refs: Vec<&str> = base_names.iter().map(String::as_str).collect();
    let cur = match (meta.game_type, meta.game_mode) {
        (Some(t), Some(g)) => BASES.iter().position(|(_, b)| *b == Some((t, g))).unwrap_or(0),
        _ => 0,
    };
    let base = adw::ComboRow::builder().title(t("Valve game type")).model(&gtk::StringList::new(&base_refs)).selected(cur as u32).build();
    let prefixes = adw::EntryRow::builder().title(t("Allowed map prefixes (empty = any)")).text(meta.map_prefixes.join(" ")).build();
    for w in [id.upcast_ref::<gtk::Widget>(), name.upcast_ref(), desc.upcast_ref(), base.upcast_ref(), prefixes.upcast_ref()] {
        g.add(w);
    }
    page.add(&g);

    let gt = adw::PreferencesGroup::new();
    gt.set_title(&t("Terrorists"));
    let tt = team_rows(&gt, &doc, "t");
    page.add(&gt);
    let gc = adw::PreferencesGroup::new();
    gc.set_title(&t("Counter-Terrorists"));
    let ct = team_rows(&gc, &doc, "ct");
    page.add(&gc);

    let gr = adw::PreferencesGroup::new();
    gr.set_title(&t("Rules"));
    let num = |k: &str, def: f64| doc.get(k).and_then(|v| v.parse().ok()).unwrap_or(def);
    let on = |k: &str| doc.get(k).is_some_and(|v| v != "0" && !v.is_empty());
    let gravity = spin(&t("Gravity (800 = normal)"), 50.0, 1600.0, 50.0, 0, num("sv_gravity", 800.0));
    let respawn =
        adw::SwitchRow::builder().title(t("Respawn on death")).active(on("mp_respawn_on_death_t") || on("mp_respawn_on_death_ct")).build();
    let headshot = adw::SwitchRow::builder().title(t("Headshots only")).active(on("mp_damage_headshot_only")).build();
    let ammo = adw::ComboRow::builder()
        .title(t("Infinite ammo"))
        .model(&gtk::StringList::new(&[&t("No"), &t("Yes"), &t("Yes, without reloading")]))
        .selected(num("sv_infinite_ammo", 0.0) as u32)
        .build();
    let buy = adw::SwitchRow::builder().title(t("Buy anywhere")).active(on("mp_buy_anywhere")).build();
    let air = spin(&t("Air acceleration (12 = normal)"), 0.0, 1000.0, 1.0, 0, num("sv_airaccelerate", 12.0));
    for w in [
        gravity.upcast_ref::<gtk::Widget>(),
        respawn.upcast_ref(),
        headshot.upcast_ref(),
        ammo.upcast_ref(),
        buy.upcast_ref(),
        air.upcast_ref(),
    ] {
        gr.add(w);
    }
    page.add(&gr);

    let ge = adw::PreferencesGroup::new();
    ge.set_title(&t("Special rules"));
    let vampiro = spin(&t("Vampire: fraction of damage that heals (0 = off)"), 0.0, 2.0, 0.1, 2, num("modo_vampiro", 0.0));
    let unabala = adw::SwitchRow::builder().title(t("One in the Chamber")).active(on("modo_unabala")).build();
    let aleatorio = adw::SwitchRow::builder().title(t("Random weapon each round")).active(on("modo_aleatorio")).build();
    let jefe = spin(&t("Boss bot health (0 = no boss)"), 0.0, 20000.0, 500.0, 0, num("modo_jefe", 0.0));
    let anarquia = adw::SwitchRow::builder()
        .title(t("Anarchy"))
        .subtitle(t("Nobody is kicked or banned automatically"))
        .active(on("modo_anarquia"))
        .build();
    let cheats = adw::SwitchRow::builder().title(t("Random cheats power-ups")).active(on("modo_cheats")).build();
    let zombies = adw::SwitchRow::builder().title(t("Zombie waves (T are zombies)")).active(on("modo_zombies")).build();
    for w in [
        vampiro.upcast_ref::<gtk::Widget>(),
        unabala.upcast_ref(),
        aleatorio.upcast_ref(),
        jefe.upcast_ref(),
        anarquia.upcast_ref(),
        cheats.upcast_ref(),
        zombies.upcast_ref(),
    ] {
        ge.add(w);
    }
    page.add(&ge);

    let gx = adw::PreferencesGroup::new();
    gx.set_title(&t("Extra variables"));
    gx.set_description(Some(&t("One per line: cvar value")));
    let extra_txt: String = doc.pairs.iter().filter(|(k, _)| !known(k)).map(|(k, v)| format!("{k} {v}\n")).collect();
    let tv = gtk::TextView::builder().monospace(true).top_margin(8).bottom_margin(8).left_margin(8).right_margin(8).build();
    tv.buffer().set_text(&extra_txt);
    let frame = gtk::Frame::builder().child(&tv).height_request(140).build();
    gx.add(&frame);
    page.add(&gx);

    let save = button(&t("Save"), &["suggested-action"]);
    let try_btn = button(&t("Save and try"), &[]);
    try_btn.set_visible(a.live.borrow().rcon_ok);
    let header = adw::HeaderBar::new();
    header.pack_end(&save);
    header.pack_end(&try_btn);
    let tvw = adw::ToolbarView::new();
    tvw.add_top_bar(&header);
    tvw.set_content(Some(&page));
    let title = if editing { t("Edit mode") } else { t("New mode") };
    let dialog = adw::Dialog::builder().title(&title).content_width(620).content_height(760).child(&tvw).build();

    let build = Rc::new(move || -> anyhow::Result<(String, ModeMeta, String)> {
        let idv = id.text().trim().to_string();
        let mut d = CfgDoc { comment: format!("{idv}: {}", desc.text()), pairs: vec![] };
        write_team(&mut d, "t", &tt);
        write_team(&mut d, "ct", &ct);
        let mut put = |k: &str, v: String, def: &str| {
            if v != def {
                d.set(k, &v);
            }
        };
        put("sv_gravity", fmt(gravity.value()), "800");
        let r = if respawn.is_active() { "1" } else { "0" };
        put("mp_respawn_on_death_t", r.into(), "0");
        put("mp_respawn_on_death_ct", r.into(), "0");
        put("mp_damage_headshot_only", (headshot.is_active() as u8).to_string(), "0");
        put("sv_infinite_ammo", ammo.selected().min(2).to_string(), "0");
        put("mp_buy_anywhere", (buy.is_active() as u8).to_string(), "0");
        put("sv_airaccelerate", fmt(air.value()), "12");
        put("modo_vampiro", fmt(vampiro.value()), "0");
        put("modo_unabala", (unabala.is_active() as u8).to_string(), "0");
        put("modo_aleatorio", (aleatorio.is_active() as u8).to_string(), "0");
        put("modo_jefe", fmt(jefe.value()), "0");
        put("modo_anarquia", (anarquia.is_active() as u8).to_string(), "0");
        put("modo_cheats", (cheats.is_active() as u8).to_string(), "0");
        put("modo_zombies", (zombies.is_active() as u8).to_string(), "0");
        let b = tv.buffer();
        for l in b.text(&b.start_iter(), &b.end_iter(), false).lines() {
            let mut it = l.trim().splitn(2, char::is_whitespace);
            if let Some(k) = it.next().filter(|k| !k.is_empty() && !k.starts_with("//")) {
                d.set(k, it.next().unwrap_or("").trim().trim_matches('"'));
            }
        }
        d.validate()?;
        let mut meta = meta.clone();
        let lang = csls_core::lang();
        let (n, ds) = (name.text().to_string(), desc.text().to_string());
        if lang == "es" {
            meta.name.es = n;
            meta.description.es = ds;
        } else {
            meta.name.en = n;
            meta.description.en = ds;
        }
        if meta.name.en.is_empty() && meta.name.es.is_empty() {
            meta.name = Localized::same(&idv);
        }
        meta.category = "custom".into();
        (meta.game_type, meta.game_mode) = match BASES[base.selected().min(3) as usize].1 {
            Some((t, g)) => (Some(t), Some(g)),
            None => (None, None),
        };
        meta.map_prefixes = prefixes.text().split_whitespace().map(str::to_string).collect();
        if meta.map_prefixes.is_empty() {
            meta.default_map = None;
        }
        meta.exclusive_maps = false;
        Ok((idv, meta, d.render()))
    });

    let do_save = Rc::new(glib::clone!(
        #[strong]
        a,
        #[strong]
        reload,
        #[weak]
        dialog,
        #[upgrade_or]
        false,
        move |try_it: bool| -> bool {
            let r = build().and_then(|(id, meta, cfg)| {
                if !editing && a.modes.borrow().0.contains_key(&id) {
                    anyhow::bail!("{} '{id}'", t("A mode already exists with id"));
                }
                modes::save_user_mode(&paths::user_modes_dir(), &id, &meta, &cfg)?;
                Ok(id)
            });
            match r {
                Ok(id) => {
                    reload();
                    dialog.close();
                    a.toast(&t("Saved"));
                    if try_it {
                        let cfg = a.cfg.borrow().clone();
                        let modes = a.modes.borrow().clone();
                        let mut o = LaunchOptions::from_config(&cfg);
                        o.mode = id;
                        bg(
                            move || Server::new(&cfg).change(&modes, &o),
                            glib::clone!(
                                #[strong]
                                a,
                                move |r| {
                                    match r {
                                        Ok(m) => a.toast(&format!("{} · {m}", t("Mode changed"))),
                                        Err(e) => a.error(&e),
                                    }
                                    a.refresh();
                                }
                            ),
                        );
                    }
                    true
                }
                Err(e) => {
                    a.error(&e);
                    false
                }
            }
        }
    ));
    save.connect_clicked(glib::clone!(
        #[strong]
        do_save,
        move |_| {
            do_save(false);
        }
    ));
    try_btn.connect_clicked(move |_| {
        do_save(true);
    });
    dialog.present(Some(&a.win));
}

pub fn page(a: &AppRc) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let mine = adw::PreferencesGroup::new();
    mine.set_title(&t("Your modes"));
    mine.set_description(Some(&t("They are saved in ~/.config/csgo-legacy-server/modes and deployed automatically")));
    let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let new = button(&t("New mode"), &["suggested-action"]);
    let import = button(&t("Import"), &[]);
    hb.append(&import);
    hb.append(&new);
    mine.set_header_suffix(Some(&hb));
    let mine_list = gtk::ListBox::new();
    mine_list.add_css_class("boxed-list");
    mine_list.set_selection_mode(gtk::SelectionMode::None);
    mine.add(&mine_list);
    page.add(&mine);
    let off = adw::PreferencesGroup::new();
    off.set_title(&t("Built-in modes"));
    off.set_description(Some(&t("Duplicate one to make your own version")));
    let off_list = gtk::ListBox::new();
    off_list.add_css_class("boxed-list");
    off_list.set_selection_mode(gtk::SelectionMode::None);
    off.add(&off_list);
    page.add(&off);

    let reload: crate::LateFn = Rc::default();
    let rl = reload.clone();
    let fill: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong]
        a,
        #[weak]
        mine_list,
        #[weak]
        off_list,
        move || {
            a.reload_modes();
            mine_list.remove_all();
            off_list.remove_all();
            let lang = csls_core::lang();
            let me = rl.borrow().clone().unwrap();
            let modes: Vec<Mode> = a.modes.borrow().0.values().cloned().collect();
            if !modes.iter().any(|m| m.custom) {
                mine_list.append(&adw::ActionRow::builder().title(t("No custom modes yet")).build());
            }
            for m in modes {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&m.name(lang)))
                    .subtitle(glib::markup_escape_text(&format!("{} · {}", m.id, m.meta.description.get(lang))))
                    .build();
                let mk = |label: &str, class: &[&str]| {
                    let b = button(label, class);
                    b.add_css_class("flat");
                    b.set_valign(gtk::Align::Center);
                    row.add_suffix(&b);
                    b
                };
                if m.custom {
                    let mm = m.clone();
                    let r2 = me.clone();
                    mk(&t("Edit"), &[]).connect_clicked(glib::clone!(
                        #[strong]
                        a,
                        move |_| editor(&a, Some(&mm), r2.clone())
                    ));
                    let mm = m.clone();
                    mk(&t("Export"), &[]).connect_clicked(glib::clone!(
                        #[strong]
                        a,
                        move |_| export(&a, &mm)
                    ));
                    let id = m.id.clone();
                    let r2 = me.clone();
                    mk(&t("Delete"), &["error"]).connect_clicked(glib::clone!(
                        #[strong]
                        a,
                        move |_| {
                            let d = adw::AlertDialog::new(Some(&format!("{} {id}?", t("Delete"))), None);
                            d.add_response("cancel", &t("Cancel"));
                            d.add_response("del", &t("Delete"));
                            d.set_response_appearance("del", adw::ResponseAppearance::Destructive);
                            let (id, r2) = (id.clone(), r2.clone());
                            d.connect_response(
                                None,
                                glib::clone!(
                                    #[strong]
                                    a,
                                    move |_, r| {
                                        if r == "del" {
                                            match modes::delete_user_mode(&paths::user_modes_dir(), &id) {
                                                Ok(()) => r2(),
                                                Err(e) => a.error(&e),
                                            }
                                        }
                                    }
                                ),
                            );
                            d.present(Some(&a.win));
                        }
                    ));
                    mine_list.append(&row);
                } else {
                    let mm = m.clone();
                    let r2 = me.clone();
                    mk(&t("Duplicate"), &[]).connect_clicked(glib::clone!(
                        #[strong]
                        a,
                        move |_| editor(&a, Some(&mm), r2.clone())
                    ));
                    off_list.append(&row);
                }
            }
        }
    ));
    *reload.borrow_mut() = Some(fill.clone());
    fill();
    new.connect_clicked(glib::clone!(
        #[strong]
        a,
        #[strong]
        fill,
        move |_| editor(&a, None, fill.clone())
    ));
    import.connect_clicked(glib::clone!(
        #[strong]
        a,
        #[strong]
        fill,
        move |_| {
            let f = gtk::FileFilter::new();
            f.add_pattern("*.toml");
            f.set_name(Some("TOML"));
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&f);
            let fd = gtk::FileDialog::builder().title(t("Import")).filters(&filters).modal(true).build();
            fd.open(
                Some(&a.win),
                None::<&gio::Cancellable>,
                glib::clone!(
                    #[strong]
                    a,
                    #[strong]
                    fill,
                    move |r| {
                        let Ok(file) = r else { return };
                        let res = file
                            .path()
                            .ok_or_else(|| anyhow::anyhow!("?"))
                            .and_then(|p| Ok(std::fs::read_to_string(p)?))
                            .and_then(|txt| modes::import_mode(&paths::user_modes_dir(), &txt));
                        match res {
                            Ok(id) => {
                                fill();
                                a.toast(&format!("{} {id}", t("Imported mode")));
                            }
                            Err(e) => a.error(&e),
                        }
                    }
                ),
            );
        }
    ));
    // changes in the folder (modes copied by hand) show up when returning to the tab
    a.on_modes_changed(move || fill());
    page.upcast()
}

fn export(a: &AppRc, m: &Mode) {
    let txt = match modes::export_mode(m) {
        Ok(t) => t,
        Err(e) => return a.error(&e),
    };
    let fd = gtk::FileDialog::builder().title(t("Export")).initial_name(format!("{}.toml", m.id)).modal(true).build();
    fd.save(
        Some(&a.win),
        None::<&gio::Cancellable>,
        glib::clone!(
            #[strong]
            a,
            move |r| {
                let Ok(f) = r else { return };
                if let Some(p) = f.path() {
                    match std::fs::write(&p, &txt) {
                        Ok(()) => a.toast(&t("Saved")),
                        Err(e) => a.error(&e.into()),
                    }
                }
            }
        ),
    );
}

/// Debug (screenshots): opens the editor duplicating mode `id`.
pub fn debug_open_editor(a: &AppRc, id: &str) {
    let m = a.modes.borrow().0.get(id).cloned();
    editor(a, m.as_ref(), Rc::new(|| {}));
}
