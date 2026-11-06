//! Red sin herramientas externas (sin ip/ss/hostname -I).
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// IP local con la que se sale a internet (truco del socket UDP: no envía nada).
pub fn local_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:80").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

/// ¿Es una IP de VPN tipo Tailscale (100.64.0.0/10)? Steam echa a quien conecta a una de estas.
pub fn is_vpn_ip(ip: &str) -> bool {
    ip.parse::<Ipv4Addr>().is_ok_and(|i| i.octets()[0] == 100 && (64..128).contains(&i.octets()[1]))
}

/// IPs locales de /proc/net/fib_trie: cada "|-- IP" seguida de "/32 host LOCAL".
fn parse_local_ips(fib: &str) -> Vec<String> {
    let lines: Vec<&str> = fib.lines().collect();
    let mut v = Vec::new();
    for w in lines.windows(2) {
        if let Some(ip) = w[0].trim_start().strip_prefix("|-- ") {
            if w[1].contains("/32 host LOCAL") && !v.iter().any(|x| x == ip) {
                v.push(ip.to_string());
            }
        }
    }
    v
}

/// IP de este PC en la VPN. shortcut: solo reconoce 100.64.0.0/10 (Tailscale, ZeroTier con esa red); otras VPN no se detectan.
pub fn vpn_ip() -> Option<String> {
    parse_local_ips(&std::fs::read_to_string("/proc/net/fib_trie").ok()?).into_iter().find(|i| is_vpn_ip(i))
}

/// Pregunta a un servidor Source su nombre (A2S_INFO). None si no contesta.
pub fn a2s_info(addr: SocketAddr, timeout: Duration) -> Option<String> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.set_read_timeout(Some(Duration::from_millis(800))).ok()?;
    let mut query = b"\xFF\xFF\xFF\xFFTSource Engine Query\0".to_vec();
    let base = query.len();
    let t = Instant::now();
    let mut buf = [0u8; 1500];
    while t.elapsed() < timeout {
        s.send_to(&query, addr).ok();
        let Ok((n, _)) = s.recv_from(&mut buf) else { continue };
        match buf[..n] {
            // reto: repetir la consulta con él al final
            [0xFF, 0xFF, 0xFF, 0xFF, 0x41, a, b, c, d] => {
                query.truncate(base);
                query.extend([a, b, c, d]);
            }
            // cabecera + 'I' + protocolo + nombre terminado en 0
            [0xFF, 0xFF, 0xFF, 0xFF, 0x49, _, ref rest @ ..] => {
                let name = rest.split(|b| *b == 0).next()?;
                return Some(String::from_utf8_lossy(name).into_owned());
            }
            _ => {}
        }
    }
    None
}

/// ¿Está libre el puerto UDP? (en Linux falla si alguien lo tiene en cualquier IP)
pub fn udp_port_free(port: u16) -> bool {
    UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

pub fn hostname() -> Option<String> {
    let h = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .ok()?;
    let h = h.trim();
    (!h.is_empty()).then(|| h.to_string())
}

/// IPv4 de este nombre de host según /etc/hosts o DNS (srcds ata rcon a esta IP, p. ej. 127.0.1.1).
pub fn hostname_ip() -> Option<IpAddr> {
    let h = hostname()?;
    (h.as_str(), 0).to_socket_addrs().ok()?.map(|a| a.ip()).find(IpAddr::is_ipv4)
}

/// IPs donde probar rcon, por orden y sin repetir.
pub fn rcon_candidates(configured: &str) -> Vec<IpAddr> {
    let mut v: Vec<IpAddr> = Vec::new();
    if let Ok(ip) = configured.parse() {
        v.push(ip);
    }
    v.extend(hostname_ip());
    v.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    v.extend(local_ip());
    let mut out = Vec::new();
    for ip in v {
        if !out.contains(&ip) {
            out.push(ip);
        }
    }
    out
}

/// IP pública (para jugadores por internet). Pregunta a api.ipify.org con curl; None sin red.
pub fn public_ip() -> Option<IpAddr> {
    let o = std::process::Command::new("curl").args(["-fsS", "--max-time", "5", "https://api.ipify.org"]).output().ok()?;
    String::from_utf8_lossy(&o.stdout).trim().parse().ok()
}

/// Abre el puerto UDP en el router por UPnP (necesita `upnpc`, paquete miniupnpc).
pub fn upnp_open(port: u16) -> crate::Result<()> {
    upnpc(&["-e", "csgo-legacy-server", "-r", &port.to_string(), "UDP"], "is redirected")
}

/// Cierra la redirección abierta con upnp_open.
pub fn upnp_close(port: u16) -> crate::Result<()> {
    upnpc(&["-d", &port.to_string(), "UDP"], "")
}

fn upnpc(args: &[&str], ok_marker: &str) -> crate::Result<()> {
    let o = std::process::Command::new("upnpc")
        .args(args)
        .output()
        .map_err(|_| anyhow::anyhow!("falta upnpc (paquete miniupnpc) para abrir el puerto por UPnP"))?;
    let out = String::from_utf8_lossy(&o.stdout);
    if !o.status.success() || !out.contains(ok_marker) || out.contains("No IGD UPnP Device") {
        anyhow::bail!("el router no aceptó UPnP; abre el puerto UDP a mano: {}", out.lines().last().unwrap_or(""));
    }
    Ok(())
}

/// Cortafuegos activo y comando para abrir el puerto UDP (y TCP para rcon remoto no: solo UDP del juego).
pub fn firewall_hint(port: u16) -> Option<String> {
    let active = |unit: &str| {
        std::process::Command::new("systemctl")
            .args(["is-active", "--quiet", unit])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if active("ufw") {
        Some(format!("sudo ufw allow {port}/udp"))
    } else if active("firewalld") {
        Some(format!("sudo firewall-cmd --permanent --add-port={port}/udp && sudo firewall-cmd --reload"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidatos_sin_repetir() {
        let v = rcon_candidates("127.0.0.1");
        assert_eq!(v[0], IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(v.iter().filter(|i| **i == v[0]).count(), 1);
    }
    #[test]
    fn deteccion_de_vpn() {
        assert!(is_vpn_ip("100.107.0.27"));
        assert!(is_vpn_ip("100.64.0.1"));
        assert!(!is_vpn_ip("100.128.0.1"));
        assert!(!is_vpn_ip("192.168.1.12"));
        assert!(!is_vpn_ip("amigo.local"));
        let fib = "Local:\n  +-- 0.0.0.0/0 3 0 5\n     |-- 0.0.0.0\n        /0 universe UNICAST\n     |-- 100.107.0.27\n        /32 host LOCAL\n     |-- 192.168.1.1\n        /32 link UNICAST\n     |-- 192.168.1.12\n        /32 host LOCAL\n";
        assert_eq!(parse_local_ips(fib), ["100.107.0.27", "192.168.1.12"]);
    }
    #[test]
    fn consulta_a2s_con_reto() {
        let srv = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = srv.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut b = [0u8; 64];
            let (n, from) = srv.recv_from(&mut b).unwrap();
            assert_eq!(n, 25);
            srv.send_to(b"\xFF\xFF\xFF\xFF\x41ABCD", from).unwrap();
            let (n, from) = srv.recv_from(&mut b).unwrap();
            assert_eq!(&b[n - 4..n], b"ABCD");
            srv.send_to(b"\xFF\xFF\xFF\xFF\x49\x11CS:GO Legacy\0de_dust2\0", from).unwrap();
        });
        assert_eq!(a2s_info(addr, Duration::from_secs(5)).as_deref(), Some("CS:GO Legacy"));
    }
    #[test]
    fn puerto() {
        let s = UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(!udp_port_free(s.local_addr().unwrap().port()));
    }
}
