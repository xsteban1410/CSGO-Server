//! Generates addons/sourcemod/configs/skins.cfg from the game files (formerly tools/generar_skins.py).
//! Reads scripts/items/items_game.txt and resource/csgo_<lang>.txt. Order: most wanted, then rarity and name.
use crate::kv::{self, Kv};
use anyhow::Context;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const POPULAR: &[&str] = &[
    "Dragon Lore", "Howl", "Fire Serpent", "Fade", "Doppler", "Gamma Doppler", "Marble Fade",
    "Tiger Tooth", "Lore", "Autotronic", "Crimson Web", "Case Hardened", "Slaughter", "Asiimov",
    "Printstream", "Hyper Beast", "Neo-Noir", "Vulcan", "Redline", "Bloodsport", "The Empress",
    "Wild Lotus", "Gungnir", "Medusa", "Kill Confirmed", "Fever Dream", "Neon Rider", "Blaze",
    "Hot Rod", "Golden Coil", "Hydroponic", "Fuel Injector", "Gold Arabesque", "Lightning Strike",
    "Cyrex", "Oni Taiji", "Damascus Steel", "Ultraviolet", "Bright Water", "Blue Steel", "Night",
    "Stained", "Black Laminate", "Emerald Pinstripe", "Aquamarine Revenge", "Jaguar", "Head Shot",
    "Code Red", "Mecha Industries", "Desolate Space", "Hellfire", "Orion",
    "Cortex", "Bullet Rain", "Nightmare", "Chantico's Fire", "Icarus Fell", "Atheris", "The Prince",
];

fn rarity(r: &str) -> i32 {
    match r {
        "immortal" => 7,
        "ancient" => 6,
        "legendary" => 5,
        "mythical" => 4,
        "rare" => 3,
        "uncommon" => 2,
        "common" => 1,
        _ => 0,
    }
}

const VARIANTS: &[(&str, &str, &str)] = &[
    ("phase1", "Phase 1", "Fase 1"), ("phase2", "Phase 2", "Fase 2"), ("phase3", "Phase 3", "Fase 3"),
    ("phase4", "Phase 4", "Fase 4"), ("ruby", "Ruby", "Rubí"), ("sapphire", "Sapphire", "Zafiro"),
    ("blackpearl", "Black Pearl", "Perla negra"), ("emerald", "Emerald", "Esmeralda"),
];

/// All sections with this name, at any depth.
fn sections<'a>(t: &'a Kv, name: &str, out: &mut Vec<&'a Kv>) {
    for (k, v) in t.entries() {
        if let Kv::Obj(_) = v {
            if k.eq_ignore_ascii_case(name) {
                out.push(v);
            }
            sections(v, name, out);
        }
    }
}

fn find<'a>(t: &'a Kv, name: &str) -> Vec<&'a Kv> {
    let mut v = Vec::new();
    sections(t, name, &mut v);
    v
}

fn read_text(p: &Path) -> Option<String> {
    let raw = std::fs::read(p).ok()?;
    if raw.len() >= 2 && (raw[..2] == [0xff, 0xfe] || raw[..2] == [0xfe, 0xff]) {
        let le = raw[0] == 0xff;
        let u: Vec<u16> = raw[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return Some(String::from_utf16_lossy(&u));
    }
    Some(String::from_utf8_lossy(&raw).into_owned())
}

fn tokens(p: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(t) = read_text(p) {
        let doc = kv::parse(&t);
        for sec in find(&doc, "tokens") {
            for (k, v) in sec.entries() {
                if let Some(s) = v.str() {
                    out.insert(k.to_lowercase(), s.to_string());
                }
            }
        }
    }
    out
}

fn strs(body: &Kv) -> HashMap<String, &str> {
    body.entries().iter().filter_map(|(k, v)| Some((k.to_lowercase(), v.str()?))).collect()
}

fn esc(s: &str) -> String {
    s.replace('"', "'")
}

/// Returns the skins.cfg text and (weapons, knives, skins). `lang` = "es" (name + original) or "en".
pub fn generate(csgo: &Path, lang: &str) -> crate::Result<(String, (usize, usize, usize))> {
    let items_txt = std::fs::read_to_string(csgo.join("scripts/items/items_game.txt"))
        .or_else(|_| read_text(&csgo.join("scripts/items/items_game.txt")).context("no text"))
        .with_context(|| format!("leyendo {}/scripts/items/items_game.txt", csgo.display()))?;
    let items = kv::parse(&items_txt);
    let en = tokens(&csgo.join("resource/csgo_english.txt"));
    let local = if lang == "es" {
        let l = tokens(&csgo.join("resource/csgo_latam.txt"));
        if l.is_empty() { tokens(&csgo.join("resource/csgo_spanish.txt")) } else { l }
    } else {
        HashMap::new()
    };
    let loc = |tag: &str, def: &str| -> String {
        let k = tag.trim_start_matches('#').to_lowercase();
        local.get(&k).or_else(|| en.get(&k)).cloned().unwrap_or_else(|| def.to_string())
    };

    // internal name -> (id, local name, English name)
    let mut kits: HashMap<String, (u32, String, String)> = HashMap::new();
    for sec in find(&items, "paint_kits") {
        for (pid, body) in sec.entries() {
            let Ok(id) = pid.parse::<u32>() else { continue };
            if id == 0 || body.str().is_some() {
                continue;
            }
            let d = strs(body);
            if let (Some(n), Some(tag)) = (d.get("name"), d.get("description_tag")) {
                let en_n = en.get(&tag.trim_start_matches('#').to_lowercase()).cloned().unwrap_or_default();
                kits.insert(n.to_lowercase(), (id, loc(tag, ""), en_n));
            }
        }
    }
    let mut rar: HashMap<String, i32> = HashMap::new();
    for sec in find(&items, "paint_kits_rarity") {
        for (k, v) in sec.entries() {
            if let Some(s) = v.str() {
                rar.insert(k.to_lowercase(), rarity(s));
            }
        }
    }

    // weapon -> skins, from the "[skin]weapon_xxx" loot lists
    let mut by_weapon: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    fn walk(t: &Kv, kits: &HashMap<String, (u32, String, String)>, out: &mut BTreeMap<String, BTreeSet<String>>) {
        for (k, v) in t.entries() {
            if let Some(rest) = k.strip_prefix('[') {
                if let Some((skin, w)) = rest.split_once(']') {
                    let sk = skin.to_lowercase();
                    if w.starts_with("weapon_") && w[7..].bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') && kits.contains_key(&sk) {
                        out.entry(w.to_string()).or_default().insert(sk);
                    }
                }
            }
            if let Kv::Obj(_) = v {
                walk(v, kits, out);
            }
        }
    }
    walk(&items, &kits, &mut by_weapon);

    // knives: only appear in icons econ/default_generated/<weapon>_<skin>_light
    let mut known: BTreeSet<String> = BTreeSet::new();
    let mut index: HashMap<String, String> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for sec in find(&items, "items") {
        for (idx, body) in sec.entries() {
            if body.str().is_some() {
                continue;
            }
            let d = strs(body);
            let n = d.get("name").copied().unwrap_or("");
            if n.starts_with("weapon_") {
                known.insert(n.to_string());
                if idx.bytes().all(|b| b.is_ascii_digit()) {
                    index.entry(n.to_string()).or_insert_with(|| idx.clone());
                }
                if let Some(tag) = d.get("item_name") {
                    names.entry(n.to_string()).or_insert_with(|| loc(tag, n));
                }
            }
        }
    }
    fn icons(t: &Kv, known: &BTreeSet<String>, kits: &HashMap<String, (u32, String, String)>, out: &mut BTreeMap<String, BTreeSet<String>>) {
        for (k, v) in t.entries() {
            match v {
                Kv::Obj(_) => icons(v, known, kits, out),
                Kv::Str(s) if k.eq_ignore_ascii_case("icon_path") => {
                    let Some(p) = s.strip_prefix("econ/default_generated/").and_then(|p| p.strip_suffix("_light")) else { continue };
                    if !p.starts_with("weapon_") {
                        continue;
                    }
                    for (i, _) in p.match_indices('_') {
                        let (w, sk) = (&p[..i], &p[i + 1..]);
                        if known.contains(w) && kits.contains_key(sk) {
                            out.entry(w.to_string()).or_default().insert(sk.to_string());
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    icons(&items, &known, &kits, &mut by_weapon);
    for sec in find(&items, "prefabs") {
        for (p, body) in sec.entries() {
            if let Some(w) = p.strip_suffix("_prefab").filter(|w| w.starts_with("weapon_")) {
                if let Some(tag) = strs(body).get("item_name") {
                    names.entry(w.to_string()).or_insert_with(|| loc(tag, w));
                }
            }
        }
    }

    let pop: HashMap<String, usize> = POPULAR.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    let key = |k: &String| {
        let (_, l, e) = &kits[k];
        (*pop.get(&e.to_lowercase()).unwrap_or(&999), -rar.get(k).copied().unwrap_or(0), l.to_lowercase())
    };
    let is_knife = |w: &str| w.starts_with("weapon_knife_") || w == "weapon_bayonet";
    let name_of = |w: &str| names.get(w).cloned().unwrap_or_else(|| w.to_string());

    let mut weapons: Vec<&String> = by_weapon.keys().collect();
    weapons.sort_by_key(|w| (is_knife(w), name_of(w).to_lowercase()));
    let mut out = vec!["\"skins\"".to_string(), "{".into(), "\t\"armas\"".into(), "\t{".into()];
    let mut total = 0;
    for w in &weapons {
        out.push(format!("\t\t\"{w}\""));
        out.push("\t\t{".into());
        out.push(format!("\t\t\t\"nombre\"\t\"{}\"", esc(&name_of(w))));
        out.push(format!("\t\t\t\"indice\"\t\"{}\"", index.get(*w).map(String::as_str).unwrap_or("0")));
        out.push("\t\t\t\"pintura\"".into());
        out.push("\t\t\t{".into());
        let mut list: Vec<&String> = by_weapon[*w].iter().collect();
        list.sort_by_key(|k| key(k));
        let locals: Vec<&str> = list.iter().map(|k| kits[*k].1.as_str()).collect();
        for k in &list {
            let (id, l, e) = &kits[*k];
            let mut shown = if e.is_empty() || l == e || l.is_empty() { if l.is_empty() { e.clone() } else { l.clone() } } else { format!("{l} ({e})") };
            if locals.iter().filter(|x| **x == l).count() > 1 {
                // Doppler phases share a name
                let v = VARIANTS.iter().find(|(id, _, _)| k.contains(id)).map(|(_, en, es)| if lang == "es" { *es } else { *en }).unwrap_or(k.as_str());
                shown += &format!(" - {v}");
            }
            out.push(format!("\t\t\t\t\"{id}\"\t\"{}\"", esc(&shown)));
            total += 1;
        }
        out.push("\t\t\t}".into());
        out.push("\t\t}".into());
    }
    out.push("\t}".into());
    out.push("\t\"cuchillos\"".into());
    out.push("\t{".into());
    let knives: Vec<&&String> = weapons.iter().filter(|w| is_knife(w)).collect();
    let mut knives: Vec<&String> = knives.into_iter().copied().collect();
    knives.sort();
    let shown: Vec<String> = knives.iter().map(|k| name_of(k)).collect();
    for (k, v) in knives.iter().zip(&shown) {
        let v = if shown.iter().filter(|x| *x == v).count() == 1 { v.clone() } else { format!("{v} ({})", k.trim_start_matches("weapon_knife_")) };
        out.push(format!("\t\t\"{k}\"\t\"{}\"", esc(&v)));
    }
    out.push("\t}".into());
    out.push("}".into());
    Ok((out.join("\n") + "\n", (weapons.len(), knives.len(), total)))
}
