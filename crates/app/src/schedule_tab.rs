//! Tasks tab: scheduled work. The agent checks and runs it; this only edits the list.
use crate::i18n::t;
use crate::{bg, btn, model, row, AppRc};
use csls_core::agent::{self, Task};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const ACTIONS: [&str; 6] = ["restart", "backup", "plugins", "map", "mode", "rcon"];
const LABELS: [&str; 6] = ["Restart the server", "Make a backup", "Update plugins", "Change map", "Change mode", "RCON command"];
const EVERY: [&str; 3] = ["daily", "weekly", "hours"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn describe(x: &Task) -> String {
    let when = match x.every.as_str() {
        "daily" => format!("{} {}", t("Every day"), x.at),
        "weekly" => format!("{} {}", t(DAYS[usize::from(x.weekday).min(6)]), x.at),
        _ => format!("{} h", x.hours),
    };
    let state = if x.enabled { t("On") } else { t("Off") };
    if x.arg.is_empty() {
        format!("{when} · {state}")
    } else {
        format!("{} · {when} · {state}", x.arg)
    }
}

fn label(action: &str) -> String {
    ACTIONS.iter().position(|a| *a == action).map_or_else(|| action.to_string(), |i| t(LABELS[i]))
}

fn fill(a: &AppRc, tasks: &Rc<RefCell<Vec<Task>>>) {
    let (a, tasks) = (a.clone(), tasks.clone());
    bg(
        || agent::tasks().and_then(|l| Ok((l, agent::task_history()?))),
        move |r| {
            let s = a.s();
            match r {
                Ok((list, history)) => {
                    s.set_sc_note("".into());
                    let rows = list
                        .iter()
                        .map(|x| {
                            let toggle = if x.enabled { btn("off", &t("Turn off")) } else { btn("on", &t("Turn on")) };
                            let mut del = btn("del", &t("Delete"));
                            del.danger = true;
                            row(&x.id, &label(&x.action), &describe(x), vec![btn("run", &t("Run now")), toggle, del])
                        })
                        .collect();
                    s.set_sc_rows(model(rows));
                    let runs = history.iter().rev().take(20).map(|h| {
                        let mark = if h.ok { t("OK") } else { t("Failed") };
                        row("", &format!("{} · {mark}", h.id), &format!("{} · {}", crate::when(h.start), h.output.trim()), vec![])
                    });
                    s.set_sc_history(model(runs.collect()));
                    *tasks.borrow_mut() = list;
                }
                Err(_) => s.set_sc_note(t("Tasks need the agent, and it is not running").into()),
            }
        },
    );
}

/// The whole list goes to the agent, which refuses it if something is wrong.
fn store(a: &AppRc, tasks: &Rc<RefCell<Vec<Task>>>, list: Vec<Task>) {
    let (a, t2) = (a.clone(), tasks.clone());
    bg(
        move || agent::set_tasks(&list),
        move |r| {
            if let Err(e) = r {
                a.error(&e);
            }
            fill(&a, &t2);
        },
    );
}

/// A task from the form. It only reads the fields: the agent has the last word.
fn from_form(kind: i32, arg: &str, every: i32, at: &str, weekday: i32, hours: i32, id: String) -> Task {
    let kind = kind.clamp(0, 5) as usize;
    let every = EVERY[every.clamp(0, 2) as usize];
    Task {
        id,
        action: ACTIONS[kind].into(),
        arg: if kind >= 3 { arg.trim().to_string() } else { String::new() },
        every: every.into(),
        at: if every == "hours" { String::new() } else { at.trim().to_string() },
        weekday: if every == "weekly" { weekday.clamp(0, 6) as u8 } else { 0 },
        hours: if every == "hours" { hours.max(1) as u32 } else { 0 },
        enabled: true,
    }
}

pub fn init(a: &AppRc) {
    let tasks: Rc<RefCell<Vec<Task>>> = Rc::default();
    let s = a.s();
    s.set_sc_note(t("Tasks need the agent, and it is not running").into());
    fill(a, &tasks);
    let (app, tk) = (a.clone(), tasks.clone());
    a.on_agent_changed(move || fill(&app, &tk));
    let (app, tk) = (a.clone(), tasks.clone());
    s.on_sc_add(move || {
        let s = app.s();
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let task = from_form(
            s.get_sc_kind(),
            &s.get_sc_arg(),
            s.get_sc_every(),
            &s.get_sc_at(),
            s.get_sc_weekday(),
            s.get_sc_hours(),
            format!("task-{stamp}"),
        );
        let mut list = tk.borrow().clone();
        list.push(task);
        store(&app, &tk, list);
    });
    let (app, tk) = (a.clone(), tasks);
    s.on_sc_action(move |id, action| {
        let mut list = tk.borrow().clone();
        match action.as_str() {
            "del" => list.retain(|x| x.id != id.as_str()),
            "on" | "off" => {
                if let Some(x) = list.iter_mut().find(|x| x.id == id.as_str()) {
                    x.enabled = action == "on";
                }
            }
            _ => {
                let (a2, tk2, id) = (app.clone(), tk.clone(), id.to_string());
                bg(
                    move || agent::run_task(&id),
                    move |r| {
                        match r {
                            Ok(()) => a2.toast(&t("Started")),
                            Err(e) => a2.error(&e),
                        }
                        fill(&a2, &tk2);
                    },
                );
                return;
            }
        }
        store(&app, &tk, list);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_fills_only_what_applies() {
        let x = from_form(0, "ignored", 0, " 05:00 ", 3, 9, "a".into());
        assert_eq!(
            (x.action.as_str(), x.arg.as_str(), x.every.as_str(), x.at.as_str(), x.weekday, x.hours),
            ("restart", "", "daily", "05:00", 0, 0)
        );
        let x = from_form(5, " status ", 2, "05:00", 3, 9, "a".into());
        assert_eq!((x.action.as_str(), x.arg.as_str(), x.every.as_str(), x.at.as_str(), x.hours), ("rcon", "status", "hours", "", 9));
        let x = from_form(3, "de_nuke", 1, "20:30", 6, 9, "a".into());
        assert_eq!((x.weekday, x.at.as_str()), (6, "20:30"));
    }
}
