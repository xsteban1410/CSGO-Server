//! Source RCON client (TCP). Detects which IP srcds listens on.
use anyhow::{bail, Context};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

const AUTH: i32 = 3;
const EXEC: i32 = 2;
const RESPONSE: i32 = 0;

/// What can go wrong talking to the server, for code that reacts differently to each.
#[derive(Debug)]
pub enum RconError {
    /// Wrong password: retrying cannot help.
    Auth,
    /// The connection went away during a command, for example on a map change.
    Lost(std::io::Error),
}

impl std::fmt::Display for RconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RconError::Auth => write!(f, "rcon: wrong password"),
            RconError::Lost(e) => write!(f, "rcon: connection lost: {e}"),
        }
    }
}

impl std::error::Error for RconError {}

pub fn is_auth_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| matches!(c.downcast_ref::<RconError>(), Some(RconError::Auth)))
}

/// Commands that carry a secret: the console must not echo them or keep them in its history.
pub fn hides_secret(cmd: &str) -> bool {
    matches!(cmd.split_whitespace().next(), Some("rcon_password" | "sv_password" | "sv_setsteamaccount"))
}

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
                    return Err(RconError::Auth.into());
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
                Err(e) if is_auth_error(&e) => return Err(e),
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
            let (rid, body) = read_packet(&mut self.stream).map_err(RconError::Lost)?;
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

/// A connection that comes back by itself: the engine drops rcon on map changes and plugin reloads.
pub struct RconSession<'a> {
    cfg: &'a crate::config::Config,
    conn: Option<Rcon>,
}

impl<'a> RconSession<'a> {
    pub fn new(cfg: &'a crate::config::Config) -> Self {
        RconSession { cfg, conn: None }
    }

    /// Runs a command, reconnecting and repeating it if the connection was lost. The name is the contract: the
    /// server may have run the command before the answer was lost, so only pass commands that can safely run twice.
    pub fn exec_idempotent(&mut self, cmd: &str) -> crate::Result<String> {
        let mut last = None;
        for wait_ms in [0, 200, 600, 1500] {
            std::thread::sleep(Duration::from_millis(wait_ms));
            let result = match self.conn.take() {
                Some(c) => Ok(c),
                None => Rcon::from_config(self.cfg),
            }
            .and_then(|mut c| {
                let o = c.exec(cmd)?;
                self.conn = Some(c);
                Ok(o)
            });
            match result {
                Ok(o) => return Ok(o),
                Err(e) if is_auth_error(&e) => return Err(e),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("rcon: no attempts")))
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
        let e = Rcon::connect_addr(fake(), "bad").unwrap_err();
        assert!(is_auth_error(&e), "{e}");
        assert!(!is_auth_error(&anyhow::anyhow!("my password is wrong")), "only the real error counts");
    }

    #[test]
    fn knows_which_commands_hold_secrets() {
        assert!(hides_secret("rcon_password abc") && hides_secret("  sv_password x"));
        assert!(!hides_secret("status") && !hides_secret(""));
    }
}
