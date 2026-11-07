//! Console tab: live log (console.log from -condebug) and rcon commands.
use crate::i18n::t;
use crate::{bg, AppRc};
use adw::prelude::*;
use csls_core::server::Server;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::io::{Read, Seek, SeekFrom};
use std::rc::Rc;

const TAIL: u64 = 64 * 1024;

pub fn page(a: &AppRc) -> gtk::Widget {
    let view = gtk::TextView::builder().editable(false).monospace(true).wrap_mode(gtk::WrapMode::WordChar).cursor_visible(false).build();
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.set_top_margin(8);
    let scroll = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();
    let entry = gtk::Entry::builder().placeholder_text(t("Send command")).hexpand(true).build();
    entry.set_margin_start(12);
    entry.set_margin_end(12);
    entry.set_margin_top(6);
    entry.set_margin_bottom(12);
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bx.append(&scroll);
    bx.append(&entry);

    let history: Rc<RefCell<Vec<String>>> = Rc::default();
    let hpos = Rc::new(Cell::new(0usize));
    let buf = view.buffer();
    let append = Rc::new(glib::clone!(
        #[weak]
        buf,
        #[weak]
        scroll,
        move |s: &str| {
            let adj = scroll.vadjustment();
            let at_bottom = adj.value() + adj.page_size() >= adj.upper() - 40.0;
            buf.insert(&mut buf.end_iter(), s);
            // do not grow without limit
            if buf.char_count() > 400_000 {
                let mut st = buf.start_iter();
                let mut cut = buf.iter_at_offset(100_000);
                buf.delete(&mut st, &mut cut);
            }
            if at_bottom {
                glib::idle_add_local_once(move || adj.set_value(adj.upper()));
            }
        }
    ));

    entry.connect_activate(glib::clone!(
        #[strong]
        a,
        #[strong]
        append,
        #[strong]
        history,
        #[strong]
        hpos,
        move |e| {
            let cmd = e.text().to_string();
            if cmd.trim().is_empty() {
                return;
            }
            history.borrow_mut().push(cmd.clone());
            hpos.set(history.borrow().len());
            e.set_text("");
            append(&format!("] {cmd}\n"));
            let cfg = a.cfg.borrow().clone();
            let ap = append.clone();
            bg(
                move || Server::new(&cfg).rcon()?.exec(&cmd),
                move |r: anyhow::Result<String>| match r {
                    Ok(o) => ap(&if o.ends_with('\n') || o.is_empty() { o } else { o + "\n" }),
                    Err(e) => ap(&format!("{e:#}\n")),
                },
            );
        }
    ));
    // arrows: history
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        entry,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, k, _, _| {
            let h = history.borrow();
            let i = hpos.get();
            let n = match k {
                gtk::gdk::Key::Up if i > 0 => i - 1,
                gtk::gdk::Key::Down if i < h.len() => i + 1,
                _ => return glib::Propagation::Proceed,
            };
            hpos.set(n);
            entry.set_text(h.get(n).map(String::as_str).unwrap_or(""));
            entry.set_position(-1);
            glib::Propagation::Stop
        }
    ));
    entry.add_controller(keys);

    // follow the log file
    let pos = Rc::new(Cell::new(u64::MAX));
    let in_poll = Rc::new(Cell::new(false));
    glib::timeout_add_local(
        std::time::Duration::from_millis(800),
        glib::clone!(
            #[strong]
            a,
            #[strong]
            append,
            move || {
                let path = Server::new(&a.cfg.borrow()).console_log();
                let Ok(mut f) = std::fs::File::open(&path) else { return glib::ControlFlow::Continue };
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                let mut p = pos.get();
                if p == u64::MAX || len < p {
                    p = len.saturating_sub(TAIL);
                }
                if len > p && f.seek(SeekFrom::Start(p)).is_ok() {
                    let mut b = Vec::new();
                    if f.take(len - p).read_to_end(&mut b).is_ok() {
                        let txt = String::from_utf8_lossy(&b);
                        // the app's polling sends rcon every 3 s: keep it out of the console
                        let mut clean = String::new();
                        for l in txt.split_inclusive('\n') {
                            if l.contains(crate::POLL_BEGIN) {
                                in_poll.set(true);
                            } else if l.contains(crate::POLL_END) {
                                in_poll.set(false);
                            } else if !in_poll.get() && !l.starts_with("rcon from \"") {
                                clean.push_str(l);
                            }
                        }
                        append(&clean);
                    }
                    pos.set(len);
                } else {
                    pos.set(p.max(len.min(p)));
                }
                glib::ControlFlow::Continue
            }
        ),
    );
    bx.upcast()
}
