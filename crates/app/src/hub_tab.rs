//! Hub tab: game modes, plugins and packs from CSGO-Hub, with previews. Installing or removing only touches
//! the hub folder; the folder watcher deploys and reloads the modes.
use crate::i18n::t;
use crate::{bg, AppRc, HubCard};
use csls_core::hub::{self, Entry, Index, Kind};
use slint::{Image, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The filters of the combo box, in its order.
const FILTERS: usize = 6;

struct Tab {
    cards: Rc<VecModel<HubCard>>,
    index: RefCell<Option<Index>>,
    loaded: Cell<bool>,
    /// previews already asked for, so filtering again does not download them again
    thumbs: RefCell<std::collections::HashMap<String, Image>>,
}

fn kind_label(k: Kind) -> String {
    match k {
        Kind::Mode => t("Mode"),
        Kind::Plugin => t("Plugin"),
        Kind::Pack => t("Pack"),
        Kind::Map => t("Maps"),
    }
}

fn shown(e: &Entry, filter: usize, text: &str, installed: bool, update: bool) -> bool {
    let lang = csls_core::lang();
    let kind_ok = match filter {
        1 => e.item.kind == Kind::Mode,
        2 => e.item.kind == Kind::Plugin,
        3 => e.item.kind == Kind::Pack,
        4 => installed,
        5 => update,
        _ => true,
    };
    let text = text.trim().to_lowercase();
    kind_ok
        && (text.is_empty()
            || [e.item.id.as_str(), e.item.name.get(lang), e.item.description.get(lang)]
                .iter()
                .chain(e.item.tags.iter().map(String::as_str).collect::<Vec<_>>().iter())
                .any(|s| s.to_lowercase().contains(&text)))
}

fn card(e: &Entry, installed: Option<&str>, thumb: Image) -> HubCard {
    let lang = csls_core::lang();
    let it = &e.item;
    HubCard {
        id: it.id.as_str().into(),
        title: it.name.get(lang).into(),
        kind: kind_label(it.kind).into(),
        version: it.version.as_str().into(),
        description: it.description.get(lang).into(),
        author: it.author.as_str().into(),
        requires: it.requires.join(", ").into(),
        tags: it.tags.join(", ").into(),
        installed: installed.is_some(),
        update: installed.is_some_and(|v| csls_core::updates::is_newer(&it.version, v)),
        code: e.has_code,
        compatible: hub::compatible(it).is_ok(),
        busy: false,
        thumb,
    }
}

/// Rebuilds the cards from the index, the installed items and the filter.
fn fill(a: &AppRc, tab: &Rc<Tab>) {
    let s = a.s();
    let ix = tab.index.borrow();
    let Some(ix) = ix.as_ref() else { return };
    let installed: Vec<(String, String)> = hub::installed().into_iter().map(|i| (i.item.id, i.item.version)).collect();
    let version = |id: &str| installed.iter().find(|(i, _)| i == id).map(|(_, v)| v.as_str());
    let filter = (s.get_hb_filter().max(0) as usize).min(FILTERS - 1);
    let text = s.get_hb_search().to_string();
    let mut cards = Vec::new();
    let mut updates = 0;
    for e in &ix.items {
        let c = card(e, version(&e.item.id), tab.thumbs.borrow().get(&e.item.id).cloned().unwrap_or_default());
        updates += i32::from(c.update);
        if shown(e, filter, &text, c.installed, c.update) {
            cards.push(c);
        }
    }
    s.set_hb_updates(updates);
    s.set_hb_installed(installed.len() as i32);
    tab.cards.set_vec(cards);
    // previews of what is on screen, once each
    for e in ix.items.iter().filter(|e| !e.preview_url.is_empty()) {
        if tab.thumbs.borrow().contains_key(&e.item.id) || !tab.cards.iter().any(|c| c.id == e.item.id.as_str()) {
            continue;
        }
        tab.thumbs.borrow_mut().insert(e.item.id.clone(), Image::default());
        let (e, tab) = (e.clone(), tab.clone());
        bg(
            move || hub::preview(&e).map(|p| (e.item.id.clone(), p)),
            move |r| {
                let Ok((id, path)) = r else { return };
                let Ok(img) = Image::load_from_path(&path) else { return };
                tab.thumbs.borrow_mut().insert(id.clone(), img.clone());
                if let Some((i, mut c)) = tab.cards.iter().enumerate().find(|(_, c)| c.id == id.as_str()) {
                    c.thumb = img;
                    tab.cards.set_row_data(i, c);
                }
            },
        );
    }
}

fn load(a: &AppRc, tab: &Rc<Tab>) {
    let s = a.s();
    s.set_hb_busy(true);
    s.set_hb_error("".into());
    let (a, tab) = (a.clone(), tab.clone());
    bg(hub::fetch_index, move |r| {
        a.s().set_hb_busy(false);
        match r {
            Ok(ix) => {
                *tab.index.borrow_mut() = Some(ix);
                fill(&a, &tab);
            }
            Err(e) => a.s().set_hb_error(format!("{}: {e:#}", t("Cannot reach the hub")).into()),
        }
    });
}

fn set_busy(a: &AppRc, tab: &Tab, id: &str, busy: bool) {
    // "Update all" changes no single card
    if id.is_empty() {
        a.s().set_hb_busy(busy);
    }
    if let Some((i, mut c)) = tab.cards.iter().enumerate().find(|(_, c)| c.id == id) {
        c.busy = busy;
        tab.cards.set_row_data(i, c);
    }
}

/// Runs a change of the hub folder on a thread, then shows what happened.
fn change(a: &AppRc, tab: &Rc<Tab>, id: String, work: impl FnOnce(&Index) -> anyhow::Result<String> + Send + 'static) {
    let Some(ix) = tab.index.borrow().clone() else { return };
    set_busy(a, tab, &id, true);
    a.s().set_hb_detail_open(false);
    let (a, tab) = (a.clone(), tab.clone());
    bg(
        move || work(&ix),
        move |r| {
            match r {
                Ok(msg) => a.toast(&msg),
                Err(e) => a.error(&e),
            }
            set_busy(&a, &tab, &id, false);
            fill(&a, &tab);
        },
    );
}

fn install(a: &AppRc, tab: &Rc<Tab>, id: String) {
    let code = tab.index.borrow().as_ref().and_then(|ix| ix.with_requirements(&id).ok()).is_some_and(|l| l.iter().any(|e| e.has_code));
    let go = {
        let (a, tab, id) = (a.clone(), tab.clone(), id.clone());
        move || {
            change(&a, &tab, id.clone(), move |ix| {
                let done = hub::install(ix, &id)?;
                Ok(format!("{}: {}", t("Installed"), done.join(", ")))
            })
        }
    };
    if !code {
        return go();
    }
    a.ask(
        &t("Install plugin code?"),
        &t("This brings SourceMod plugins, which run on your server with its permissions. Items are reviewed before they reach the hub, but install only what you trust. They are compiled on this PC."),
        &t("Install"),
        &t("Cancel"),
        false,
        move |yes| {
            if yes {
                go();
            }
        },
    );
}

/// Installs an item from elsewhere in the app (the "no modes" notice), loading the index first if needed.
fn get(a: &AppRc, tab: &Rc<Tab>, id: String) {
    if tab.index.borrow().is_some() {
        return install(a, tab, id);
    }
    a.toast(&t("Installing…"));
    let (a, tab) = (a.clone(), tab.clone());
    bg(hub::fetch_index, move |r| match r {
        Ok(ix) => {
            *tab.index.borrow_mut() = Some(ix);
            tab.loaded.set(true);
            fill(&a, &tab);
            install(&a, &tab, id);
        }
        Err(e) => a.error(&e.context(t("Cannot reach the hub"))),
    });
}

pub fn init(a: &AppRc) {
    let tab = Rc::new(Tab {
        cards: Rc::new(VecModel::default()),
        index: RefCell::default(),
        loaded: Cell::new(false),
        thumbs: RefCell::default(),
    });
    let s = a.s();
    s.set_hb_cards(ModelRc::from(tab.cards.clone()));
    s.set_hb_installed(hub::installed().len() as i32);

    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_shown(move || {
        if !t2.loaded.replace(true) {
            load(&app, &t2);
        }
    });
    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_refresh(move || load(&app, &t2));
    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_filter_changed(move || fill(&app, &t2));
    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_open(move |id| {
        if let Some(c) = t2.cards.iter().find(|c| c.id == id) {
            app.s().set_hb_detail(c);
            app.s().set_hb_detail_open(true);
        }
    });
    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_install(move |id| install(&app, &t2, id.to_string()));
    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_get(move |id| get(&app, &t2, id.to_string()));
    let (app, t2) = (a.clone(), tab.clone());
    s.on_hb_remove(move |id| {
        let (a, tab, id) = (app.clone(), t2.clone(), id.to_string());
        app.ask(
            &format!("{} {id}?", t("Remove")),
            &t("Its modes and plugins leave the server on the next deploy. Your own modes are not touched."),
            &t("Remove"),
            &t("Cancel"),
            true,
            move |yes| {
                if yes {
                    change(&a, &tab, id.clone(), move |_| {
                        hub::remove(&id)?;
                        Ok(format!("{}: {id}", t("Removed")))
                    });
                }
            },
        );
    });
    let (app, t2) = (a.clone(), tab);
    s.on_hb_update_all(move || {
        change(&app, &t2, String::new(), |ix| {
            let done = hub::update_all(ix)?;
            Ok(if done.is_empty() { t("Everything is up to date") } else { format!("{}: {}", t("Updated"), done.join(", ")) })
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters() {
        let mut e = Entry::default();
        e.item.id = "zombies".into();
        e.item.kind = Kind::Mode;
        e.item.name.en = "Zombies".into();
        e.item.tags = vec!["pve".into()];
        assert!(shown(&e, 0, "", false, false));
        assert!(shown(&e, 1, "PVE", false, false));
        assert!(!shown(&e, 2, "", false, false));
        assert!(!shown(&e, 4, "", false, false) && shown(&e, 4, "zomb", true, false));
        assert!(shown(&e, 5, "", true, true) && !shown(&e, 0, "awp", false, false));
    }
}
