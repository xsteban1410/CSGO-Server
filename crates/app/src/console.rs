//! Console tab: live log (console.log from -condebug) and rcon commands.
use crate::{bg, btn, model, row, strings, AppRc, ConsoleLine};
use csls_core::console_store::{self, Store};
use csls_core::server::Server;
use csls_core::{aliases, rcon};
use slint::{ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::io::{Read, Seek, SeekFrom};
use std::rc::Rc;
use std::time::Duration;

const TAIL: u64 = 64 * 1024;
/// Lines kept. More would only use memory: the file has the rest.
const KEEP: usize = 10_000;

/// Category of a line, for its color. The same rules as the agent's `logs.Classify`.
fn category(l: &str) -> &'static str {
    let low = l.to_lowercase();
    let word = |w: &str| low.split(|c: char| !c.is_alphanumeric()).any(|x| x == w);
    if low.starts_with("] ") {
        "command"
    } else if ["error", "failed", "fatal", "exception", "segmentation"].iter().any(|w| word(w))
        || low.contains("not found")
        || low.contains("unable to")
    {
        "error"
    } else if ["warning", "warn", "deprecated"].iter().any(|w| word(w)) {
        "warning"
    } else if ["[sm]", "[meta]", "[modos]", "[premier]", "[clutch]", "[jefe]"].iter().any(|p| low.starts_with(p)) {
        "plugin"
    } else if ["[all]", "[t]", "[ct]", "*dead*"].iter().any(|p| low.starts_with(p)) || word("say") || word("say_team") {
        "chat"
    } else if ["connected", "disconnect", "dropped", "joined", "entered the game", "kicked", "banned"].iter().any(|p| low.contains(p)) {
        "player"
    } else {
        "server"
    }
}

/// The app's polling sends rcon every few seconds: keep it out of the console.
fn clean(text: &str, in_poll: &Cell<bool>) -> Vec<String> {
    let mut out = Vec::new();
    for l in text.split('\n') {
        if l.contains(crate::POLL_BEGIN) {
            in_poll.set(true);
        } else if l.contains(crate::POLL_END) {
            in_poll.set(false);
        } else if !in_poll.get() && !l.starts_with("rcon from \"") && !l.trim().is_empty() {
            out.push(l.trim_end().to_string());
        }
    }
    out
}

fn read_new(path: &std::path::Path, pos: u64) -> (u64, String) {
    let Ok(mut f) = std::fs::File::open(path) else { return (pos, String::new()) };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut p = pos;
    if p == u64::MAX || len < p {
        p = len.saturating_sub(TAIL);
    }
    let mut b = Vec::new();
    if len > p && f.seek(SeekFrom::Start(p)).is_ok() && f.take(len - p).read_to_end(&mut b).is_ok() {
        return (len, String::from_utf8_lossy(&b).into_owned());
    }
    (p, String::new())
}

/// Category chosen in the combo, in its order.
const FILTERS: [&str; 7] = ["", "error", "warning", "plugin", "player", "chat", "command"];

struct Console {
    store: RefCell<Store>,
    cvars: RefCell<Vec<String>>,
    all: RefCell<Vec<String>>,
    shown: Rc<VecModel<ConsoleLine>>,
    hpos: Cell<usize>,
}

fn matches(line: &str, query: &str, filter: &str) -> bool {
    (query.is_empty() || line.to_lowercase().contains(query)) && (filter.is_empty() || category(line) == filter)
}

fn wanted(a: &AppRc) -> (String, &'static str) {
    let s = a.s();
    (s.get_con_search().to_lowercase(), FILTERS[s.get_con_category().clamp(0, 6) as usize])
}

fn fill_aliases(a: &AppRc) {
    let del = crate::t("Delete");
    let rows = aliases::load().unwrap_or_default().iter().map(|(k, v)| row(k, k, v, vec![btn("del", &del)])).collect();
    a.s().set_con_aliases(model(rows));
}

fn fill_favorites(a: &AppRc, store: &Store) {
    a.s().set_con_favorites(strings(&store.favorites));
}

/// The list of cvars of this game build, asked for once and kept.
fn load_cvars(a: &AppRc, c: &Rc<Console>) {
    let cfg = a.cfg.borrow().clone();
    let c = c.clone();
    bg(
        move || -> anyhow::Result<Vec<String>> {
            let srv = Server::new(&cfg);
            let build = console_store::game_build(&srv.csgo_dir()).unwrap_or_default();
            if let Some(v) = console_store::cached_cvars(&build) {
                return Ok(v);
            }
            let names = console_store::parse_cvarlist(&srv.rcon()?.exec("cvarlist")?);
            console_store::cache_cvars(&build, &names)?;
            Ok(names)
        },
        move |r| {
            if let Ok(v) = r {
                *c.cvars.borrow_mut() = v;
            }
        },
    );
}

fn line(text: &str) -> ConsoleLine {
    ConsoleLine { text: text.into(), category: category(text).into() }
}

impl Console {
    fn append(&self, a: &AppRc, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let (q, f) = wanted(a);
        let mut all = self.all.borrow_mut();
        for l in lines {
            if matches(&l, &q, f) {
                self.shown.push(line(&l));
            }
            all.push(l);
        }
        let extra = all.len().saturating_sub(KEEP);
        if extra > 0 {
            all.drain(..extra);
            // over the limit, the view is simply rebuilt: this happens rarely
            drop(all);
            self.refilter(a);
        } else {
            drop(all);
        }
        self.publish(a);
    }

    fn refilter(&self, a: &AppRc) {
        let (q, f) = wanted(a);
        let v: Vec<ConsoleLine> = self.all.borrow().iter().filter(|l| matches(l, &q, f)).map(|l| line(l)).collect();
        self.shown.set_vec(v);
    }

    fn visible_text(&self, a: &AppRc) -> String {
        let (q, f) = wanted(a);
        self.all.borrow().iter().filter(|l| matches(l, &q, f)).cloned().collect::<Vec<_>>().join("\n")
    }

    fn publish(&self, a: &AppRc) {
        let s = a.s();
        if s.get_con_select() {
            s.set_con_text(self.visible_text(a).into());
        }
        s.set_con_scroll_tick(s.get_con_scroll_tick() + 1);
    }
}

pub fn init(a: &AppRc) {
    let store = Store::load();
    let c = Rc::new(Console {
        hpos: Cell::new(store.history.len()),
        store: RefCell::new(store),
        cvars: RefCell::default(),
        all: RefCell::default(),
        shown: Rc::new(VecModel::default()),
    });
    let s = a.s();
    s.set_con_lines(ModelRc::from(c.shown.clone()));

    let (app, con) = (a.clone(), c.clone());
    s.on_con_filter_changed(move || {
        con.refilter(&app);
        con.publish(&app);
    });
    let (app, con) = (a.clone(), c.clone());
    s.on_con_copy(move || {
        let text = con.visible_text(&app);
        app.copy(text);
    });
    let (app, con) = (a.clone(), c.clone());
    s.on_con_history(move |delta| {
        let store = con.store.borrow();
        let h = &store.history;
        let i = con.hpos.get();
        let n = match delta {
            d if d < 0 && i > 0 => i - 1,
            d if d > 0 && i < h.len() => i + 1,
            _ => return,
        };
        con.hpos.set(n);
        app.s().set_con_entry(h.get(n).map(String::as_str).unwrap_or("").into());
    });
    let (app, con) = (a.clone(), c.clone());
    s.on_con_send(move || {
        let s = app.s();
        let cmd = s.get_con_entry().to_string();
        if cmd.trim().is_empty() {
            return;
        }
        let secret = rcon::hides_secret(&cmd);
        {
            let mut store = con.store.borrow_mut();
            store.remember(&cmd);
            con.hpos.set(store.history.len());
            if let Err(e) = store.save() {
                eprintln!("console history: {e:#}");
            }
        }
        s.set_con_entry("".into());
        s.set_con_suggest(strings(&[]));
        let cmd = aliases::expand(&aliases::load().unwrap_or_default(), &cmd);
        con.append(&app, vec![if secret { format!("] {} ***", cmd.split_whitespace().next().unwrap_or("")) } else { format!("] {cmd}") }]);
        let cfg = app.cfg.borrow().clone();
        let (a2, con2) = (app.clone(), con.clone());
        bg(
            move || Server::new(&cfg).rcon()?.exec(&cmd),
            move |r: anyhow::Result<String>| {
                let text = match r {
                    Ok(o) => o,
                    Err(e) => format!("{e:#}"),
                };
                con2.append(&a2, text.lines().map(str::to_string).collect());
            },
        );
    });

    let (app, con) = (a.clone(), c.clone());
    s.on_con_entry_edited(move || {
        let typed = app.s().get_con_entry();
        let names = con.cvars.borrow();
        let list: Vec<String> = console_store::complete(&names, &typed, 6).into_iter().map(str::to_string).collect();
        app.s().set_con_suggest(strings(&list));
    });
    let app = a.clone();
    s.on_con_use(move |text| {
        app.s().set_con_entry(text);
        app.s().set_con_suggest(strings(&[]));
    });
    let (app, con) = (a.clone(), c.clone());
    s.on_con_favorite(move || {
        let cmd = app.s().get_con_entry().trim().to_string();
        let mut store = con.store.borrow_mut();
        store.toggle_favorite(&cmd);
        fill_favorites(&app, &store);
        if let Err(e) = store.save() {
            app.error(&e);
        }
    });
    let (app, con) = (a.clone(), c.clone());
    s.on_con_download(move || {
        let text = con.visible_text(&app);
        crate::save_as(&app, "console.log".to_string(), text);
    });
    let app = a.clone();
    s.on_con_alias_add(move || {
        let s = app.s();
        match aliases::set(s.get_con_alias_name().trim(), &s.get_con_alias_cmd()) {
            Ok(()) => {
                s.set_con_alias_name("".into());
                s.set_con_alias_cmd("".into());
                fill_aliases(&app);
            }
            Err(e) => app.error(&e),
        }
    });
    let app = a.clone();
    s.on_con_alias_action(move |id, _| match aliases::remove(&id) {
        Ok(()) => fill_aliases(&app),
        Err(e) => app.error(&e),
    });
    fill_aliases(a);
    fill_favorites(a, &c.store.borrow());
    // the list needs a server that answers
    let (app, con, was) = (a.clone(), c.clone(), Cell::new(false));
    a.on_live(move |l| {
        if !was.replace(l.rcon_ok) && l.rcon_ok {
            load_cvars(&app, &con);
        }
    });

    // follow the log file, off the UI thread
    let pos = Rc::new(Cell::new(u64::MAX));
    let in_poll = Rc::new(Cell::new(false));
    let busy = Rc::new(Cell::new(false));
    let (app, con) = (a.clone(), c);
    a.every(Duration::from_millis(800), move || {
        if busy.replace(true) {
            return;
        }
        let path = Server::new(&app.cfg.borrow()).console_log();
        let p0 = pos.get();
        let (app2, con2, pos2, poll2, busy2) = (app.clone(), con.clone(), pos.clone(), in_poll.clone(), busy.clone());
        bg(
            move || read_new(&path, p0),
            move |(p, text)| {
                busy2.set(false);
                pos2.set(p);
                con2.append(&app2, clean(&text, &poll2));
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_by_text_and_category() {
        assert!(matches("Failed to load", "", ""));
        assert!(matches("Failed to load", "load", "error"));
        assert!(!matches("Failed to load", "nope", ""));
        assert!(!matches("Failed to load", "", "chat"));
    }

    #[test]
    fn classifies_lines() {
        for (l, want) in [
            ("[SM] Exception reported: oops", "error"),
            ("Failed to load plugin x", "error"),
            ("Warning: something old", "warning"),
            ("[SM] Loaded plugin modos", "plugin"),
            ("Luis connected", "player"),
            ("[ALL] Luis: hola", "chat"),
            ("] status", "command"),
            ("Executing dedicated server cfg.", "server"),
        ] {
            assert_eq!(category(l), want, "{l}");
        }
    }

    #[test]
    fn hides_the_apps_own_polling() {
        let p = Cell::new(false);
        let txt = "one\ncsls_poll_begin\nstatus output\ncsls_poll_end\nrcon from \"1.1.1.1\": x\n\ntwo\n";
        assert_eq!(clean(txt, &p), ["one", "two"]);
        assert!(!p.get());
    }

    #[test]
    fn reads_only_what_is_new() {
        let path = std::env::temp_dir().join(format!("csls-con-{}", std::process::id()));
        std::fs::write(&path, "a\n").unwrap();
        let (p, t) = read_new(&path, u64::MAX);
        assert_eq!(t, "a\n");
        std::fs::write(&path, "a\nb\n").unwrap();
        let (p, t) = read_new(&path, p);
        assert_eq!(t, "b\n");
        std::fs::write(&path, "x\n").unwrap();
        let (_, t) = read_new(&path, p);
        assert_eq!(t, "x\n", "a shorter file was rotated");
        std::fs::remove_file(path).unwrap();
    }
}
