//! Red sin herramientas externas (sin ip/ss/hostname -I).
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs, UdpSocket};

/// IP local con la que se sale a internet (truco del socket UDP: no envía nada).
pub fn local_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:80").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
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
    fn puerto() {
        let s = UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(!udp_port_free(s.local_addr().unwrap().port()));
    }
}
