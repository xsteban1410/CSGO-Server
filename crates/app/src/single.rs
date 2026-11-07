//! One app per user: a second launch shows the window of the first and leaves.
use crate::AppWindow;
use slint::ComponentHandle;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

fn path() -> std::path::PathBuf {
    csls_core::paths::runtime_dir().join("app.sock")
}

/// The socket if this is the first app; `None` if another one answered.
pub fn claim() -> anyhow::Result<Option<UnixListener>> {
    let p = path();
    if let Ok(mut s) = UnixStream::connect(&p) {
        let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
        if s.write_all(b"show").is_ok() {
            return Ok(None);
        }
    }
    std::fs::create_dir_all(csls_core::paths::runtime_dir())?;
    // nobody answered: what is there is left from a crash
    let _ = std::fs::remove_file(&p);
    Ok(Some(UnixListener::bind(&p)?))
}

pub fn serve(listener: UnixListener, ui: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            // the content does not matter: any connection only asks to show the window
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = s.read(&mut [0; 8]);
            let _ = ui.upgrade_in_event_loop(|w| {
                let _ = w.show();
            });
        }
    });
}
