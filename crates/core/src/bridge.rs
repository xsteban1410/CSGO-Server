//! UDP bridge for friends over VPN while the server is in LAN mode (formerly puente.py).
//! Server: listens on PORT+1 and forwards to the server from the local IP, so it sees friends as LAN.
//! Friend: listens on their PC and forwards to the server bridge; they connect to their own local IP
//! (Steam rejects with STEAMAUTH code 10 anyone connecting to a 100.x VPN IP).
//!
//! One thread waits on every socket, so a client costs one socket and nothing else, and no lock is shared.
use crate::paths;
use anyhow::{bail, Context};
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

const LISTEN: Token = Token(0);
const NO_ANSWER: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(500);
const STATS_EVERY: Duration = Duration::from_secs(2);
/// A refused address is logged at most this often, so a flood cannot fill the disk.
const LOG_REFUSED_EVERY: Duration = Duration::from_secs(60);

/// An IPv4 network, such as 100.64.0.0/10.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cidr {
    net: u32,
    mask: u32,
}

impl Cidr {
    pub fn parse(s: &str) -> crate::Result<Cidr> {
        let (ip, bits) = s.split_once('/').unwrap_or((s, "32"));
        let ip: Ipv4Addr = ip.parse().with_context(|| format!("invalid network '{s}'"))?;
        let bits: u32 = bits.parse().with_context(|| format!("invalid network '{s}'"))?;
        if bits > 32 {
            bail!("invalid network '{s}'");
        }
        let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
        Ok(Cidr { net: u32::from(ip) & mask, mask })
    }

    fn contains(&self, ip: Ipv4Addr) -> bool {
        u32::from(ip) & self.mask == self.net
    }
}

/// True if `ip` may use the bridge.
fn allowed(allow: &[Cidr], ip: IpAddr) -> bool {
    let v4 = match ip {
        IpAddr::V4(v) => v,
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v) => v,
            None => return false,
        },
    };
    allow.iter().any(|c| c.contains(v4))
}

/// Who may use the server side unless the config says otherwise: the VPN range and this PC.
pub fn default_server_allow() -> Vec<Cidr> {
    ["100.64.0.0/10", "127.0.0.0/8"].iter().filter_map(|s| Cidr::parse(s).ok()).chain(local_networks()).collect()
}

/// Who may use the friend side: only this PC, since the game connects to its own local IP.
pub fn friend_allow() -> Vec<Cidr> {
    std::iter::once("127.0.0.0/8").filter_map(|s| Cidr::parse(s).ok()).chain(local_networks()).collect()
}

fn local_networks() -> Vec<Cidr> {
    crate::net::local_ips().iter().filter_map(|ip| Cidr::parse(ip).ok()).collect()
}

/// Resource limits. A client past them is ignored and logged.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_clients: usize,
    pub max_per_ip: usize,
    /// New clients accepted per second, across all addresses.
    pub new_per_second: u32,
    pub idle: Duration,
    pub allow: Vec<Cidr>,
}

impl Limits {
    pub fn new(max_clients: usize, allow: Vec<Cidr>) -> Limits {
        Limits { max_clients, max_per_ip: 8, new_per_second: 20, idle: Duration::from_secs(120), allow }
    }
}

/// The traffic of one client.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientStats {
    pub addr: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub idle_secs: u64,
    /// The server has answered this client at least once.
    pub answered: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub clients: Vec<ClientStats>,
    pub refused_by_address: u64,
    pub refused_by_limit: u64,
    /// Clients that left, added up so the totals do not fall when one does.
    pub total_bytes_in: u64,
    pub total_bytes_out: u64,
}

/// Where a running bridge publishes its numbers.
pub fn stats_path() -> std::path::PathBuf {
    paths::runtime_dir().join("bridge.json")
}

pub fn read_stats() -> Option<Stats> {
    serde_json::from_str(&std::fs::read_to_string(stats_path()).ok()?).ok()
}

fn write_stats(s: &Stats) {
    let (path, tmp) = (stats_path(), stats_path().with_extension("json.tmp"));
    if let Ok(t) = serde_json::to_string(s) {
        let _ = std::fs::create_dir_all(paths::runtime_dir());
        if std::fs::write(&tmp, t).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

fn is_transient(e: &std::io::Error) -> bool {
    matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset | ErrorKind::Interrupted)
}

struct Client {
    addr: SocketAddr,
    sock: UdpSocket,
    token: Token,
    created: Instant,
    last: Instant,
    warned: bool,
    stats: ClientStats,
}

/// Refuses what would use up the machine: too many clients, too many from one address, or too many at once.
struct Gate {
    window: Instant,
    new_in_window: u32,
}

impl Gate {
    fn admit(&mut self, limits: &Limits, clients: &HashMap<Token, Client>, ip: IpAddr, now: Instant) -> bool {
        if now.duration_since(self.window) >= Duration::from_secs(1) {
            self.window = now;
            self.new_in_window = 0;
        }
        if clients.len() >= limits.max_clients
            || self.new_in_window >= limits.new_per_second
            || clients.values().filter(|c| c.addr.ip() == ip).count() >= limits.max_per_ip
        {
            return false;
        }
        self.new_in_window += 1;
        true
    }
}

/// `listen_port`: where friends arrive; `target`: server (or remote bridge);
/// `source_ip`: IP to forward from (the local one on the server, 0.0.0.0 on the friend).
pub fn run(listen_port: u16, target: SocketAddr, source_ip: IpAddr, limits: Limits, log: impl Fn(String)) -> crate::Result<()> {
    let mut poll = Poll::new()?;
    let mut listen = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), listen_port))?;
    poll.registry().register(&mut listen, LISTEN, Interest::READABLE)?;
    log(format!("Bridge: 0.0.0.0:{listen_port} -> {target} (up to {} clients)", limits.max_clients));

    let mut events = Events::with_capacity(256);
    let mut buf = vec![0u8; 65535];
    let mut clients: HashMap<Token, Client> = HashMap::new();
    let mut by_addr: HashMap<SocketAddr, Token> = HashMap::new();
    let mut next_token = 1;
    let mut gate = Gate { window: Instant::now(), new_in_window: 0 };
    let mut refused_log: HashMap<IpAddr, Instant> = HashMap::new();
    let (mut totals, mut last_stats) = (Stats::default(), Instant::now());

    loop {
        // a signal is not an error, and it does not end the bridge
        if let Err(e) = poll.poll(&mut events, Some(TICK)) {
            if e.kind() != ErrorKind::Interrupted {
                return Err(e.into());
            }
        }
        for ev in events.iter() {
            if ev.token() == LISTEN {
                loop {
                    let (n, from) = match listen.recv_from(&mut buf) {
                        Ok(r) => r,
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        // a port unreachable from an earlier send to a client: nothing to do with the rest
                        Err(e) if is_transient(&e) => continue,
                        Err(e) => return Err(e.into()),
                    };
                    let now = Instant::now();
                    let token = match by_addr.get(&from) {
                        Some(t) => *t,
                        None => {
                            let refused = if !allowed(&limits.allow, from.ip()) {
                                totals.refused_by_address += 1;
                                Some("not in bridge_allow")
                            } else if !gate.admit(&limits, &clients, from.ip(), now) {
                                totals.refused_by_limit += 1;
                                Some("too many clients")
                            } else {
                                None
                            };
                            if let Some(why) = refused {
                                let last = refused_log.get(&from.ip());
                                if last.is_none_or(|t| now.duration_since(*t) >= LOG_REFUSED_EVERY) {
                                    refused_log.insert(from.ip(), now);
                                    log(format!("Bridge: refused {}: {why}", from.ip()));
                                }
                                continue;
                            }
                            let mut sock = match UdpSocket::bind(SocketAddr::new(source_ip, 0)) {
                                Ok(s) => s,
                                Err(e) => {
                                    log(format!("Bridge: cannot open a socket from {source_ip}: {e}"));
                                    continue;
                                }
                            };
                            let token = Token(next_token);
                            next_token += 1;
                            poll.registry().register(&mut sock, token, Interest::READABLE)?;
                            let stats = ClientStats { addr: from.to_string(), ..Default::default() };
                            clients.insert(token, Client { addr: from, sock, token, created: now, last: now, warned: false, stats });
                            by_addr.insert(from, token);
                            log(format!("Bridge: new player {from}"));
                            token
                        }
                    };
                    let Some(c) = clients.get_mut(&token) else { continue };
                    c.last = now;
                    c.stats.packets_in += 1;
                    c.stats.bytes_in += n as u64;
                    // a one-off error (VPN down for a moment) must not bring down the bridge for everyone
                    if let Err(e) = c.sock.send_to(&buf[..n], target) {
                        if e.kind() != ErrorKind::WouldBlock {
                            log(format!("Bridge: cannot send to {target}: {e}"));
                        }
                    }
                }
            } else if let Some(c) = clients.get_mut(&ev.token()) {
                loop {
                    match c.sock.recv_from(&mut buf) {
                        Ok((n, src)) if src == target => {
                            if !c.stats.answered {
                                log(format!("Bridge: {target} answers {}", c.addr));
                            }
                            c.stats.answered = true;
                            c.last = Instant::now();
                            c.stats.packets_out += 1;
                            c.stats.bytes_out += n as u64;
                            let _ = listen.send_to(&buf[..n], c.addr);
                        }
                        Ok(_) => {}
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        // the target's port is closed for now (server restarting): the client may come back
                        Err(e) if is_transient(&e) => continue,
                        Err(_) => break,
                    }
                }
            }
        }

        let now = Instant::now();
        for c in clients.values_mut() {
            if !c.stats.answered && !c.warned && now.duration_since(c.created) >= NO_ANSWER {
                c.warned = true;
                log(format!("Bridge: {target} does not answer {} (server off, VPN down or no VPN profile?)", c.addr));
            }
            c.stats.idle_secs = now.duration_since(c.last).as_secs();
        }
        let gone: Vec<Token> = clients.values().filter(|c| now.duration_since(c.last) >= limits.idle).map(|c| c.token).collect();
        for t in gone {
            if let Some(mut c) = clients.remove(&t) {
                let _ = poll.registry().deregister(&mut c.sock);
                by_addr.remove(&c.addr);
                totals.total_bytes_in += c.stats.bytes_in;
                totals.total_bytes_out += c.stats.bytes_out;
                log(format!("Bridge: {} idle, forgotten", c.addr));
            }
        }
        refused_log.retain(|_, t| now.duration_since(*t) < LOG_REFUSED_EVERY);
        if now.duration_since(last_stats) >= STATS_EVERY {
            last_stats = now;
            let mut s = totals.clone();
            s.clients = clients.values().map(|c| c.stats.clone()).collect();
            s.clients.sort_by(|a, b| a.addr.cmp(&b.addr));
            s.total_bytes_in += s.clients.iter().map(|c| c.bytes_in).sum::<u64>();
            s.total_bytes_out += s.clients.iter().map(|c| c.bytes_out).sum::<u64>();
            write_stats(&s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket as Std;

    fn local() -> Vec<Cidr> {
        vec![Cidr::parse("127.0.0.0/8").unwrap()]
    }

    /// A server that echoes "ok", and a bridge in front of it. Returns the bridge's port.
    fn bridge(limits: Limits) -> u16 {
        let server = Std::bind("127.0.0.1:0").unwrap();
        let target = server.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut b = [0u8; 64];
            while let Ok((_, from)) = server.recv_from(&mut b) {
                let _ = server.send_to(b"ok", from);
            }
        });
        let port = Std::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port();
        // run() logs right after binding: waiting for that line, not for a fixed time, holds on a busy machine
        let (bound, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            run(port, target, "127.0.0.1".parse().unwrap(), limits, move |_| {
                let _ = bound.send(());
            })
        });
        wait.recv_timeout(Duration::from_secs(10)).expect("the bridge could not bind its port");
        port
    }

    fn client(from: &str, port: u16) -> bool {
        let c = Std::bind(format!("{from}:0")).unwrap();
        c.set_read_timeout(Some(Duration::from_millis(400))).unwrap();
        c.send_to(b"hola", ("127.0.0.1", port)).unwrap();
        let mut b = [0u8; 16];
        c.recv_from(&mut b).is_ok_and(|(n, _)| &b[..n] == b"ok")
    }

    #[test]
    fn forwards_both_ways() {
        assert!(client("127.0.0.1", bridge(Limits::new(4, local()))));
    }

    #[test]
    fn keeps_clients_apart_and_sends_each_its_own_replies() {
        let port = bridge(Limits::new(8, local()));
        let (a, b) = (Std::bind("127.0.0.1:0").unwrap(), Std::bind("127.0.0.1:0").unwrap());
        for c in [&a, &b] {
            c.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            c.send_to(b"x", ("127.0.0.1", port)).unwrap();
            let mut buf = [0u8; 8];
            assert!(c.recv_from(&mut buf).is_ok());
        }
    }

    #[test]
    fn refuses_addresses_outside_the_list() {
        let port = bridge(Limits::new(4, vec![Cidr::parse("100.64.0.0/10").unwrap()]));
        assert!(!client("127.0.0.1", port));
    }

    #[test]
    fn holds_to_the_client_limit() {
        let port = bridge(Limits::new(2, local()));
        let (a, b) = (Std::bind("127.0.0.1:0").unwrap(), Std::bind("127.0.0.1:0").unwrap());
        let mut buf = [0u8; 8];
        for c in [&a, &b] {
            c.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            c.send_to(b"x", ("127.0.0.1", port)).unwrap();
            assert!(c.recv_from(&mut buf).is_ok());
        }
        assert!(!client("127.0.0.1", port), "the third has no room");
        // the two in still work
        a.send_to(b"x", ("127.0.0.1", port)).unwrap();
        assert!(a.recv_from(&mut buf).is_ok());
    }

    #[test]
    fn holds_to_the_limit_per_address() {
        let mut l = Limits::new(10, local());
        l.max_per_ip = 1;
        let port = bridge(l);
        let a = Std::bind("127.0.0.1:0").unwrap();
        a.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        a.send_to(b"x", ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 8];
        assert!(a.recv_from(&mut buf).is_ok());
        assert!(!client("127.0.0.1", port));
        assert!(client("127.0.0.2", port), "another address still can");
    }

    #[test]
    fn forgets_idle_clients_and_their_place_comes_free() {
        let mut l = Limits::new(1, local());
        l.idle = Duration::from_millis(200);
        let port = bridge(l);
        assert!(client("127.0.0.1", port));
        std::thread::sleep(Duration::from_millis(1200));
        assert!(client("127.0.0.1", port));
    }

    #[test]
    fn gate_limits_new_clients_per_second() {
        let mut l = Limits::new(100, local());
        l.new_per_second = 2;
        let (now, clients) = (Instant::now(), HashMap::new());
        let mut g = Gate { window: now, new_in_window: 0 };
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(g.admit(&l, &clients, ip, now));
        assert!(g.admit(&l, &clients, ip, now));
        assert!(!g.admit(&l, &clients, ip, now));
        assert!(g.admit(&l, &clients, ip, now + Duration::from_secs(1)), "a new second");
    }

    #[test]
    fn reads_networks() {
        let c = Cidr::parse("100.64.0.0/10").unwrap();
        assert!(c.contains("100.127.1.1".parse().unwrap()));
        assert!(!c.contains("100.128.0.1".parse().unwrap()));
        assert!(Cidr::parse("10.0.0.5").unwrap().contains("10.0.0.5".parse().unwrap()));
        assert!(Cidr::parse("0.0.0.0/0").unwrap().contains("8.8.8.8".parse().unwrap()));
        for bad in ["", "x", "10.0.0.0/33", "10.0.0/8", "::1/128", "10.0.0.0/8; rm"] {
            assert!(Cidr::parse(bad).is_err(), "{bad}");
        }
        assert!(allowed(&[c], "::ffff:100.64.0.1".parse().unwrap()), "mapped IPv4");
        assert!(!allowed(&[c], "::1".parse().unwrap()));
    }
}
