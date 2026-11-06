//! Workshop tab: search and install CS:GO maps, library, .nav for bots and import local .bsp.
//! CS2 maps show up marked as incompatible.
use crate::i18n::t;
use crate::{bg, button, AppRc};
use adw::prelude::*;
use csls_core::server::Server;
use csls_core::workshop::{self, Compat, Item, Sort};
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const TAGS: &[&str] = &["", "Classic", "Deathmatch", "Armsrace", "Demolition", "Hostage", "Wingman", "Custom"];

fn mb(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1048576.0)
}

/// Installs items (collections get expanded) and notifies.
fn install(a: &AppRc, items: Vec<Item>, done: Rc<dyn Fn()>) {
    let maps = Server::new(&a.cfg.borrow()).maps_dir();
    a.toast(&t("Installing…"));
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
        glib::clone!(#[strong] a, move |res: Vec<Result<String, String>>| {
            let ok: Vec<String> = res.iter().filter_map(|r| r.as_ref().ok().cloned()).collect();
            let err: Vec<String> = res.iter().filter_map(|r| r.as_ref().err().cloned()).collect();
            if !ok.is_empty() {
                a.toast(&format!("{}: {}", t("Installed"), ok.join(", ")));
            }
            if !err.is_empty() {
                a.error(&anyhow::anyhow!(err.join("\n")));
            }
            done();
        }),
    );
}

fn card(a: &AppRc, it: &Item, installed: bool, refresh: Rc<dyn Fn()>) -> gtk::Widget {
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bx.add_css_class("card");
    bx.set_size_request(220, -1);
    let pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).height_request(124).width_request(220).build();
    pic.set_overflow(gtk::Overflow::Hidden);
    bx.append(&pic);
    let it2 = it.clone();
    bg(move || workshop::thumbnail(&it2), glib::clone!(#[weak] pic, move |r| {
        if let Ok(p) = r {
            pic.set_filename(Some(&p));
        }
    }));
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 4);
    inner.set_margin_start(10);
    inner.set_margin_end(10);
    inner.set_margin_bottom(10);
    let title = gtk::Label::builder().label(&it.title).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
    title.add_css_class("heading");
    let (badge, class) = match it.compat() {
        Compat::Csgo => (format!("CS:GO · {}", mb(it.file_size)), "success"),
        Compat::Cs2 => (t("CS2 only — incompatible"), "error"),
        Compat::Unknown if it.collection => (t("Collection"), "accent"),
        Compat::Unknown => (t("Not a map"), "dim-label"),
    };
    let sub = gtk::Label::builder().label(&badge).xalign(0.0).build();
    sub.add_css_class("caption");
    sub.add_css_class(class);
    inner.append(&title);
    inner.append(&sub);
    let can = it.compat() == Compat::Csgo || it.collection;
    let b = button(&if installed { t("Installed") } else { t("Install") }, if can && !installed { &["suggested-action"] } else { &[] });
    b.set_sensitive(can && !installed);
    b.set_margin_top(4);
    let it3 = it.clone();
    b.connect_clicked(glib::clone!(#[strong] a, move |b| {
        b.set_sensitive(false);
        install(&a, vec![it3.clone()], refresh.clone());
    }));
    inner.append(&b);
    bx.append(&inner);
    bx.set_tooltip_text(Some(&format!("{} · {}", it.id, it.tags.join(", "))));
    bx.upcast()
}

pub fn page(a: &AppRc) -> gtk::Widget {
    let page = adw::PreferencesPage::new();

    // --- library ---
    let lib = adw::PreferencesGroup::new();
    lib.set_title(&t("Installed maps"));
    lib.set_description(Some(&t("Drop a .bsp file here to import it. Bots need a .nav: generate it once per map.")));
    let import = button(&t("Import .bsp"), &[]);
    lib.set_header_suffix(Some(&import));
    let lib_list = gtk::ListBox::new();
    lib_list.add_css_class("boxed-list");
    lib_list.set_selection_mode(gtk::SelectionMode::None);
    lib.add(&lib_list);

    // --- search ---
    let sg = adw::PreferencesGroup::new();
    sg.set_title(&t("Search the Workshop"));
    sg.set_description(Some(&t("Only CS:GO maps work. CS2 maps (Source 2) cannot be loaded by CS:GO Legacy.")));
    let search = adw::EntryRow::builder().title(t("Search maps")).show_apply_button(true).build();
    let tag_names: Vec<String> = TAGS.iter().map(|x| if x.is_empty() { t("Any type") } else { x.to_string() }).collect();
    let tag_refs: Vec<&str> = tag_names.iter().map(String::as_str).collect();
    let tag = adw::ComboRow::builder().title(t("Type")).model(&gtk::StringList::new(&tag_refs)).build();
    let sort = adw::ComboRow::builder().title(t("Order")).model(&gtk::StringList::new(&[&t("Trending"), &t("Most popular"), &t("Most recent")])).build();
    let by_id = adw::EntryRow::builder().title(t("Install by ID or link (map or collection)")).show_apply_button(true).build();
    for w in [search.upcast_ref::<gtk::Widget>(), tag.upcast_ref(), sort.upcast_ref(), by_id.upcast_ref()] {
        sg.add(w);
    }
    let flow = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).column_spacing(12).row_spacing(12).homogeneous(true).max_children_per_line(3).min_children_per_line(2).build();
    let more = button(&t("More results"), &["pill"]);
    more.set_halign(gtk::Align::Center);
    more.set_visible(false);
    let spinner = adw::Spinner::new();
    spinner.set_visible(false);
    let rg = adw::PreferencesGroup::new();
    rg.add(&flow);
    rg.add(&spinner);
    rg.add(&more);
    page.add(&lib);
    page.add(&sg);
    page.add(&rg);

    let page_n = Rc::new(Cell::new(1u32));
    let installed_ids: Rc<RefCell<Vec<u64>>> = Rc::default();

    // fill the library
    let fill_lib: crate::LateFn = Rc::default();
    let fl = fill_lib.clone();
    let refill: Rc<dyn Fn()> = Rc::new(glib::clone!(#[strong] a, #[weak] lib_list, #[strong] installed_ids, move || {
        let me = fl.borrow().clone().unwrap();
        let srv_maps = Server::new(&a.cfg.borrow()).maps_dir();
        let list = workshop::installed(&srv_maps);
        *installed_ids.borrow_mut() = list.iter().filter_map(|i| i.dir.parse().ok()).collect();
        lib_list.remove_all();
        if list.is_empty() {
            lib_list.append(&adw::ActionRow::builder().title(t("No workshop maps installed")).build());
        }
        let running = a.live.borrow().rcon_ok;
        for i in list {
            let sub = format!("{} · {}", i.map, if i.has_nav { t("bots ready") } else { t("no .nav for bots") });
            let row = adw::ActionRow::builder().title(glib::markup_escape_text(&i.title)).subtitle(sub).build();
            let mk = |label: &str, class: &[&str]| {
                let b = button(label, class);
                b.add_css_class("flat");
                b.set_valign(gtk::Align::Center);
                row.add_suffix(&b);
                b
            };
            if running {
                let m = i.map.clone();
                mk(&t("Load"), &[]).connect_clicked(glib::clone!(#[strong] a, move |_| {
                    let (cfg, m) = (a.cfg.borrow().clone(), m.clone());
                    bg(move || Server::new(&cfg).rcon()?.exec(&format!("changelevel {m}")), glib::clone!(#[strong] a, move |r| {
                        if let Err(e) = r {
                            a.error(&e);
                        }
                        a.refresh();
                    }));
                }));
                if !i.has_nav {
                    let m = i.map.clone();
                    let me2 = me.clone();
                    mk(&t("Generate .nav"), &[]).connect_clicked(glib::clone!(#[strong] a, move |b| {
                        b.set_sensitive(false);
                        a.toast(&t("Generating .nav (seconds to minutes)…"));
                        let (cfg, m) = (a.cfg.borrow().clone(), m.clone());
                        let me3 = me2.clone();
                        bg(
                            move || -> anyhow::Result<String> {
                                let srv = Server::new(&cfg);
                                let mut r = srv.rcon()?;
                                if Server::current_map(&mut r).as_deref() != Some(m.as_str()) {
                                    r.exec(&format!("changelevel {m}"))?;
                                    drop(r);
                                    std::thread::sleep(Duration::from_secs(8));
                                    srv.wait_rcon(Duration::from_secs(60))?;
                                }
                                srv.generate_nav(Duration::from_secs(600), false)
                            },
                            glib::clone!(#[strong] a, move |r| {
                                match r {
                                    Ok(m) => a.toast(&format!("{}: {m}", t("Bots ready"))),
                                    Err(e) => a.error(&e),
                                }
                                me3();
                            }),
                        );
                    }));
                }
            }
            let dir = i.dir.clone();
            let me2 = me.clone();
            mk(&t("Delete"), &["error"]).connect_clicked(glib::clone!(#[strong] a, move |_| {
                match workshop::remove(&Server::new(&a.cfg.borrow()).maps_dir(), &dir) {
                    Ok(()) => me2(),
                    Err(e) => a.error(&e),
                }
            }));
            lib_list.append(&row);
        }
    }));
    *fill_lib.borrow_mut() = Some(refill.clone());
    refill();
    // the Load / .nav buttons depend on the server answering: only rebuilt when that changes
    let was = Cell::new(a.live.borrow().rcon_ok);
    a.on_live(glib::clone!(#[strong] refill, move |l| {
        if was.replace(l.rcon_ok) != l.rcon_ok {
            refill();
        }
    }));

    // search
    let run_search: Rc<dyn Fn(bool)> = Rc::new(glib::clone!(#[strong] a, #[weak] flow, #[weak] more, #[weak] spinner, #[weak] search, #[weak] tag, #[weak] sort, #[strong] page_n, #[strong] installed_ids, #[strong] refill, move |append: bool| {
        if !append {
            page_n.set(1);
            while let Some(c) = flow.first_child() {
                flow.remove(&c);
            }
        }
        let (text, tg) = (search.text().to_string(), TAGS[tag.selected().min(TAGS.len() as u32 - 1) as usize]);
        let s = [Sort::Trend, Sort::Popular, Sort::Recent][sort.selected().min(2) as usize];
        let p = page_n.get();
        spinner.set_visible(true);
        more.set_visible(false);
        let tg = (!tg.is_empty()).then(|| tg.to_string());
        bg(move || workshop::search(&text, tg.as_deref(), s, p), glib::clone!(#[strong] a, #[strong] installed_ids, #[strong] refill, #[strong] page_n, move |r: anyhow::Result<Vec<Item>>| {
            spinner.set_visible(false);
            match r {
                Ok(items) => {
                    more.set_visible(!items.is_empty());
                    for it in items {
                        let inst = installed_ids.borrow().contains(&it.id);
                        flow.insert(&card(&a, &it, inst, refill.clone()), -1);
                    }
                    page_n.set(p + 1);
                }
                Err(e) => a.error(&e),
            }
        }));
    }));
    search.connect_apply(glib::clone!(#[strong] run_search, move |_| run_search(false)));
    tag.connect_selected_notify(glib::clone!(#[strong] run_search, move |_| run_search(false)));
    sort.connect_selected_notify(glib::clone!(#[strong] run_search, move |_| run_search(false)));
    more.connect_clicked(glib::clone!(#[strong] run_search, move |_| run_search(true)));
    // lazy first load: when the tab is shown
    let loaded = Rc::new(Cell::new(false));
    page.connect_map(glib::clone!(#[strong] run_search, move |_| {
        if !loaded.replace(true) {
            run_search(false);
        }
    }));

    by_id.connect_apply(glib::clone!(#[strong] a, #[strong] refill, move |e| {
        // accepts the number or a ...?id=123 link
        let txt = e.text().to_string();
        let digits: String = txt.rsplit("id=").next().unwrap_or(&txt).chars().take_while(char::is_ascii_digit).collect();
        let Ok(id) = digits.parse::<u64>() else {
            e.add_css_class("error");
            return;
        };
        e.remove_css_class("error");
        bg(move || workshop::details(&[id]), glib::clone!(#[strong] a, #[strong] refill, move |r: anyhow::Result<Vec<Item>>| match r {
            Ok(items) if !items.is_empty() => install(&a, items, refill.clone()),
            Ok(_) => a.error(&anyhow::anyhow!("{} {id}", t("Not found:"))),
            Err(e) => a.error(&e),
        }));
    }));

    // import .bsp (button or drag)
    let import_file = Rc::new(glib::clone!(#[strong] a, #[strong] refill, move |p: std::path::PathBuf| {
        match workshop::import_local(&Server::new(&a.cfg.borrow()).maps_dir(), &p) {
            Ok(m) => {
                a.toast(&format!("{}: {m}", t("Installed")));
                refill();
            }
            Err(e) => a.error(&e),
        }
    }));
    import.connect_clicked(glib::clone!(#[strong] a, #[strong] import_file, move |_| {
        let f = gtk::FileFilter::new();
        f.add_pattern("*.bsp");
        f.set_name(Some("BSP"));
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&f);
        let fd = gtk::FileDialog::builder().title(t("Import .bsp")).filters(&filters).modal(true).build();
        fd.open(Some(&a.win), None::<&gio::Cancellable>, glib::clone!(#[strong] import_file, move |r| {
            if let Some(p) = r.ok().and_then(|f| f.path()) {
                import_file(p);
            }
        }));
    }));
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop(move |_, v, _, _| {
        if let Ok(list) = v.get::<gdk::FileList>() {
            for f in list.files() {
                if let Some(p) = f.path() {
                    import_file(p);
                }
            }
            return true;
        }
        false
    });
    lib.add_controller(drop);
    page.upcast()
}
