//! Workshop tab: search and install CS:GO maps, library, .nav for bots and import local .bsp.
//! CS2 maps show up marked as incompatible.
use crate::i18n::t;
use crate::{bg, btn, model, row, ActionRow, AppRc, Card};
use csls_core::server::Server;
use csls_core::workshop::{self, Compat, Item, Sort};
use slint::{Image, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const TAGS: &[&str] = &["", "Classic", "Deathmatch", "Armsrace", "Demolition", "Hostage", "Wingman", "Custom"];

fn mb(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1048576.0)
}

/// Accepts the number or a ...?id=123 link.
fn parse_id(txt: &str) -> Option<u64> {
    let digits: String = txt.trim().rsplit("id=").next().unwrap_or("").chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

struct Tab {
    cards: Rc<VecModel<Card>>,
    items: RefCell<HashMap<String, Item>>,
    page: Cell<u32>,
    installed: RefCell<Vec<u64>>,
    loaded: Cell<bool>,
    /// A newer search makes the answers of older ones useless.
    generation: Cell<u64>,
}

fn fill_library(a: &AppRc, tab: &Rc<Tab>) {
    let maps = Server::new(&a.cfg.borrow()).maps_dir();
    let running = a.live.borrow().rcon_ok;
    let (a, tab) = (a.clone(), tab.clone());
    bg(
        move || workshop::installed(&maps),
        move |list| {
            *tab.installed.borrow_mut() = list.iter().filter_map(|i| i.dir.parse().ok()).collect();
            let rows: Vec<ActionRow> = list
                .iter()
                .map(|i| {
                    let sub = format!("{} · {}", i.map, if i.has_nav { t("bots ready") } else { t("no .nav for bots") });
                    let mut b = Vec::new();
                    if running {
                        b.push(btn(&format!("load:{}", i.map), &t("Load")));
                        if !i.has_nav {
                            b.push(btn(&format!("nav:{}", i.map), &t("Generate .nav")));
                        }
                    }
                    let mut del = btn("del", &t("Delete"));
                    del.danger = true;
                    b.push(del);
                    row(&i.dir, &i.title, &sub, b)
                })
                .collect();
            a.s().set_ws_library(model(rows));
        },
    );
}

fn card(it: &Item, installed: bool) -> Card {
    let (badge, kind) = match it.compat() {
        Compat::Csgo => (format!("CS:GO · {}", mb(it.file_size)), "ok"),
        Compat::Cs2 => (t("CS2 only — incompatible"), "bad"),
        Compat::Unknown if it.collection => (t("Collection"), "accent"),
        Compat::Unknown => (t("Not a map"), "dim"),
    };
    Card {
        id: it.id.to_string().into(),
        title: it.title.as_str().into(),
        badge: badge.into(),
        kind: kind.into(),
        can: it.compat() == Compat::Csgo || it.collection,
        installed,
        thumb: Image::default(),
        tip: format!("{} · {}", it.id, it.tags.join(", ")).into(),
    }
}

fn set_card(tab: &Tab, id: &str, f: impl FnOnce(&mut Card)) {
    if let Some((i, mut c)) = tab.cards.iter().enumerate().find(|(_, c)| c.id == id) {
        f(&mut c);
        tab.cards.set_row_data(i, c);
    }
}

/// Installs items (collections get expanded) and notifies.
fn install(a: &AppRc, tab: &Rc<Tab>, items: Vec<Item>) {
    let maps = Server::new(&a.cfg.borrow()).maps_dir();
    a.toast(&t("Installing…"));
    let (a, tab) = (a.clone(), tab.clone());
    bg(
        move || -> Vec<Result<String, String>> {
            let mut out = Vec::new();
            for it in items {
                let list = if it.collection {
                    match workshop::collection(it.id).and_then(|ids| workshop::details(&ids)) {
                        Ok(l) => l,
                        Err(e) => {
                            out.push(Err(format!("{e:#}")));
                            continue;
                        }
                    }
                } else {
                    vec![it]
                };
                for i in list {
                    out.push(workshop::install(&maps, &i).map(|_| i.title.clone()).map_err(|e| format!("{e:#}")));
                }
            }
            out
        },
        move |res| {
            let ok: Vec<String> = res.iter().filter_map(|r| r.as_ref().ok().cloned()).collect();
            let err: Vec<String> = res.iter().filter_map(|r| r.as_ref().err().cloned()).collect();
            if !ok.is_empty() {
                a.toast(&format!("{}: {}", t("Installed"), ok.join(", ")));
            }
            if !err.is_empty() {
                a.error(&anyhow::anyhow!(err.join("\n")));
            }
            fill_library(&a, &tab);
        },
    );
}

fn run_search(a: &AppRc, tab: &Rc<Tab>, append: bool) {
    let s = a.s();
    if !append {
        tab.page.set(1);
        tab.cards.set_vec(Vec::new());
        tab.items.borrow_mut().clear();
    }
    let generation = tab.generation.get() + 1;
    tab.generation.set(generation);
    let text = s.get_ws_search().to_string();
    let tag = TAGS[s.get_ws_tag().clamp(0, TAGS.len() as i32 - 1) as usize];
    let tag = (!tag.is_empty()).then(|| tag.to_string());
    let sort = [Sort::Trend, Sort::Popular, Sort::Recent][s.get_ws_sort().clamp(0, 2) as usize];
    let page = tab.page.get();
    s.set_ws_busy(true);
    s.set_ws_more(false);
    let (a, tab) = (a.clone(), tab.clone());
    bg(
        move || workshop::search(&text, tag.as_deref(), sort, page),
        move |r: anyhow::Result<Vec<Item>>| {
            if tab.generation.get() != generation {
                return;
            }
            a.s().set_ws_busy(false);
            match r {
                Ok(items) => {
                    a.s().set_ws_more(!items.is_empty());
                    tab.page.set(page + 1);
                    for it in items {
                        let id = it.id.to_string();
                        tab.cards.push(card(&it, tab.installed.borrow().contains(&it.id)));
                        tab.items.borrow_mut().insert(id.clone(), it.clone());
                        let tab2 = tab.clone();
                        bg(
                            move || workshop::thumbnail(&it),
                            move |r| {
                                let Ok(path) = r else { return };
                                let Ok(img) = Image::load_from_path(&path) else { return };
                                set_card(&tab2, &id, |c| c.thumb = img);
                            },
                        );
                    }
                }
                Err(e) => a.error(&e),
            }
        },
    );
}

fn generate_nav(a: &AppRc, tab: &Rc<Tab>, map: String) {
    a.toast(&t("Generating .nav (seconds to minutes)…"));
    let cfg = a.cfg.borrow().clone();
    let (a, tab) = (a.clone(), tab.clone());
    bg(
        move || -> anyhow::Result<String> {
            let srv = Server::new(&cfg);
            let mut r = srv.rcon()?;
            if Server::current_map(&mut r).as_deref() != Some(map.as_str()) {
                r.exec(&format!("changelevel {map}"))?;
                drop(r);
                std::thread::sleep(Duration::from_secs(8));
                srv.wait_rcon(Duration::from_secs(60))?;
            }
            srv.generate_nav(Duration::from_secs(600), false)
        },
        move |r| {
            match r {
                Ok(m) => a.toast(&format!("{}: {m}", t("Bots ready"))),
                Err(e) => a.error(&e),
            }
            fill_library(&a, &tab);
        },
    );
}

pub fn init(a: &AppRc) {
    let tab = Rc::new(Tab {
        cards: Rc::new(VecModel::default()),
        items: RefCell::default(),
        page: Cell::new(1),
        installed: RefCell::default(),
        loaded: Cell::new(false),
        generation: Cell::new(0),
    });
    let s = a.s();
    s.set_ws_cards(ModelRc::from(tab.cards.clone()));
    fill_library(a, &tab);

    // the Load / .nav buttons depend on the server answering: only rebuilt when that changes
    let was = Cell::new(a.live.borrow().rcon_ok);
    let (app, t2) = (a.clone(), tab.clone());
    a.on_live(move |l| {
        if was.replace(l.rcon_ok) != l.rcon_ok {
            fill_library(&app, &t2);
        }
    });

    let (app, t2) = (a.clone(), tab.clone());
    s.on_ws_library_action(move |dir, action| {
        if action == "del" {
            match workshop::remove(&Server::new(&app.cfg.borrow()).maps_dir(), &dir) {
                Ok(()) => fill_library(&app, &t2),
                Err(e) => app.error(&e),
            }
        } else if let Some(map) = action.strip_prefix("nav:") {
            generate_nav(&app, &t2, map.to_string());
        } else if let Some(map) = action.strip_prefix("load:") {
            let (cfg, map) = (app.cfg.borrow().clone(), map.to_string());
            let a = app.clone();
            bg(
                move || Server::new(&cfg).rcon()?.exec(&format!("changelevel {map}")),
                move |r: anyhow::Result<String>| {
                    if let Err(e) = r {
                        a.error(&e);
                    }
                    a.refresh();
                },
            );
        }
    });
    let (app, t2) = (a.clone(), tab.clone());
    s.on_ws_run_search(move |append| run_search(&app, &t2, append));
    // lazy first load: when the tab is shown
    let (app, t2) = (a.clone(), tab.clone());
    s.on_ws_shown(move || {
        if !t2.loaded.replace(true) {
            run_search(&app, &t2, false);
        }
    });
    let (app, t2) = (a.clone(), tab.clone());
    s.on_ws_install(move |id| {
        let Some(it) = t2.items.borrow().get(id.as_str()).cloned() else { return };
        set_card(&t2, &id, |c| c.can = false);
        install(&app, &t2, vec![it]);
    });
    let (app, t2) = (a.clone(), tab.clone());
    s.on_ws_install_id(move || {
        let Some(id) = parse_id(&app.s().get_ws_by_id()) else {
            app.error(&anyhow::anyhow!("{} {}", t("Not found:"), app.s().get_ws_by_id()));
            return;
        };
        let (a, tab) = (app.clone(), t2.clone());
        bg(
            move || workshop::details(&[id]),
            move |r: anyhow::Result<Vec<Item>>| match r {
                Ok(items) if !items.is_empty() => install(&a, &tab, items),
                Ok(_) => a.error(&anyhow::anyhow!("{} {id}", t("Not found:"))),
                Err(e) => a.error(&e),
            },
        );
    });
    let (app, t2) = (a.clone(), tab);
    s.on_ws_import(move || {
        let (a, tab) = (app.clone(), t2.clone());
        bg(
            || rfd::FileDialog::new().set_title(t("Import .bsp")).add_filter("BSP", &["bsp"]).pick_file(),
            move |p| {
                let Some(p) = p else { return };
                match workshop::import_local(&Server::new(&a.cfg.borrow()).maps_dir(), &p) {
                    Ok(m) => {
                        a.toast(&format!("{}: {m}", t("Installed")));
                        fill_library(&a, &tab);
                    }
                    Err(e) => a.error(&e),
                }
            },
        );
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_from_numbers_and_links() {
        assert_eq!(super::parse_id(" 123 "), Some(123));
        assert_eq!(super::parse_id("https://steamcommunity.com/sharedfiles/filedetails/?id=456&x=1"), Some(456));
        assert_eq!(super::parse_id("nothing"), None);
    }
}
