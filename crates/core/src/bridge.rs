//! Puente UDP para amigos por VPN con el servidor en modo LAN (antes puente.py).
//! Servidor: escucha en PUERTO+1 y reenvía al servidor desde la IP local, que así ve a los amigos como LAN.
//! Amigo: escucha en su PC y reenvía al puente del servidor; él conecta a su propia IP local
//! (Steam rechaza con STEAMAUTH código 10 a quien conecta a una IP 100.x de VPN).
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(120);

type Clients = Arc<Mutex<HashMap<SocketAddr, (Arc<UdpSocket>, Instant)>>>;

/// `listen_port`: dónde reciben los amigos; `target`: servidor (o puente remoto);
/// `source_ip`: IP desde la que se reenvía (la local en el servidor, 0.0.0.0 en el amigo).
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
                let s = Arc::new(UdpSocket::bind((source_ip, 0))?);
                s.set_read_timeout(Some(IDLE))?;
                map.insert(from, (s.clone(), Instant::now()));
                log(format!("Puente: nuevo jugador {from}"));
                // respuestas del servidor -> al amigo
                let (back, l, cl, lg) = (s.clone(), listen.clone(), clients.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut b = vec![0u8; 65535];
                    while let Ok((m, src)) = back.recv_from(&mut b) {
                        if src == target {
                            let _ = l.send_to(&b[..m], from);
                            if let Some(e) = cl.lock().unwrap().get_mut(&from) {
                                e.1 = Instant::now();
                            }
                        }
                    }
                    cl.lock().unwrap().remove(&from);
                    lg(format!("Puente: {from} inactivo, olvidado"));
                });
                s
            }
        };
        drop(map);
        sock.send_to(&buf[..n], target)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reenvia_ida_y_vuelta() {
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
