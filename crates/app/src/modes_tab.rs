//! Modes tab: custom modes (create, duplicate, edit, import, export, delete).
use crate::i18n::t;
use crate::{bg, btn, model, row, strings, AppRc, RuleEdit, TeamEdit};
use csls_core::launch::LaunchOptions;
use csls_core::modes::{self, CfgDoc, Localized, Mode, ModeMeta};
use csls_core::paths;
use csls_core::rules::{self, Rule, ACTIONS, EVENTS, MAX_RULES};
use csls_core::server::Server;
use slint::{Model, ModelRc, VecModel};
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
/// Valve base: (game_type, game_mode), in the order of the combo.
const BASES: [Option<(u8, u8)>; 4] = [None, Some((1, 2)), Some((1, 0)), Some((1, 1))];

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
        "mp_roundtime",
        "bot_quota",
        "bot_difficulty",
    ];
    K.contains(&k)
        || rules::rule_index(k).is_some()
        || ["modo_t_", "modo_ct_"]
            .iter()
            .any(|p| k.strip_prefix(p).is_some_and(|r| ["quitar", "arma", "pistola", "objeto", "vida", "velocidad", "color"].contains(&r)))
}

/// The values of a combo, with one the list does not have added at the end, and which one is selected.
fn options(list: &[&str], value: &str) -> (Vec<String>, i32) {
    let mut values: Vec<String> = list.iter().map(|s| s.to_string()).collect();
    let i = match values.iter().position(|v| v == value) {
        Some(i) => i,
        None if value.is_empty() => 0,
        None => {
            values.push(value.to_string());
            values.len() - 1
        }
    };
    (values, i as i32)
}

fn labels(values: &[String]) -> Vec<String> {
    values.iter().map(|s| if s.is_empty() { "—".to_string() } else { s.trim_start_matches("weapon_").to_string() }).collect()
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Reads a number typed in a text box: empty is the default.
fn number(text: &str, default: f64, what: &str) -> anyhow::Result<f64> {
    let s = text.trim().replace(',', ".");
    if s.is_empty() {
        return Ok(default);
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| anyhow::anyhow!("{what}: {text}"))
}

/// What the editor keeps while it is open and the UI does not hold.
#[derive(Default)]
pub struct State {
    meta: ModeMeta,
    editing: bool,
    /// Primary, pistol and item values of each team, in the order of its combos.
    t: [Vec<String>; 3],
    ct: [Vec<String>; 3],
}

fn load_team(d: &CfgDoc, p: &str) -> (TeamEdit, [Vec<String>; 3]) {
    let get = |k: &str| d.get(&format!("modo_{p}_{k}")).unwrap_or("").to_string();
    let (w, wi) = options(PRIMARIES, &get("arma"));
    let (pi, pii) = options(PISTOLS, &get("pistola"));
    let (it, iti) = options(ITEMS, &get("objeto"));
    let speed = get("velocidad");
    let team = TeamEdit {
        strip: get("quitar") == "1",
        weapon: wi,
        pistol: pii,
        item: iti,
        health: get("vida").parse::<f64>().unwrap_or(0.0) as i32,
        speed: if speed.is_empty() { "1".into() } else { speed.into() },
        color: get("color").into(),
    };
    (team, [w, pi, it])
}

fn write_team(d: &mut CfgDoc, p: &str, team: &TeamEdit, lists: &[Vec<String>; 3]) -> anyhow::Result<()> {
    let value = |list: &Vec<String>, i: i32| list.get(i.max(0) as usize).cloned().unwrap_or_default();
    let mut put = |k: &str, v: String, def: &str| {
        if v != def {
            d.set(&format!("modo_{p}_{k}"), &v);
        }
    };
    put("quitar", if team.strip { "1".into() } else { "0".into() }, "0");
    put("arma", value(&lists[0], team.weapon), "");
    put("pistola", value(&lists[1], team.pistol), "");
    put("objeto", value(&lists[2], team.item), "");
    put("vida", team.health.max(0).to_string(), "0");
    put("velocidad", fmt(number(&team.speed, 1.0, &t("Speed"))?), "1");
    put("color", team.color.trim().to_string(), "");
    Ok(())
}

/// Opens the editor. `from`: mode to edit (if the user's) or duplicate (if official).
fn editor(a: &AppRc, from: Option<&Mode>) {
    let lang = csls_core::lang();
    let editing = from.is_some_and(|m| m.custom);
    let doc = from.and_then(|m| std::fs::read_to_string(&m.cfg).ok()).map(|x| CfgDoc::parse(&x)).unwrap_or_default();
    let meta = from.map(|m| m.meta.clone()).unwrap_or_default();
    let s = a.s();
    s.set_ed_title(if editing { t("Edit mode") } else { t("New mode") }.into());
    s.set_ed_editing(editing);
    s.set_ed_id(
        match from {
            Some(m) if m.custom => m.id.clone(),
            Some(m) => format!("{}_2", m.id),
            None => String::new(),
        }
        .into(),
    );
    s.set_ed_name(from.map(|m| m.name(lang)).unwrap_or_default().into());
    s.set_ed_desc(meta.description.get(lang).into());
    let base = match (meta.game_type, meta.game_mode) {
        (Some(t), Some(g)) => BASES.iter().position(|b| *b == Some((t, g))).unwrap_or(0),
        _ => 0,
    };
    s.set_ed_base(base as i32);
    s.set_ed_prefixes(meta.map_prefixes.join(" ").into());

    let (team_t, lists_t) = load_team(&doc, "t");
    let (team_ct, lists_ct) = load_team(&doc, "ct");
    s.set_ed_t(team_t);
    s.set_ed_ct(team_ct);
    s.set_ed_t_weapons(strings(&labels(&lists_t[0])));
    s.set_ed_t_pistols(strings(&labels(&lists_t[1])));
    s.set_ed_t_items(strings(&labels(&lists_t[2])));
    s.set_ed_ct_weapons(strings(&labels(&lists_ct[0])));
    s.set_ed_ct_pistols(strings(&labels(&lists_ct[1])));
    s.set_ed_ct_items(strings(&labels(&lists_ct[2])));

    let num = |k: &str, def: f64| doc.get(k).and_then(|v| v.parse().ok()).unwrap_or(def);
    let on = |k: &str| doc.get(k).is_some_and(|v| v != "0" && !v.is_empty());
    s.set_ed_gravity(num("sv_gravity", 800.0) as i32);
    s.set_ed_respawn(on("mp_respawn_on_death_t") || on("mp_respawn_on_death_ct"));
    s.set_ed_headshot(on("mp_damage_headshot_only"));
    s.set_ed_ammo(num("sv_infinite_ammo", 0.0) as i32);
    s.set_ed_buy(on("mp_buy_anywhere"));
    s.set_ed_air(num("sv_airaccelerate", 12.0) as i32);
    s.set_ed_vampire(fmt(num("modo_vampiro", 0.0)).into());
    s.set_ed_chamber(on("modo_unabala"));
    s.set_ed_random(on("modo_aleatorio"));
    s.set_ed_boss(num("modo_jefe", 0.0) as i32);
    s.set_ed_anarchy(on("modo_anarquia"));
    s.set_ed_cheats(on("modo_cheats"));
    s.set_ed_zombies(on("modo_zombies"));
    s.set_ed_roundtime(doc.get("mp_roundtime").unwrap_or("").into());
    s.set_ed_bots(doc.get("bot_quota").and_then(|v| v.parse().ok()).unwrap_or(-1));
    s.set_ed_difficulty(doc.get("bot_difficulty").and_then(|v| v.parse::<i32>().ok()).filter(|d| (0..=3).contains(d)).unwrap_or(4));
    s.set_ed_rules(ModelRc::from(Rc::new(VecModel::from(load_rules(&doc)))));
    let extra: String = doc.pairs.iter().filter(|(k, _)| !known(k)).map(|(k, v)| format!("{k} {v}\n")).collect();
    s.set_ed_extra(extra.into());
    *a.editor.borrow_mut() = State { meta, editing, t: lists_t, ct: lists_ct };
    s.set_ed_open(true);
}

fn load_rules(d: &CfgDoc) -> Vec<RuleEdit> {
    let index = |list: &[&str], v: &str| list.iter().position(|x| *x == v).unwrap_or(0) as i32;
    d.pairs
        .iter()
        .filter(|(k, _)| rules::rule_index(k).is_some())
        .filter_map(|(_, v)| Rule::parse(v).ok())
        .map(|r| RuleEdit { event: index(&EVENTS, &r.event), action: index(&ACTIONS, &r.action), value: r.value.into() })
        .collect()
}

fn write_rules(d: &mut CfgDoc, list: impl Iterator<Item = RuleEdit>) -> anyhow::Result<()> {
    for (i, r) in list.enumerate() {
        let event = EVENTS[r.event.clamp(0, 3) as usize];
        let action = ACTIONS[r.action.clamp(0, 4) as usize];
        d.set(&rules::key(i), &Rule::new(event, action, &r.value)?.to_value());
    }
    Ok(())
}

/// Everything goes through `CfgDoc::validate`, the same as a mode written by hand.
fn build(a: &AppRc) -> anyhow::Result<(String, ModeMeta, String)> {
    let s = a.s();
    let st = a.editor.borrow();
    let id = s.get_ed_id().trim().to_string();
    let mut d = CfgDoc { comment: format!("{id}: {}", s.get_ed_desc()), pairs: vec![] };
    write_team(&mut d, "t", &s.get_ed_t(), &st.t)?;
    write_team(&mut d, "ct", &s.get_ed_ct(), &st.ct)?;
    let vampire = number(&s.get_ed_vampire(), 0.0, &t("Vampire: fraction of damage that heals (0 = off)"))?;
    let bit = |b: bool| u8::from(b).to_string();
    let mut put = |k: &str, v: String, def: &str| {
        if v != def {
            d.set(k, &v);
        }
    };
    put("sv_gravity", s.get_ed_gravity().to_string(), "800");
    put("mp_respawn_on_death_t", bit(s.get_ed_respawn()), "0");
    put("mp_respawn_on_death_ct", bit(s.get_ed_respawn()), "0");
    put("mp_damage_headshot_only", bit(s.get_ed_headshot()), "0");
    put("sv_infinite_ammo", s.get_ed_ammo().clamp(0, 2).to_string(), "0");
    put("mp_buy_anywhere", bit(s.get_ed_buy()), "0");
    put("sv_airaccelerate", s.get_ed_air().to_string(), "12");
    put("modo_vampiro", fmt(vampire), "0");
    put("modo_unabala", bit(s.get_ed_chamber()), "0");
    put("modo_aleatorio", bit(s.get_ed_random()), "0");
    put("modo_jefe", s.get_ed_boss().to_string(), "0");
    put("modo_anarquia", bit(s.get_ed_anarchy()), "0");
    put("modo_cheats", bit(s.get_ed_cheats()), "0");
    put("modo_zombies", bit(s.get_ed_zombies()), "0");
    let round = s.get_ed_roundtime();
    if !round.trim().is_empty() {
        put("mp_roundtime", fmt(number(&round, 0.0, &t("Round time, minutes (empty = the mode's)"))?), "");
    }
    if s.get_ed_bots() >= 0 {
        put("bot_quota", s.get_ed_bots().to_string(), "");
    }
    if (0..=3).contains(&s.get_ed_difficulty()) {
        put("bot_difficulty", s.get_ed_difficulty().to_string(), "");
    }
    write_rules(&mut d, s.get_ed_rules().iter())?;
    for l in s.get_ed_extra().lines() {
        let mut it = l.trim().splitn(2, char::is_whitespace);
        if let Some(k) = it.next().filter(|k| !k.is_empty() && !k.starts_with("//")) {
            d.set(k, it.next().unwrap_or("").trim().trim_matches('"'));
        }
    }
    d.validate()?;
    let mut meta = st.meta.clone();
    let (n, ds) = (s.get_ed_name().to_string(), s.get_ed_desc().to_string());
    if csls_core::lang() == "es" {
        meta.name.es = n;
        meta.description.es = ds;
    } else {
        meta.name.en = n;
        meta.description.en = ds;
    }
    if meta.name.en.is_empty() && meta.name.es.is_empty() {
        meta.name = Localized::same(&id);
    }
    meta.category = "custom".into();
    (meta.game_type, meta.game_mode) = match BASES[s.get_ed_base().clamp(0, 3) as usize] {
        Some((t, g)) => (Some(t), Some(g)),
        None => (None, None),
    };
    meta.map_prefixes = s.get_ed_prefixes().split_whitespace().map(str::to_string).collect();
    if meta.map_prefixes.is_empty() {
        meta.default_map = None;
    }
    meta.exclusive_maps = false;
    Ok((id, meta, d.render()))
}

fn save(a: &AppRc, try_it: bool) {
    let editing = a.editor.borrow().editing;
    let r = build(a).and_then(|(id, meta, cfg)| {
        if !editing && a.modes.borrow().0.contains_key(&id) {
            anyhow::bail!("{} '{id}'", t("A mode already exists with id"));
        }
        modes::save_user_mode(&paths::user_modes_dir(), &id, &meta, &cfg)?;
        Ok(id)
    });
    let id = match r {
        Ok(id) => id,
        Err(e) => return a.error(&e),
    };
    fill(a);
    a.s().set_ed_open(false);
    a.toast(&t("Saved"));
    if try_it {
        let cfg = a.cfg.borrow().clone();
        let modes = a.modes.borrow().clone();
        let mut o = LaunchOptions::from_config(&cfg);
        o.mode = id;
        let a = a.clone();
        bg(
            move || Server::new(&cfg).change(&modes, &o),
            move |r: anyhow::Result<String>| {
                match r {
                    Ok(m) => a.toast(&format!("{} · {m}", t("Mode changed"))),
                    Err(e) => a.error(&e),
                }
                a.refresh();
            },
        );
    }
}

fn fill(a: &AppRc) {
    a.reload_modes();
    let lang = csls_core::lang();
    let (mut mine, mut builtin) = (Vec::new(), Vec::new());
    for m in a.modes.borrow().0.values() {
        let sub = format!("{} · {}", m.id, m.meta.description.get(lang));
        if m.custom {
            let mut del = btn("del", &t("Delete"));
            del.danger = true;
            mine.push(row(&m.id, &m.name(lang), &sub, vec![btn("edit", &t("Edit")), btn("export", &t("Export")), del]));
        } else {
            builtin.push(row(&m.id, &m.name(lang), &sub, vec![btn("dup", &t("Duplicate"))]));
        }
    }
    let s = a.s();
    s.set_no_modes(a.modes.borrow().0.is_empty());
    s.set_md_mine(model(mine));
    s.set_md_builtin(model(builtin));
}

fn export(a: &AppRc, m: &Mode) {
    let txt = match modes::export_mode(m) {
        Ok(x) => x,
        Err(e) => return a.error(&e),
    };
    let name = format!("{}.toml", m.id);
    let a = a.clone();
    bg(
        move || -> anyhow::Result<bool> {
            let Some(p) = rfd::FileDialog::new().set_title(t("Export")).set_file_name(name).save_file() else { return Ok(false) };
            std::fs::write(p, txt)?;
            Ok(true)
        },
        move |r| match r {
            Ok(true) => a.toast(&t("Saved")),
            Ok(false) => {}
            Err(e) => a.error(&e),
        },
    );
}

fn import(a: &AppRc) {
    let a = a.clone();
    bg(
        || rfd::FileDialog::new().set_title(t("Import")).add_filter("TOML", &["toml"]).pick_file(),
        move |p| {
            let Some(p) = p else { return };
            let r = std::fs::read_to_string(p).map_err(anyhow::Error::from).and_then(|x| modes::import_mode(&paths::user_modes_dir(), &x));
            match r {
                Ok(id) => {
                    fill(&a);
                    a.toast(&format!("{} {id}", t("Imported mode")));
                }
                Err(e) => a.error(&e),
            }
        },
    );
}

pub fn init(a: &AppRc) {
    fill(a);
    let s = a.s();
    let app = a.clone();
    s.on_md_new(move || editor(&app, None));
    let app = a.clone();
    s.on_md_import(move || import(&app));
    let app = a.clone();
    s.on_md_builtin_action(move |id, _| {
        let m = app.modes.borrow().0.get(id.as_str()).cloned();
        editor(&app, m.as_ref());
    });
    let app = a.clone();
    s.on_md_mine_action(move |id, action| {
        let Some(m) = app.modes.borrow().0.get(id.as_str()).cloned() else { return };
        match action.as_str() {
            "edit" => editor(&app, Some(&m)),
            "export" => export(&app, &m),
            _ => {
                let (a2, id) = (app.clone(), m.id.clone());
                app.ask(&format!("{} {id}?", t("Delete")), "", &t("Delete"), &t("Cancel"), true, move |yes| {
                    if yes {
                        match modes::delete_user_mode(&paths::user_modes_dir(), &id) {
                            Ok(()) => fill(&a2),
                            Err(e) => a2.error(&e),
                        }
                    }
                });
            }
        }
    });
    let app = a.clone();
    s.on_ed_save(move |try_it| save(&app, try_it));
    let app = a.clone();
    s.on_ed_show_preview(move || match build(&app) {
        Ok((id, _, cfg)) => app.ask(&format!("modo_{id}.cfg"), &cfg, "OK", "", false, |_| {}),
        Err(e) => app.error(&e),
    });
    let app = a.clone();
    s.on_ed_rule_add(move || {
        let list = app.s().get_ed_rules();
        if list.row_count() >= MAX_RULES {
            return;
        }
        if let Some(m) = list.as_any().downcast_ref::<VecModel<RuleEdit>>() {
            m.push(RuleEdit { event: 0, action: 0, value: "".into() });
        }
    });
    let app = a.clone();
    s.on_ed_rule_remove(move |i| {
        if let Some(m) = app.s().get_ed_rules().as_any().downcast_ref::<VecModel<RuleEdit>>() {
            if (i as usize) < m.row_count() {
                m.remove(i as usize);
            }
        }
    });
    let app = a.clone();
    s.on_ed_rule_changed(move |i, event, action, value| {
        app.s().get_ed_rules().set_row_data(i as usize, RuleEdit { event, action, value });
    });
    // modes copied to the folder by hand
    let app = a.clone();
    a.on_modes_changed(move || fill(&app));
}

/// Debug (screenshots): opens the editor duplicating mode `id`.
pub fn debug_open_editor(a: &AppRc, id: &str) {
    let m = a.modes.borrow().0.get(id).cloned();
    editor(a, m.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_outside_the_list_is_kept() {
        let (v, i) = options(PISTOLS, "weapon_deagle");
        assert_eq!((v.len(), i), (PISTOLS.len(), 1));
        let (v, i) = options(PISTOLS, "weapon_custom");
        assert_eq!(v[i as usize], "weapon_custom");
        assert_eq!(options(PISTOLS, "").1, 0);
        assert_eq!(labels(&["".into(), "weapon_awp".into(), "item_defuser".into()]), ["—", "awp", "item_defuser"]);
    }

    #[test]
    fn a_team_survives_being_written_and_read() {
        let mut d = CfgDoc::default();
        let (team, lists) = load_team(&d, "t");
        write_team(&mut d, "t", &team, &lists).unwrap();
        assert!(d.pairs.is_empty(), "defaults write nothing: {:?}", d.pairs);
        d.set("modo_t_arma", "weapon_awp");
        d.set("modo_t_velocidad", "1.5");
        d.set("modo_t_vida", "200");
        let (team, lists) = load_team(&d, "t");
        let mut back = CfgDoc::default();
        write_team(&mut back, "t", &team, &lists).unwrap();
        assert_eq!(back.get("modo_t_arma"), Some("weapon_awp"));
        assert_eq!(back.get("modo_t_velocidad"), Some("1.5"));
        assert_eq!(back.get("modo_t_vida"), Some("200"));
    }

    #[test]
    fn rules_go_through_the_catalog() {
        let mut d = CfgDoc::default();
        let good = RuleEdit { event: 1, action: 1, value: "25".into() };
        write_rules(&mut d, [good].into_iter()).unwrap();
        assert_eq!(d.get("modo_regla_1"), Some("kill health 25"));
        d.validate().unwrap();
        let back = load_rules(&d);
        assert_eq!((back[0].event, back[0].action, back[0].value.as_str()), (1, 1, "25"));
        let bad = RuleEdit { event: 0, action: 0, value: "weapon_;quit".into() };
        assert!(write_rules(&mut CfgDoc::default(), [bad].into_iter()).is_err());
    }

    #[test]
    fn numbers_from_text_boxes() {
        assert_eq!(number("", 1.0, "x").unwrap(), 1.0);
        assert_eq!(number("1,5", 1.0, "x").unwrap(), 1.5);
        assert!(number("abc", 1.0, "x").is_err());
        assert!(number("-1", 1.0, "x").is_err());
        assert_eq!(fmt(1.50), "1.5");
        assert_eq!(fmt(800.0), "800");
    }
}
