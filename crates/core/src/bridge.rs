//! UDP bridge for friends over VPN while the server is in LAN mode (formerly puente.py).
//! Server: listens on PORT+1 and forwards to the server from the local IP, so it sees friends as LAN.
//! Friend: listens on their PC and forwards to the server bridge; they connect to their own local IP
//! (Steam rejects with STEAMAUTH code 10 anyone connecting to a 100.x VPN IP).
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(120);
const NO_ANSWER: Duration = Duration::from_secs(5);

type Clients = Arc<Mutex<HashMap<SocketAddr, (Arc<UdpSocket>, Instant)>>>;

/// `listen_port`: where friends arrive; `target`: server (or remote bridge);
/// `source_ip`: IP to forward from (the local one on the server, 0.0.0.0 on the friend).
pub fn run(listen_port: u16, target: SocketAddr, source_ip: IpAddr, log: impl Fn(String) + Send + Sync + 'static) -> crate::Result<()> {
    let listen = Arc::new(UdpSocket::bind(("0.0.0.0", listen_port))?);
    let log = Arc::new(log);
    log(format!("Puente: 0.0.0.0:{listen_port} -> {target}"));
    let clients: Clients = Arc::default();
    let mut buf = vec![0u8; 65535];
    loop {
        let (n, from) = listen.recv_from(&mut buf)?;
        let mut map = clients.lock().unwrap();
        map.retain(|_, (_, t)| t.elapsed() < IDLE);
        let sock = match map.get_mut(&from) {
            Some((s, t)) => {
                *t = Instant::now();
                s.clone()
            }
            None => {
                let s = match UdpSocket::bind((source_ip, 0)) {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        log(format!("Bridge: cannot open a socket from {source_ip}: {e}"));
                        continue;
                    }
                };
                s.set_read_timeout(Some(NO_ANSWER))?;
                map.insert(from, (s.clone(), Instant::now()));
                log(format!("Bridge: new player {from}"));
                // server replies -> to the friend
                let (back, l, cl, lg) = (s.clone(), listen.clone(), clients.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut b = vec![0u8; 65535];
                    let (mut answered, mut warned, mut last) = (false, false, Instant::now());
                    loop {
                        match back.recv_from(&mut b) {
                            Ok((m, src)) if src == target => {
                                if !answered {
                                    lg(format!("Puente: {target} responde a {from}"));
                                }
                                answered = true;
                                last = Instant::now();
                                let _ = l.send_to(&b[..m], from);
                                if let Some(e) = cl.lock().unwrap().get_mut(&from) {
                                    e.1 = Instant::now();
                                }
                            }
                            Ok(_) => {}
                            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                                if !answered && !warned {
                                    warned = true;
                                    lg(format!("Bridge: {target} does not answer {from} (server off, VPN down or no VPN profile?)"));
                                }
                                if last.elapsed() >= IDLE {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    cl.lock().unwrap().remove(&from);
                    lg(format!("Puente: {from} inactivo, olvidado"));
                });
                s
            }
        };
        drop(map);
        // a one-off error (VPN down for a moment) must not bring down the bridge for everyone
        if let Err(e) = sock.send_to(&buf[..n], target) {
            log(format!("Bridge: cannot send to {target}: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_both_ways() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target = server.local_addr().unwrap();
        let port = UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port();
        std::thread::spawn(move || run(port, target, "127.0.0.1".parse().unwrap(), |_| {}));
        std::thread::sleep(Duration::from_millis(100));
        let c = UdpSocket::bind("127.0.0.1:0").unwrap();
        c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        c.send_to(b"hola", ("127.0.0.1", port)).unwrap();
        let mut b = [0u8; 16];
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let (n, via) = server.recv_from(&mut b).unwrap();
        assert_eq!(&b[..n], b"hola");
        server.send_to(b"ok", via).unwrap();
        let (n, _) = c.recv_from(&mut b).unwrap();
        assert_eq!(&b[..n], b"ok");
    }
}
