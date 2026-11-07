//! Statistics tab: what the agent stored, in simple charts, and the controls for keeping or erasing it.
use crate::i18n::t;
use crate::{bars, bg, model, AppRc};
use csls_core::agent;

const RANGES: [u32; 3] = [7, 30, 365];

fn fill(a: &AppRc) {
    let days = RANGES[a.s().get_ss_range().clamp(0, 2) as usize];
    let a = a.clone();
    bg(
        move || agent::summary(days),
        move |r| {
            let s = a.s();
            match r {
                Ok(x) => {
                    s.set_ss_note("".into());
                    s.set_ss_avg(format!("{}: {:.1}", t("Average players"), x.avg_players).into());
                    s.set_ss_days(model(bars(&x.per_day)));
                    s.set_ss_hours(model(bars(&x.per_hour)));
                    s.set_ss_maps(model(bars(&x.maps)));
                    s.set_ss_modes(model(bars(&x.modes)));
                }
                Err(_) => s.set_ss_note(t("No statistics: the agent is not running, or they are off").into()),
            }
        },
    );
}

pub fn init(a: &AppRc) {
    let s = a.s();
    s.set_ss_note(t("No statistics: the agent is not running, or they are off").into());
    {
        let c = a.cfg.borrow();
        s.set_ss_on(c.server.stats);
        s.set_ss_keep(c.server.stats_days as i32);
    }
    fill(a);
    let app = a.clone();
    a.on_agent_changed(move || fill(&app));
    let app = a.clone();
    s.on_ss_refresh(move || fill(&app));
    let app = a.clone();
    s.on_ss_save(move || {
        let mut cfg = app.cfg.borrow().clone();
        cfg.server.stats = app.s().get_ss_on();
        cfg.server.stats_days = app.s().get_ss_keep().max(1) as u32;
        match cfg.save() {
            Ok(()) => {
                *app.cfg.borrow_mut() = cfg;
                crate::restart_agent(&app);
                app.toast(&t("Saved"));
            }
            Err(e) => app.error(&e),
        }
    });
    let app = a.clone();
    s.on_ss_erase(move || {
        let a2 = app.clone();
        app.ask(
            &t("Erase all"),
            &t("This deletes the history of players, maps and modes, and cannot be undone."),
            &t("Erase all"),
            &t("Cancel"),
            true,
            move |yes| {
                if !yes {
                    return;
                }
                let a3 = a2.clone();
                bg(agent::erase_stats, move |r| match r {
                    Ok(()) => {
                        a3.toast(&t("Erased"));
                        a3.agent_changed();
                    }
                    Err(e) => a3.error(&e),
                });
            },
        );
    });
}
