//! Networking without external tools (no ip/ss/hostname -I).
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// Local IP used to reach the internet (UDP socket trick: sends nothing).
pub fn local_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:80").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

/// Is it a Tailscale-style VPN IP (100.64.0.0/10)? Steam kicks anyone connecting to one of these.
pub fn is_vpn_ip(ip: &str) -> bool {
    ip.parse::<Ipv4Addr>().is_ok_and(|i| i.octets()[0] == 100 && (64..128).contains(&i.octets()[1]))
}

/// Local IPs from /proc/net/fib_trie: each "|-- IP" followed by "/32 host LOCAL".
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

/// Every IP of this PC.
pub fn local_ips() -> Vec<String> {
    parse_local_ips(&std::fs::read_to_string("/proc/net/fib_trie").unwrap_or_default())
}

/// This PC's VPN IP. shortcut: only recognizes 100.64.0.0/10 (Tailscale, ZeroTier with that range); other VPNs are not detected.
pub fn vpn_ip() -> Option<String> {
    parse_local_ips(&std::fs::read_to_string("/proc/net/fib_trie").ok()?).into_iter().find(|i| is_vpn_ip(i))
}

/// Asks a Source server for its name (A2S_INFO). None if it does not answer.
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
            // challenge: repeat the query with it appended
            [0xFF, 0xFF, 0xFF, 0xFF, 0x41, a, b, c, d] => {
                query.truncate(base);
                query.extend([a, b, c, d]);
            }
            // header + 'I' + protocol + null-terminated name
            [0xFF, 0xFF, 0xFF, 0xFF, 0x49, _, ref rest @ ..] => {
                let name = rest.split(|b| *b == 0).next()?;
                return Some(String::from_utf8_lossy(name).into_owned());
            }
            _ => {}
        }
    }
    None
}

/// Is the UDP port free? (on Linux it fails if anyone holds it on any IP)
pub fn udp_port_free(port: u16) -> bool {
    UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

pub fn hostname() -> Option<String> {
    let h = std::fs::read_to_string("/proc/sys/kernel/hostname").or_else(|_| std::fs::read_to_string("/etc/hostname")).ok()?;
    let h = h.trim();
    (!h.is_empty()).then(|| h.to_string())
}

/// IPv4 of this hostname per /etc/hosts or DNS (srcds binds rcon to this IP, e.g. 127.0.1.1).
pub fn hostname_ip() -> Option<IpAddr> {
    let h = hostname()?;
    (h.as_str(), 0).to_socket_addrs().ok()?.map(|a| a.ip()).find(IpAddr::is_ipv4)
}

/// IPs to try rcon on, in order, without repeats.
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

/// IPs listening on a TCP port, from the text of /proc/net/tcp (little-endian hex).
fn parse_tcp_listeners(table: &str, port: u16) -> Vec<Ipv4Addr> {
    let listening = |line: &str| -> Option<Ipv4Addr> {
        let mut f = line.split_whitespace();
        let (ip, p) = f.nth(1)?.split_once(':')?;
        // state 0A = LISTEN
        (u16::from_str_radix(p, 16).ok()? == port && f.nth(1)? == "0A").then_some(())?;
        Some(Ipv4Addr::from(u32::from_str_radix(ip, 16).ok()?.to_le_bytes()))
    };
    table.lines().skip(1).filter_map(listening).collect()
}

/// IPs a local TCP port listens on that other machines can reach. shortcut: IPv4 only, srcds does not use IPv6.
pub fn tcp_exposed(port: u16) -> Vec<Ipv4Addr> {
    let table = std::fs::read_to_string("/proc/net/tcp").unwrap_or_default();
    parse_tcp_listeners(&table, port).into_iter().filter(|ip| !ip.is_loopback()).collect()
}

/// Public IP (for internet players). Asks api.ipify.org with curl; None without network.
pub fn public_ip() -> Option<IpAddr> {
    let o = std::process::Command::new("curl").args(["-fsS", "--max-time", "5", "https://api.ipify.org"]).output().ok()?;
    String::from_utf8_lossy(&o.stdout).trim().parse().ok()
}

/// Opens the UDP port on the router via UPnP (needs `upnpc`, package miniupnpc).
pub fn upnp_open(port: u16) -> crate::Result<()> {
    upnpc(&["-e", "csgo-legacy-server", "-r", &port.to_string(), "UDP"], "is redirected")
}

/// Closes the forward opened with upnp_open.
pub fn upnp_close(port: u16) -> crate::Result<()> {
    upnpc(&["-d", &port.to_string(), "UDP"], "")
}

fn upnpc(args: &[&str], ok_marker: &str) -> crate::Result<()> {
    let o = std::process::Command::new("upnpc")
        .args(args)
        .output()
        .map_err(|_| anyhow::anyhow!("upnpc (package miniupnpc) is missing, needed to open the port via UPnP"))?;
    let out = String::from_utf8_lossy(&o.stdout);
    if !o.status.success() || !out.contains(ok_marker) || out.contains("No IGD UPnP Device") {
        anyhow::bail!("the router rejected UPnP; open the UDP port by hand: {}", out.lines().last().unwrap_or(""));
    }
    Ok(())
}

/// Active firewall and the command to open the UDP port (game UDP only, not TCP for remote rcon).
pub fn firewall_hint(port: u16) -> Option<String> {
    let active = |unit: &str| {
        std::process::Command::new("systemctl").args(["is-active", "--quiet", unit]).status().map(|s| s.success()).unwrap_or(false)
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
    #[test]
    fn finds_tcp_listeners() {
        let table = "  sl  local_address rem_address   st\n   0: 0101007F:6996 00000000:0000 0A 0:0\n   1: 0C01A8C0:6996 00000000:0000 0A 0:0\n   2: 0C01A8C0:0050 00000000:0000 0A 0:0\n   3: 0C01A8C0:6996 01010101:0050 01 0:0\n";
        let ips = super::parse_tcp_listeners(table, 27030);
        assert_eq!(ips, vec![std::net::Ipv4Addr::new(127, 0, 1, 1), std::net::Ipv4Addr::new(192, 168, 1, 12)]);
    }
    use super::*;
    #[test]
    fn candidates_without_repeats() {
        let v = rcon_candidates("127.0.0.1");
        assert_eq!(v[0], IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(v.iter().filter(|i| **i == v[0]).count(), 1);
    }
    #[test]
    fn vpn_detection() {
        assert!(is_vpn_ip("100.107.0.27"));
        assert!(is_vpn_ip("100.64.0.1"));
        assert!(!is_vpn_ip("100.128.0.1"));
        assert!(!is_vpn_ip("192.168.1.12"));
        assert!(!is_vpn_ip("friend.local"));
        let fib = "Local:\n  +-- 0.0.0.0/0 3 0 5\n     |-- 0.0.0.0\n        /0 universe UNICAST\n     |-- 100.107.0.27\n        /32 host LOCAL\n     |-- 192.168.1.1\n        /32 link UNICAST\n     |-- 192.168.1.12\n        /32 host LOCAL\n";
        assert_eq!(parse_local_ips(fib), ["100.107.0.27", "192.168.1.12"]);
    }
    #[test]
    fn a2s_query_with_challenge() {
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
    fn port_in_use() {
        let s = UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(!udp_port_free(s.local_addr().unwrap().port()));
    }
}
