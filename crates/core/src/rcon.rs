//! Source RCON client (TCP). Detects which IP srcds listens on.
use anyhow::{bail, Context};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

const AUTH: i32 = 3;
const EXEC: i32 = 2;
const RESPONSE: i32 = 0;

#[derive(Debug)]
pub struct Rcon {
    stream: TcpStream,
    next_id: i32,
    pub addr: SocketAddr,
}

fn packet(id: i32, kind: i32, body: &str) -> Vec<u8> {
    let mut d = Vec::with_capacity(body.len() + 14);
    d.extend_from_slice(&((body.len() + 10) as i32).to_le_bytes());
    d.extend_from_slice(&id.to_le_bytes());
    d.extend_from_slice(&kind.to_le_bytes());
    d.extend_from_slice(body.as_bytes());
    d.extend_from_slice(&[0, 0]);
    d
}

fn read_packet(s: &mut TcpStream) -> std::io::Result<(i32, String)> {
    read_full(s).map(|(id, _, b)| (id, b))
}

fn read_full(s: &mut TcpStream) -> std::io::Result<(i32, i32, String)> {
    let mut n = [0u8; 4];
    s.read_exact(&mut n)?;
    let size = i32::from_le_bytes(n);
    if !(10..=1 << 20).contains(&size) {
        return Err(std::io::Error::new(ErrorKind::InvalidData, "invalid rcon packet"));
    }
    let mut d = vec![0u8; size as usize];
    s.read_exact(&mut d)?;
    let id = i32::from_le_bytes(d[0..4].try_into().unwrap());
    let kind = i32::from_le_bytes(d[4..8].try_into().unwrap());
    let body = String::from_utf8_lossy(&d[8..d.len() - 2]).into_owned();
    Ok((id, kind, body))
}

impl Rcon {
    pub fn connect_addr(addr: SocketAddr, password: &str) -> crate::Result<Rcon> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(800))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_nodelay(true).ok();
        stream.write_all(&packet(1, AUTH, password))?;
        // srcds answers with an empty packet and then the result (type 2; id -1 = wrong password)
        loop {
            let (id, kind, _) = read_full(&mut stream).context("rcon: no answer to authentication")?;
            if kind == 2 {
                if id == -1 {
                    bail!("rcon: wrong password");
                }
                break;
            }
        }
        let mut r = Rcon { stream, next_id: 10, addr };
        r.drain();
        Ok(r)
    }

    /// Tries the candidate IPs (config, hostname, 127.0.0.1, local IP).
    pub fn connect(configured_host: &str, port: u16, password: &str) -> crate::Result<Rcon> {
        let mut last = None;
        for ip in crate::net::rcon_candidates(configured_host) {
            match Rcon::connect_addr(SocketAddr::new(ip, port), password) {
                Ok(r) => return Ok(r),
                Err(e) if e.to_string().contains("password") => return Err(e),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("no IPs")).context("the server does not answer rcon (is it running?)"))
    }

    pub fn from_config(c: &crate::config::Config) -> crate::Result<Rcon> {
        Rcon::connect(&c.server.rcon_host, c.server.port, &c.server.rcon_password)
    }

    fn drain(&mut self) {
        let _ = self.stream.set_read_timeout(Some(Duration::from_millis(30)));
        while read_packet(&mut self.stream).is_ok() {}
        let _ = self.stream.set_read_timeout(Some(Duration::from_secs(5)));
    }

    /// Runs a command and returns the full response (even across several packets).
    pub fn exec(&mut self, cmd: &str) -> crate::Result<String> {
        if cmd.contains(['\0', '\n']) {
            bail!("rcon command with forbidden characters");
        }
        let id = self.next_id;
        let marker = id + 1;
        self.next_id += 2;
        self.stream.write_all(&packet(id, EXEC, cmd))?;
        // empty marker packet: srcds returns it after the whole response
        self.stream.write_all(&packet(marker, RESPONSE, ""))?;
        let mut out = String::new();
        loop {
            let (rid, body) = read_packet(&mut self.stream).context("rcon: connection lost")?;
            if rid == marker {
                break;
            }
            if rid == id {
                out.push_str(&body);
            }
        }
        self.drain();
        Ok(out)
    }

    pub fn ip(&self) -> IpAddr {
        self.addr.ip()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Fake rcon server imitating srcds.
    fn fake() -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            loop {
                let Ok((id, body)) = read_packet(&mut s) else { return };
                match body.as_str() {
                    "secreto" => {
                        s.write_all(&packet(id, RESPONSE, "")).unwrap();
                        s.write_all(&packet(id, 2, "")).unwrap();
                    }
                    "bad" => {
                        s.write_all(&packet(id, RESPONSE, "")).unwrap();
                        s.write_all(&packet(-1, 2, "")).unwrap();
                    }
                    "" => {
                        s.write_all(&packet(id, RESPONSE, "")).unwrap();
                        s.write_all(&packet(id, RESPONSE, "\u{1}")).unwrap();
                    }
                    c => {
                        s.write_all(&packet(id, RESPONSE, &format!("{c}-1|"))).unwrap();
                        s.write_all(&packet(id, RESPONSE, "2")).unwrap();
                    }
                }
            }
        });
        a
    }

    #[test]
    fn multi_packet() {
        let mut r = Rcon::connect_addr(fake(), "secreto").unwrap();
        assert_eq!(r.exec("status").unwrap(), "status-1|2");
        assert_eq!(r.exec("otro").unwrap(), "otro-1|2");
    }

    #[test]
    fn wrong_password() {
        assert!(Rcon::connect_addr(fake(), "bad").unwrap_err().to_string().contains("password"));
    }
}
