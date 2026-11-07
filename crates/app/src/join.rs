//! Join tab: open CS:GO Legacy connecting to a server, with a VPN bridge if needed.
use crate::i18n::t;
use crate::{bg, AppRc};
use csls_core::{net, server, steam_client};
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

/// Prepares the connection: with a bridge, starts it and returns the address to connect to.
fn prepare(a: &AppRc) -> anyhow::Result<String> {
    let s = a.s();
    let dest = with_port(&s.get_jn_addr());
    if dest.is_empty() {
        s.set_jn_target("".into());
        return Ok(dest);
    }
    let connect = if s.get_jn_vpn() { server::join_through_bridge(&dest)? } else { dest };
    s.set_jn_bridge(server::bridge_running());
    s.set_jn_target(format!("connect {connect}").into());
    Ok(connect)
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

pub fn init(a: &AppRc) {
    let s = a.s();
    s.set_jn_bridge(server::bridge_running());
    let app = a.clone();
    s.on_jn_addr_changed(move || {
        let text = app.s().get_jn_addr();
        if net::is_vpn_ip(text.trim().split(':').next().unwrap_or("")) {
            app.s().set_jn_vpn(true);
        }
    });
    let app = a.clone();
    s.on_jn_copy(move || {
        let text = app.s().get_jn_target().to_string();
        app.copy(text);
    });
    let app = a.clone();
    s.on_jn_stop_bridge(move || {
        server::stop_bridge();
        app.s().set_jn_bridge(false);
    });
    let app = a.clone();
    s.on_jn_cs2(move || {
        if let Err(e) = steam_client::launch_cs2() {
            app.error(&e);
        }
    });
    let app = a.clone();
    s.on_jn_test(move || match prepare(&app) {
        Ok(c) if !c.is_empty() => check(&app, c),
        Ok(_) => app.toast(&t("Write the server address first")),
        Err(e) => app.error(&e),
    });
    let app = a.clone();
    s.on_jn_play(move || {
        let r = prepare(&app).and_then(|c| {
            steam_client::launch_csgo((!c.is_empty()).then_some(c.as_str())).map(drop)?;
            Ok(c)
        });
        match r {
            Ok(c) if !c.is_empty() => {
                app.copy(format!("connect {c}"));
                app.toast(&t("Opening. If it does not join by itself, paste the copied command in the console"));
                check(&app, c);
            }
            Ok(_) => {}
            Err(e) => app.error(&e),
        }
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
