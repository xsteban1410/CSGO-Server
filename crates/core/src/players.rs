//! Jugadores: parseo de `status` y comandos de kick/ban.
use anyhow::bail;

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub userid: u32,
    pub name: String,
    /// STEAM_1:x:y, o "BOT".
    pub steamid: String,
    pub bot: bool,
    pub connected: String,
    pub ping: u32,
    pub loss: u32,
    pub addr: String,
}

impl Player {
    /// SteamID64 a partir de STEAM_X:Y:Z.
    pub fn steamid64(&self) -> Option<u64> {
        steamid64(&self.steamid)
    }
}

pub fn steamid64(s: &str) -> Option<u64> {
    let mut it = s.strip_prefix("STEAM_")?.split(':');
    let _universe = it.next()?;
    let y: u64 = it.next()?.parse().ok()?;
    let z: u64 = it.next()?.parse().ok()?;
    Some(76561197960265728 + z * 2 + y)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub hostname: String,
    pub map: String,
    pub humans: u32,
    pub bots: u32,
    pub max: u32,
    pub players: Vec<Player>,
}

/// Parsea la salida de `status` de CS:GO.
pub fn parse_status(s: &str) -> Status {
    let mut st = Status::default();
    for line in s.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("hostname:") {
            st.hostname = v.trim().into();
        } else if let Some(v) = l.strip_prefix("map") {
            if let Some(v) = v.trim_start().strip_prefix(':') {
                st.map = v.split_whitespace().next().unwrap_or("").into();
            }
        } else if let Some(v) = l.strip_prefix("players :") {
            // "1 humans, 9 bots (20/0 max) (not hibernating)"
            let w: Vec<&str> = v.split_whitespace().collect();
            st.humans = w.first().and_then(|x| x.parse().ok()).unwrap_or(0);
            st.bots = w.get(2).and_then(|x| x.parse().ok()).unwrap_or(0);
            st.max = w.get(4).and_then(|x| x.trim_start_matches('(').split('/').next()?.parse().ok()).unwrap_or(0);
        } else if let Some(p) = parse_player(l) {
            st.players.push(p);
        }
    }
    st
}

fn parse_player(l: &str) -> Option<Player> {
    // "# 2 "Bot" BOT active 64"  /  "# 3 1 "Nombre" STEAM_1:0:1 05:21 41 0 active 786432 1.2.3.4:27005"
    let rest = l.strip_prefix('#')?.trim_start();
    let q1 = rest.find('"')?;
    let q2 = rest.rfind('"')?;
    if q2 <= q1 {
        return None;
    }
    let userid: u32 = rest[..q1].split_whitespace().next()?.parse().ok()?;
    let name = rest[q1 + 1..q2].to_string();
    let w: Vec<&str> = rest[q2 + 1..].split_whitespace().collect();
    let steamid = w.first()?.to_string();
    let bot = steamid == "BOT";
    let (connected, ping, loss, addr) = if bot {
        (String::new(), 0, 0, String::new())
    } else {
        (
            w.get(1).unwrap_or(&"").to_string(),
            w.get(2).and_then(|x| x.parse().ok()).unwrap_or(0),
            w.get(3).and_then(|x| x.parse().ok()).unwrap_or(0),
            w.get(6).unwrap_or(&"").to_string(),
        )
    };
    Some(Player { userid, name, steamid, bot, connected, ping, loss, addr })
}

fn clean_reason(r: &str) -> String {
    r.chars().filter(|c| !matches!(c, '"' | ';' | '\n' | '\r')).collect()
}

pub fn kick_cmd(userid: u32, reason: &str) -> String {
    format!("kickid {userid} \"{}\"", clean_reason(reason))
}

/// Ban de SourceMod (basebans): minutos, 0 = permanente.
pub fn ban_cmd(userid: u32, minutes: u32, reason: &str) -> String {
    format!("sm_ban #{userid} {minutes} \"{}\"", clean_reason(reason))
}

pub fn unban_cmd(steamid: &str) -> crate::Result<String> {
    if !steamid.starts_with("STEAM_") || !steamid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'_') {
        bail!("SteamID inválido '{steamid}'");
    }
    Ok(format!("sm_unban \"{steamid}\""))
}

/// Baneos guardados por SourceMod/srcds en cfg/banned_user.cfg ("banid 0 STEAM_1:0:1").
pub fn read_bans(csgo_dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(csgo_dir.join("cfg/banned_user.cfg"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().nth(2).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const S: &str = r#"hostname: CS:GO Legacy contra bots
version : 1.38.8.1/13881 1575/8853 insecure  [G:1:123]
udp/ip  : 127.0.1.1:27030  (public ip: 1.2.3.4)
os      :  Linux
type    :  community dedicated
map     : de_dust2
players : 1 humans, 2 bots (20/0 max) (not hibernating)

# userid name uniqueid connected ping loss state rate adr
#  2 "Albert" BOT active 64
#  3 "Bot \"Raro\"" BOT active 64
#  4 1 "Player One" STEAM_1:1:12345 05:21 41 0 active 786432 192.168.0.20:27005
#end
"#;

    #[test]
    fn status() {
        let s = parse_status(S);
        assert_eq!(s.map, "de_dust2");
        assert_eq!((s.humans, s.bots, s.max), (1, 2, 20));
        assert_eq!(s.players.len(), 3);
        let h = &s.players[2];
        assert_eq!((h.userid, h.name.as_str(), h.ping, h.bot), (4, "Player One", 41, false));
        assert_eq!(h.addr, "192.168.0.20:27005");
        assert_eq!(h.steamid64(), Some(76561197960265728 + 12345 * 2 + 1));
        assert!(s.players[0].bot);
    }

    #[test]
    fn comandos() {
        assert_eq!(kick_cmd(3, "fuera\";quit"), "kickid 3 \"fueraquit\"");
        assert_eq!(ban_cmd(3, 0, "x"), "sm_ban #3 0 \"x\"");
        assert!(unban_cmd("STEAM_1:0:1;quit").is_err());
    }
}
