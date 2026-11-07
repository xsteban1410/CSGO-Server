//! Console tab: live log (console.log from -condebug) and rcon commands.
use crate::{bg, AppRc, ConsoleLine};
use csls_core::server::Server;
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

struct Console {
    all: RefCell<Vec<String>>,
    shown: Rc<VecModel<ConsoleLine>>,
    history: RefCell<Vec<String>>,
    hpos: Cell<usize>,
}

fn matches(line: &str, query: &str) -> bool {
    query.is_empty() || line.to_lowercase().contains(query)
}

fn line(text: &str) -> ConsoleLine {
    ConsoleLine { text: text.into(), category: category(text).into() }
}

impl Console {
    fn append(&self, a: &AppRc, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let q = a.s().get_con_search().to_lowercase();
        let mut all = self.all.borrow_mut();
        for l in lines {
            if matches(&l, &q) {
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
        let q = a.s().get_con_search().to_lowercase();
        let v: Vec<ConsoleLine> = self.all.borrow().iter().filter(|l| matches(l, &q)).map(|l| line(l)).collect();
        self.shown.set_vec(v);
    }

    fn visible_text(&self, a: &AppRc) -> String {
        let q = a.s().get_con_search().to_lowercase();
        self.all.borrow().iter().filter(|l| matches(l, &q)).cloned().collect::<Vec<_>>().join("\n")
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
    let c =
        Rc::new(Console { all: RefCell::default(), shown: Rc::new(VecModel::default()), history: RefCell::default(), hpos: Cell::new(0) });
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
        let h = con.history.borrow();
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
        let secret = csls_core::rcon::hides_secret(&cmd);
        if !secret {
            con.history.borrow_mut().push(cmd.clone());
        }
        con.hpos.set(con.history.borrow().len());
        s.set_con_entry("".into());
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
