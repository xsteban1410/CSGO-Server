//! Backups tab: list, create, restore after seeing what changes, export and retention.
use crate::i18n::t;
use crate::{bg, btn, model, row, AppRc};
use csls_core::backup;
use std::path::PathBuf;

fn file_of(id: &str) -> Option<PathBuf> {
    // only what the list says: the id comes back from the row
    backup::list().ok()?.into_iter().find(|p| p.to_string_lossy() == id)
}

fn fill(a: &AppRc) {
    let list = backup::list().unwrap_or_default();
    let rows = list
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let mut restore = btn("restore", &t("Restore"));
            restore.danger = true;
            row(&p.to_string_lossy(), &name, &format!("{} KB", size / 1024), vec![btn("export", &t("Export")), restore])
        })
        .collect();
    a.s().set_bk_rows(model(rows));
}

fn restore(a: &AppRc, file: PathBuf) {
    let cfg = a.cfg.borrow().clone();
    let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let f = file.clone();
    let a = a.clone();
    bg(
        move || backup::preview(&cfg, &f),
        move |r: anyhow::Result<Vec<backup::Change>>| match r {
            Ok(c) if c.is_empty() => a.toast(&t("Nothing to restore: the files are the same")),
            Ok(c) => {
                let names: Vec<String> = c.iter().take(8).map(|x| x.path.display().to_string()).collect();
                let more = if c.len() > 8 { "\n…" } else { "" };
                let text = format!("{name}\n{}: {}\n{}{more}", t("Files that will change"), c.len(), names.join("\n"));
                let app = a.clone();
                a.ask(&t("Restore"), &text, &t("Restore"), &t("Cancel"), true, move |yes| {
                    if !yes {
                        return;
                    }
                    let cfg = app.cfg.borrow().clone();
                    let a = app.clone();
                    bg(
                        move || backup::restore(&cfg, &file),
                        move |r: anyhow::Result<Vec<backup::Change>>| match r {
                            Ok(_) => a.toast(&t("Backup restored; restart the app")),
                            Err(e) => a.error(&e),
                        },
                    );
                });
            }
            Err(e) => a.error(&e),
        },
    );
}

fn export(a: &AppRc, file: PathBuf) {
    let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let a = a.clone();
    bg(
        move || -> anyhow::Result<bool> {
            let Some(to) = rfd::FileDialog::new().set_title(t("Export")).set_file_name(name).save_file() else { return Ok(false) };
            std::fs::copy(file, to)?;
            Ok(true)
        },
        move |r| match r {
            Ok(true) => a.toast(&t("Saved")),
            Ok(false) => {}
            Err(e) => a.error(&e),
        },
    );
}

pub fn init(a: &AppRc) {
    fill(a);
    let s = a.s();
    s.set_bk_keep(a.cfg.borrow().server.backup_keep as i32);
    let app = a.clone();
    s.on_bk_create(move || {
        let cfg = app.cfg.borrow().clone();
        let a = app.clone();
        bg(
            move || backup::create(&cfg),
            move |r: anyhow::Result<PathBuf>| match r {
                Ok(p) => {
                    a.toast(&format!("{}: {}", t("Backup saved"), p.file_name().unwrap_or_default().to_string_lossy()));
                    fill(&a);
                }
                Err(e) => a.error(&e),
            },
        );
    });
    let app = a.clone();
    s.on_bk_action(move |id, action| {
        let Some(file) = file_of(&id) else { return };
        match action.as_str() {
            "restore" => restore(&app, file),
            _ => export(&app, file),
        }
    });
    let app = a.clone();
    s.on_bk_keep_changed(move || {
        let mut cfg = app.cfg.borrow().clone();
        cfg.server.backup_keep = app.s().get_bk_keep().max(1) as u32;
        match cfg.save() {
            Ok(()) => *app.cfg.borrow_mut() = cfg,
            Err(e) => app.error(&e),
        }
    });
}
