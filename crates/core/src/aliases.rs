//! Console aliases: a short command that stands for a longer one. Extra arguments go at the end.
use crate::paths;
use anyhow::bail;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// short name -> command. A `;` is allowed because the user wrote it, in their own file, for their own console.
pub type Aliases = BTreeMap<String, String>;

fn path() -> PathBuf {
    paths::config_dir().join("aliases.toml")
}

fn check(name: &str, command: &str) -> crate::Result<()> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-') {
        bail!("invalid alias '{name}': lowercase letters, numbers, - and _");
    }
    if command.trim().is_empty() || command.contains(['\n', '\r', '\0']) {
        bail!("invalid command for alias '{name}'");
    }
    if crate::rcon::hides_secret(command) {
        bail!("an alias cannot hold a password");
    }
    Ok(())
}

pub fn load() -> crate::Result<Aliases> {
    match std::fs::read_to_string(path()) {
        Ok(t) => {
            let a: Aliases = toml::from_str(&t)?;
            // the file can be edited by hand, so what it says is checked like anything else
            a.iter().try_for_each(|(k, v)| check(k, v))?;
            Ok(a)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Aliases::new()),
        Err(e) => Err(e.into()),
    }
}

fn save(a: &Aliases) -> crate::Result<()> {
    std::fs::create_dir_all(paths::config_dir())?;
    std::fs::write(path(), toml::to_string(a)?)?;
    Ok(())
}

pub fn set(name: &str, command: &str) -> crate::Result<()> {
    check(name, command)?;
    let mut a = load()?;
    a.insert(name.into(), command.trim().into());
    save(&a)
}

pub fn remove(name: &str) -> crate::Result<()> {
    let mut a = load()?;
    if a.remove(name).is_none() {
        bail!("there is no alias '{name}'");
    }
    save(&a)
}

/// What to send for a line typed in a console: the alias with the rest of the line after it, or the line itself.
pub fn expand(a: &Aliases, line: &str) -> String {
    let line = line.trim();
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    match a.get(first) {
        Some(cmd) if rest.trim().is_empty() => cmd.clone(),
        Some(cmd) => format!("{cmd} {}", rest.trim()),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_only_the_first_word() {
        let a: Aliases = [("r".to_string(), "mp_restartgame 1".to_string()), ("k".to_string(), "sm_kick".to_string())].into();
        assert_eq!(expand(&a, " r "), "mp_restartgame 1");
        assert_eq!(expand(&a, "k Luis  now"), "sm_kick Luis  now");
        assert_eq!(expand(&a, "status r"), "status r");
    }

    #[test]
    fn checks_names_and_commands() {
        assert!(check("ok_1-a", "status; say hi").is_ok());
        for (n, c) in [("", "x"), ("Bad", "x"), ("a b", "x"), ("a", " "), ("a", "x\ny"), ("a", "rcon_password x")] {
            assert!(check(n, c).is_err(), "{n} {c}");
        }
    }
}
