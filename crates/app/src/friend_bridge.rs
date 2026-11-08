//! Playing on a friend's server over a VPN: a local UDP bridge to it (Server tab), since the game only
//! joins LAN servers that look local.
use crate::i18n::t;
use crate::{bg, AppRc};
use csls_core::{net, server};
use std::time::Duration;

/// "IP" or "IP:PORT", with the default port if absent.
fn with_port(addr: &str) -> String {
    let a = addr.trim();
    if a.is_empty() || a.contains(':') {
        a.to_string()
    } else {
        format!("{a}:27030")
    }
}

/// A2S_INFO through the bridge: checks the whole path without opening the game.
fn check(a: &AppRc, connect: String) {
    let Ok(sock) = connect.parse() else { return };
    let a = a.clone();
    bg(
        move || net::a2s_info(sock, Duration::from_secs(8)),
        move |r: Option<String>| match r {
            Some(name) => a.toast(&format!("{}: {name}", t("The server answers"))),
            None => a.toast(&t("No answer: is the server on, with the VPN profile, and is your VPN connected?")),
        },
    );
}

/// Starts the bridge to the friend's server and shows the address to type in the game.
fn start(a: &AppRc) -> anyhow::Result<Option<String>> {
    let s = a.s();
    let dest = with_port(&s.get_fb_addr());
    if dest.is_empty() {
        return Ok(None);
    }
    let connect = server::join_through_bridge(&dest)?;
    s.set_fb_bridge(server::bridge_running());
    s.set_fb_target(format!("connect {connect}").into());
    Ok(Some(connect))
}

pub fn init(a: &AppRc) {
    let s = a.s();
    s.set_fb_bridge(server::bridge_running());
    let app = a.clone();
    s.on_fb_copy(move || {
        let text = app.s().get_fb_target().to_string();
        app.copy(text);
    });
    let app = a.clone();
    s.on_fb_stop(move || {
        server::stop_bridge();
        app.s().set_fb_bridge(false);
        app.s().set_fb_target("".into());
    });
    let app = a.clone();
    s.on_fb_start(move || match start(&app) {
        Ok(Some(c)) => {
            app.copy(format!("connect {c}"));
            app.toast(&t("Bridge running. Paste the copied command in the game console"));
            check(&app, c);
        }
        Ok(None) => app.toast(&t("Write the server address first")),
        Err(e) => app.error(&e),
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_port() {
        assert_eq!(super::with_port(" 100.1.2.3 "), "100.1.2.3:27030");
        assert_eq!(super::with_port("1.2.3.4:27015"), "1.2.3.4:27015");
        assert_eq!(super::with_port("  "), "");
    }
}
