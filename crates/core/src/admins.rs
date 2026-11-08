//! Admins and roles: who may use admin commands in game, and the file `csls_admins.sp` reads.
//!
//! The server runs with `-insecure`, so a SteamID can be faked. Permissions that give the server console
//! (`console`, `root`) are only granted after `sm_login <password>` in game; the rest go by SteamID alone.
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::path::Path;

const STEAMID64_BASE: u64 = 76561197960265728;

/// The closed catalog of permissions; each maps to SourceMod admin flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// `sm_rcon` and `sm_cvar`: the server console from the game
    Console,
    Kick,
    Ban,
    Map,
    Slay,
    Chat,
    /// aimbot, wallhack, noclip and similar hub tools (CUSTOM1)
    Cheats,
    /// bot control (CUSTOM2)
    Bots,
    /// everything
    Root,
}

impl Permission {
    pub const ALL: [Permission; 9] = [
        Permission::Console,
        Permission::Kick,
        Permission::Ban,
        Permission::Map,
        Permission::Slay,
        Permission::Chat,
        Permission::Cheats,
        Permission::Bots,
        Permission::Root,
    ];

    /// SourceMod flag letters (see admin_flags in SourceMod's admin.inc).
    pub fn flags(self) -> &'static str {
        match self {
            Permission::Console => "hm",
            Permission::Kick => "c",
            Permission::Ban => "de",
            Permission::Map => "g",
            Permission::Slay => "f",
            Permission::Chat => "j",
            Permission::Cheats => "o",
            Permission::Bots => "p",
            Permission::Root => "z",
        }
    }

    /// Needs `sm_login` as well as the SteamID.
    pub fn needs_login(self) -> bool {
        matches!(self, Permission::Console | Permission::Root)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Console => "console",
            Permission::Kick => "kick",
            Permission::Ban => "ban",
            Permission::Map => "map",
            Permission::Slay => "slay",
            Permission::Chat => "chat",
            Permission::Cheats => "cheats",
            Permission::Bots => "bots",
            Permission::Root => "root",
        }
    }

    pub fn parse(s: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct User {
    /// STEAM_X:Y:Z, [U:1:N] or SteamID64, as the user typed it
    pub steamid: String,
    pub name: String,
    pub role: String,
    /// for `sm_login`; only roles with `console` or `root` need it
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Admins {
    pub users: Vec<User>,
    pub roles: Vec<Role>,
}

impl Default for Admins {
    fn default() -> Self {
        use Permission::*;
        let role = |name: &str, permissions: &[Permission]| Role { name: name.into(), permissions: permissions.to_vec() };
        Admins {
            users: Vec::new(),
            roles: vec![
                role("owner", &[Root]),
                role("admin", &[Console, Kick, Ban, Map, Slay, Chat, Bots]),
                role("moderator", &[Kick, Slay, Chat]),
                role("cheats", &[Cheats]),
            ],
        }
    }
}

/// The Steam account id (the low 32 bits) of STEAM_X:Y:Z, [U:1:N] or a SteamID64.
pub fn account_id(s: &str) -> Option<u32> {
    let s = s.trim();
    let id = if s.starts_with("STEAM_") {
        crate::players::steamid64(s)? - STEAMID64_BASE
    } else if let Some(n) = s.strip_prefix("[U:1:").and_then(|r| r.strip_suffix(']')) {
        n.parse().ok()?
    } else {
        s.parse::<u64>().ok()?.checked_sub(STEAMID64_BASE)?
    };
    u32::try_from(id).ok().filter(|id| *id > 0)
}

/// A value written between quotes in a KeyValues file or typed in a console.
fn plain(s: &str) -> bool {
    !s.contains(['"', ';', '\n', '\r', '\\', '{', '}'])
}

impl Admins {
    pub fn role(&self, name: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.name == name)
    }

    /// The index of a user by SteamID (any format) or name.
    pub fn find(&self, who: &str) -> Option<usize> {
        let id = account_id(who);
        self.users.iter().position(|u| (id.is_some() && account_id(&u.steamid) == id) || u.name == who)
    }

    pub fn validate(&self) -> crate::Result<()> {
        for r in &self.roles {
            if r.name.is_empty() || !r.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                bail!("invalid role name '{}': use letters, numbers, - and _", r.name);
            }
            if self.roles.iter().filter(|o| o.name == r.name).count() > 1 {
                bail!("role '{}' is defined twice", r.name);
            }
        }
        let mut seen = Vec::new();
        for u in &self.users {
            let Some(id) = account_id(&u.steamid) else { bail!("invalid SteamID '{}'", u.steamid) };
            if seen.contains(&id) {
                bail!("SteamID '{}' is listed twice", u.steamid);
            }
            seen.push(id);
            if !plain(&u.name) {
                bail!("admin name '{}' cannot contain quotes, ;, braces, \\ or line breaks", u.name);
            }
            if !plain(&u.password) || u.password.contains(' ') {
                bail!("the password of '{}' cannot contain spaces, quotes, ;, braces, \\ or line breaks", u.name);
            }
            let Some(role) = self.role(&u.role) else { bail!("'{}' has the unknown role '{}'", u.name, u.role) };
            if role.permissions.iter().any(|p| p.needs_login()) && u.password.len() < 8 {
                bail!(
                    "'{}' has the role '{}', which gives the server console: it needs a password of 8 characters or more",
                    u.name,
                    role.name
                );
            }
        }
        Ok(())
    }

    /// The KeyValues file `csls_admins.sp` reads: one block per account id with the flags given by SteamID
    /// and the ones given after `sm_login`.
    pub fn render(&self) -> String {
        let mut out = String::from("// Written by csgo-legacy-server from config.toml: edit admins there, not here.\n\"Admins\"\n{\n");
        for u in &self.users {
            let (Some(id), Some(role)) = (account_id(&u.steamid), self.role(&u.role)) else { continue };
            // b (generic) lets SourceMod's admin menu open for any admin
            let mut flags = String::from("b");
            let mut login = String::new();
            for p in &role.permissions {
                let to = if p.needs_login() { &mut login } else { &mut flags };
                for c in p.flags().chars() {
                    if !to.contains(c) {
                        to.push(c);
                    }
                }
            }
            out += &format!("\t\"{id}\"\n\t{{\n\t\t\"name\"\t\"{}\"\n\t\t\"flags\"\t\"{flags}\"\n", u.name);
            if !login.is_empty() {
                out += &format!("\t\t\"login\"\t\"{login}\"\n\t\t\"password\"\t\"{}\"\n", u.password);
            }
            out += "\t}\n";
        }
        out + "}\n"
    }

    /// Writes the file if it changed (mode 600: it holds passwords). True when it changed.
    pub fn write(&self, path: &Path) -> crate::Result<bool> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        self.validate()?;
        let txt = self.render();
        if std::fs::read_to_string(path).is_ok_and(|t| t == txt) {
            return Ok(false);
        }
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("cfg.tmp");
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(txt.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steamid_formats() {
        assert_eq!(account_id("STEAM_1:1:12345"), Some(24691));
        assert_eq!(account_id("STEAM_0:1:12345"), Some(24691));
        assert_eq!(account_id("[U:1:24691]"), Some(24691));
        assert_eq!(account_id("76561197960290419"), Some(24691));
        for bad in ["", "BOT", "STEAM_1:1:x", "[U:1:]", "123", "[U:1:0]", "STEAM_1:0:0"] {
            assert_eq!(account_id(bad), None, "{bad}");
        }
    }

    fn user(steamid: &str, name: &str, role: &str, password: &str) -> User {
        User { steamid: steamid.into(), name: name.into(), role: role.into(), password: password.into() }
    }

    #[test]
    fn console_needs_a_password_and_values_stay_plain() {
        let mut a = Admins::default();
        a.users.push(user("STEAM_1:0:1", "me", "admin", ""));
        assert!(a.validate().is_err(), "console without a password");
        a.users[0].password = "long-enough".into();
        a.validate().unwrap();
        for (field, v) in [("name", "x\"\n\"flags\" \"z"), ("password", "a;b c\"d e f g")] {
            let mut b = a.clone();
            if field == "name" {
                b.users[0].name = v.into()
            } else {
                b.users[0].password = v.into()
            }
            assert!(b.validate().is_err(), "{field}: {v}");
        }
        a.users.push(user("[U:1:2]", "twice", "cheats", ""));
        assert!(a.validate().is_err(), "the same account twice");
        a.users[1].steamid = "[U:1:3]".into();
        a.users[1].role = "nope".into();
        assert!(a.validate().is_err(), "unknown role");
    }

    #[test]
    fn renders_flags_and_login_apart() {
        let mut a = Admins::default();
        a.users.push(user("STEAM_1:0:1", "boss", "admin", "secret-pass"));
        a.users.push(user("[U:1:7]", "friend", "cheats", ""));
        let t = a.render();
        let kv = crate::kv::parse(&t);
        let admins = kv.get("Admins").unwrap();
        let boss = admins.get("2").unwrap();
        assert_eq!(boss.get("flags").unwrap().str(), Some("bcdegfjp"));
        assert_eq!(boss.get("login").unwrap().str(), Some("hm"));
        assert_eq!(boss.get("password").unwrap().str(), Some("secret-pass"));
        let friend = admins.get("7").unwrap();
        assert_eq!(friend.get("flags").unwrap().str(), Some("bo"));
        assert!(friend.get("login").is_none() && friend.get("password").is_none());
        assert_eq!(a.find("STEAM_1:1:3"), Some(1));
        assert_eq!(a.find("boss"), Some(0));
    }
}
