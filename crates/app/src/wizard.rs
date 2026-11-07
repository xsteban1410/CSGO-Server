//! Wizard: mode -> map -> options -> server (only when starting) -> start/apply.
use crate::i18n::t;
use crate::{bg, model, strings, AppRc, ModeItem};
use csls_core::config::{Kind, Network};
use csls_core::launch::{installed_maps, parse_cvar, LaunchOptions, Site, Teams};
use csls_core::server::Server;
use std::time::Duration;

const CATEGORIES: &[(&str, &str)] = &[
    ("competitive", "Competitive"),
    ("classic", "Classic"),
    ("weapons", "Weapons"),
    ("deathmatch", "Deathmatch"),
    ("fun", "Fun"),
    ("objective", "Objective"),
    ("pve", "PvE"),
    ("custom", "Custom"),
];
const NETS: [Network; 4] = [Network::Lan, Network::Vpn, Network::Internet, Network::Community];

#[derive(Default)]
pub struct State {
    opts: Option<LaunchOptions>,
    maps: Vec<String>,
    kinds: Vec<Kind>,
    mode_game_type: Option<u8>,
}

#[cfg(test)]
impl State {
    pub fn debug_options(&self) -> Option<LaunchOptions> {
        self.opts.clone()
    }
}

fn item(id: &str, title: &str, subtitle: &str, heading: bool) -> ModeItem {
    ModeItem { id: id.into(), title: title.into(), subtitle: subtitle.into(), heading }
}

/// Starts (or applies live) a saved preset without going through the wizard.
pub fn run_preset(a: &AppRc, opts: LaunchOptions, live: bool) {
    apply(a, opts, live);
}

/// The Start button: on a PC that chose to only join there is no server yet, so it offers the download instead.
pub fn start_or_install(a: &AppRc, live: bool) {
    if !live && !Server::new(&a.cfg.borrow()).is_installed() {
        a.s().set_setup("install".into());
        return;
    }
    open(a, live);
}

pub fn open(a: &AppRc, live: bool) {
    a.reload_modes();
    let lang = csls_core::lang();
    let modes = a.modes.borrow();
    let mut cats: Vec<(String, String)> = CATEGORIES.iter().map(|(k, l)| (k.to_string(), t(l))).collect();
    for m in modes.0.values() {
        if !cats.iter().any(|(k, _)| *k == m.meta.category) {
            cats.push((m.meta.category.clone(), m.meta.category.clone()));
        }
    }
    let mut items = Vec::new();
    for (cat, label) in cats {
        let list: Vec<_> = modes.0.values().filter(|m| m.meta.category == cat).collect();
        if list.is_empty() {
            continue;
        }
        items.push(item("", &label, "", true));
        items.extend(list.iter().map(|m| item(&m.id, &m.name(lang), m.meta.description.get(lang), false)));
    }
    drop(modes);
    let s = a.s();
    s.set_wiz_modes(model(items));
    s.set_wiz_live(live);
    s.set_wiz_step(0);
    a.wizard.borrow_mut().opts = Some(LaunchOptions::from_config(&a.cfg.borrow()));
    s.set_wiz_open(true);
}

/// By prefix: de_, cs_, ar_, gd_, workshop/…
fn map_items(maps: &[String], query: &str) -> Vec<ModeItem> {
    let q = query.to_lowercase();
    let mut groups: Vec<(String, Vec<ModeItem>)> = Vec::new();
    for m in maps.iter().filter(|m| m.contains(&q)) {
        let pre = if m.starts_with("workshop/") || m.starts_with("csls/") {
            "Workshop".to_string()
        } else {
            m.split('_').next().unwrap_or("").to_string() + "_"
        };
        let i = match groups.iter().position(|(p, _)| *p == pre) {
            Some(i) => i,
            None => {
                groups.push((pre, Vec::new()));
                groups.len() - 1
            }
        };
        groups[i].1.push(item(m, m, "", false));
    }
    groups.into_iter().flat_map(|(p, v)| std::iter::once(item("", &p, "", true)).chain(v)).collect()
}

fn pick_mode(a: &AppRc, id: &str) {
    let modes = a.modes.borrow();
    let Ok(mode) = modes.get(id) else { return };
    let excl = modes.exclusive_prefixes();
    let maps_dir = Server::new(&a.cfg.borrow()).maps_dir();
    let maps: Vec<String> = installed_maps(&maps_dir)
        .into_iter()
        .filter(|m| if mode.meta.map_prefixes.is_empty() { !excl.iter().any(|p| m.starts_with(p.as_str())) } else { mode.accepts_map(m) })
        .collect();
    let s = a.s();
    s.set_wiz_search("".into());
    s.set_wiz_workshop("".into());
    s.set_wiz_maps(model(map_items(&maps, "")));
    let mut w = a.wizard.borrow_mut();
    w.maps = maps;
    if let Some(o) = w.opts.as_mut() {
        o.mode = id.to_string();
    }
    s.set_wiz_step(1);
}

fn fill_options(a: &AppRc) {
    let modes = a.modes.borrow();
    let mut w = a.wizard.borrow_mut();
    let Some(o) = w.opts.clone() else { return };
    let Ok(mode) = modes.get(&o.mode) else { return };
    let s = a.s();
    s.set_wiz_title(mode.name(csls_core::lang()).as_str().into());
    s.set_wiz_humans(o.humans as i32);
    s.set_wiz_no_bots(o.bots == Some(0));
    s.set_wiz_bots_on(false);
    s.set_wiz_difficulty(i32::from(o.difficulty));
    s.set_wiz_rounds(0);
    // only the types that fit the chosen mode
    let kinds: Vec<Kind> = Kind::ALL.into_iter().filter(|k| mode.allows_kind(*k)).collect();
    let names: Vec<String> = kinds.iter().map(|k| t(k.label())).collect();
    s.set_wiz_kinds(strings(&names));
    s.set_wiz_kind(kinds.iter().position(|k| *k == o.kind).unwrap_or(0) as i32);
    s.set_wiz_kind_show(mode.meta.game_type.is_none());
    s.set_wiz_site_show(mode.has_option("site"));
    s.set_wiz_site(0);
    s.set_wiz_teams_show(mode.has_option("teams"));
    // the mode's own choice unless this launch says otherwise
    let mode_teams = std::fs::read_to_string(&mode.cfg)
        .ok()
        .and_then(|c| csls_core::modes::CfgDoc::parse(&c).get("modo_equipos").and_then(Teams::parse));
    s.set_wiz_teams(match o.teams.or(mode_teams) {
        Some(Teams::Together) => 1,
        Some(Teams::Free) => 2,
        _ => 0,
    });
    s.set_wiz_nodraw_show(mode.has_option("nodraw"));
    s.set_wiz_nodraw(o.no_draw);
    s.set_wiz_precision(false);
    s.set_wiz_anarchy(false);
    s.set_wiz_extra("".into());
    s.set_wiz_preset("".into());
    s.set_wiz_advanced(false);
    w.kinds = kinds;
    w.mode_game_type = mode.meta.game_type;
    s.set_wiz_step(2);
}

fn pick_map(a: &AppRc, map: Option<String>, workshop: Option<u64>) {
    if let Some(o) = a.wizard.borrow_mut().opts.as_mut() {
        o.map = map;
        o.workshop = workshop;
    }
    fill_options(a);
}

/// Reads the options page back. It says what is wrong instead of starting something else.
fn read_options(a: &AppRc) -> anyhow::Result<LaunchOptions> {
    let s = a.s();
    let w = a.wizard.borrow();
    let mut o = w.opts.clone().ok_or_else(|| anyhow::anyhow!("no options"))?;
    o.humans = s.get_wiz_humans().max(0) as u32;
    o.bots = if s.get_wiz_no_bots() {
        Some(0)
    } else if s.get_wiz_bots_on() {
        Some(s.get_wiz_bots().max(0) as u32)
    } else {
        None
    };
    o.no_draw = s.get_wiz_nodraw_show() && s.get_wiz_nodraw();
    o.difficulty = s.get_wiz_difficulty().clamp(0, 3) as u8;
    o.rounds = (s.get_wiz_rounds() > 0).then(|| s.get_wiz_rounds() as u32);
    o.kind = w.kinds.get(s.get_wiz_kind().max(0) as usize).copied().unwrap_or_default();
    // the type is chosen after the map: if the map does not suit it, its own is used
    if let (Some((prefix, _)), None) = (o.kind.own_maps(), w.mode_game_type) {
        if o.map.as_deref().is_some_and(|m| !m.starts_with(prefix)) {
            o.map = None;
        }
    }
    o.site = s.get_wiz_site_show().then(|| [Site::Random, Site::A, Site::B][s.get_wiz_site().clamp(0, 2) as usize]);
    o.teams = s.get_wiz_teams_show().then(|| [Teams::Versus, Teams::Together, Teams::Free][s.get_wiz_teams().clamp(0, 2) as usize]);
    o.precision = s.get_wiz_precision();
    o.anarchy = s.get_wiz_anarchy();
    o.cvars.clear();
    for p in s.get_wiz_extra().split_whitespace() {
        o.cvars.push(parse_cvar(p)?);
    }
    let name = s.get_wiz_preset();
    if !name.trim().is_empty() {
        csls_core::presets::Presets::put(&name, &o)?;
        a.toast(&t("Preset saved"));
        a.modes_changed();
    }
    Ok(o)
}

fn apply(a: &AppRc, opts: LaunchOptions, live: bool) {
    let cfg = a.cfg.borrow().clone();
    let modes = a.modes.borrow().clone();
    let mode_id = opts.mode.clone();
    a.s().set_wiz_open(false);
    a.toast(&t("Starting…"));
    let a = a.clone();
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
                        a.warn(&warnings.join("\n"));
                    }
                }
                Err(e) => a.error(&e),
            }
            a.refresh();
        },
    );
}

/// Debug (screenshots): `mode[:map[:next]]` walks the wizard up to that step. `1` only opens it.
pub fn debug_open(a: &AppRc, path: &str) {
    open(a, false);
    let mut parts = path.split(':');
    if let Some(mode) = parts.next().filter(|m| *m != "1") {
        pick_mode(a, mode);
    }
    if let Some(map) = parts.next() {
        pick_map(a, Some(map.to_string()), None);
    }
    if parts.next().is_some() {
        a.s().invoke_wiz_options_next();
    }
}

pub fn init(a: &AppRc) {
    let s = a.s();
    let app = a.clone();
    s.on_wiz_pick_mode(move |id| pick_mode(&app, &id));
    let app = a.clone();
    s.on_wiz_pick_map(move |m| pick_map(&app, (!m.is_empty()).then(|| m.to_string()), None));
    let app = a.clone();
    s.on_wiz_search_changed(move || {
        let items = map_items(&app.wizard.borrow().maps, &app.s().get_wiz_search());
        app.s().set_wiz_maps(model(items));
    });
    let app = a.clone();
    s.on_wiz_pick_workshop(move || match app.s().get_wiz_workshop().trim().parse::<u64>() {
        Ok(id) => pick_map(&app, None, Some(id)),
        Err(_) => app.error(&anyhow::anyhow!(t("Workshop map ID"))),
    });
    let app = a.clone();
    s.on_wiz_options_next(move || {
        let live = app.s().get_wiz_live();
        match read_options(&app) {
            Ok(o) if live => apply(&app, o, true),
            Ok(o) => {
                app.wizard.borrow_mut().opts = Some(o);
                let c = app.cfg.borrow().server.clone();
                let s = app.s();
                s.set_wiz_net(NETS.iter().position(|n| *n == c.network).unwrap_or(0) as i32);
                s.set_wiz_name(c.hostname.as_str().into());
                s.set_wiz_pass(c.password.as_str().into());
                s.set_wiz_max(c.max_players as i32);
                s.set_wiz_gslt(c.gslt.as_str().into());
                s.set_wiz_step(3);
            }
            Err(e) => app.error(&e),
        }
    });
    let app = a.clone();
    s.on_wiz_start(move || {
        let s = app.s();
        let mut cfg = app.cfg.borrow().clone();
        cfg.server.network = NETS[s.get_wiz_net().clamp(0, 3) as usize];
        cfg.server.hostname = s.get_wiz_name().to_string();
        cfg.server.password = s.get_wiz_pass().to_string();
        cfg.server.max_players = s.get_wiz_max().max(2) as u32;
        cfg.server.gslt = s.get_wiz_gslt().trim().to_string();
        if let Err(e) = cfg.save() {
            app.error(&e);
            return;
        }
        *app.cfg.borrow_mut() = cfg;
        let Some(o) = app.wizard.borrow().opts.clone() else { return };
        apply(&app, o, false);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_group_by_prefix_and_filter() {
        let maps = ["de_dust2", "de_nuke", "cs_office", "workshop/1/x"].map(String::from);
        let all = map_items(&maps, "");
        let titles: Vec<String> = all.iter().map(|i| i.title.to_string()).collect();
        assert_eq!(titles, ["de_", "de_dust2", "de_nuke", "cs_", "cs_office", "Workshop", "workshop/1/x"]);
        assert!(all[0].heading && !all[1].heading);
        assert_eq!(map_items(&maps, "NUKE").len(), 2);
        assert!(map_items(&maps, "zzz").is_empty());
    }
}
