//! Cliente Source RCON (TCP). Detecta en qué IP escucha srcds.
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
        return Err(std::io::Error::new(ErrorKind::InvalidData, "paquete rcon inválido"));
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
        // srcds responde con un paquete vacío y luego el resultado (tipo 2; id -1 = contraseña mala)
        loop {
            let (id, kind, _) = read_full(&mut stream).context("rcon: sin respuesta a la autenticación")?;
            if kind == 2 {
                if id == -1 {
                    bail!("rcon: contraseña incorrecta");
                }
                break;
            }
        }
        let mut r = Rcon { stream, next_id: 10, addr };
        r.drain();
        Ok(r)
    }

    /// Prueba las IPs candidatas (config, hostname, 127.0.0.1, IP local).
    pub fn connect(configured_host: &str, port: u16, password: &str) -> crate::Result<Rcon> {
        let mut last = None;
        for ip in crate::net::rcon_candidates(configured_host) {
            match Rcon::connect_addr(SocketAddr::new(ip, port), password) {
                Ok(r) => return Ok(r),
                Err(e) if e.to_string().contains("contraseña") => return Err(e),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("sin IPs")).context("el servidor no responde por rcon (¿está encendido?)"))
    }

    pub fn from_config(c: &crate::config::Config) -> crate::Result<Rcon> {
        Rcon::connect(&c.server.rcon_host, c.server.port, &c.server.rcon_password)
    }

    fn drain(&mut self) {
        let _ = self.stream.set_read_timeout(Some(Duration::from_millis(30)));
        while read_packet(&mut self.stream).is_ok() {}
        let _ = self.stream.set_read_timeout(Some(Duration::from_secs(5)));
    }

    /// Ejecuta un comando y devuelve la respuesta completa (aunque venga en varios paquetes).
    pub fn exec(&mut self, cmd: &str) -> crate::Result<String> {
        if cmd.contains(['\0', '\n']) {
            bail!("comando rcon con caracteres no permitidos");
        }
        let id = self.next_id;
        let marker = id + 1;
        self.next_id += 2;
        self.stream.write_all(&packet(id, EXEC, cmd))?;
        // paquete vacío de marca: srcds lo devuelve después de toda la respuesta
        self.stream.write_all(&packet(marker, RESPONSE, ""))?;
        let mut out = String::new();
        loop {
            let (rid, body) = read_packet(&mut self.stream).context("rcon: conexión perdida")?;
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

    /// Servidor rcon falso que imita a srcds.
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
                    "malo" => {
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
    fn multipaquete() {
        let mut r = Rcon::connect_addr(fake(), "secreto").unwrap();
        assert_eq!(r.exec("status").unwrap(), "status-1|2");
        assert_eq!(r.exec("otro").unwrap(), "otro-1|2");
    }

    #[test]
    fn contrasena_mala() {
        assert!(Rcon::connect_addr(fake(), "malo").unwrap_err().to_string().contains("contraseña"));
    }
}
