//! Mode rules as blocks: "when <event>, <action>". A closed catalog, stored as `modo_regla_N` in the mode's cfg and run
//! by `game/plugin/modos_reglas.sp`. Nothing here runs code or free commands: an imported mode comes from someone else.
use anyhow::{bail, Context};

pub const MAX_RULES: usize = 16;
pub const EVENTS: [&str; 4] = ["spawn", "kill", "headshot", "round_start"];
pub const ACTIONS: [&str; 5] = ["give", "health", "speed", "money", "message"];

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub event: String,
    pub action: String,
    pub value: String,
}

impl Rule {
    pub fn new(event: &str, action: &str, value: &str) -> crate::Result<Self> {
        if !EVENTS.contains(&event) {
            bail!("unknown rule event '{event}'");
        }
        let value = value.trim();
        match action {
            "give" => {
                let name = value.strip_prefix("weapon_").or_else(|| value.strip_prefix("item_")).unwrap_or("");
                if name.is_empty() || value.len() > 31 || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
                    bail!("invalid weapon in a rule: '{value}'");
                }
            }
            "health" => in_range(value, -1000.0, 10000.0, true)?,
            "money" => in_range(value, -16000.0, 16000.0, true)?,
            "speed" => in_range(value, 0.1, 5.0, false)?,
            "message" => {
                if value.is_empty() || value.chars().count() > 120 || value.contains([';', '"', '\n', '\r', '%']) {
                    bail!("invalid message in a rule: up to 120 characters, no ; % or quotes");
                }
            }
            _ => bail!("unknown rule action '{action}'"),
        }
        Ok(Rule { event: event.into(), action: action.into(), value: value.into() })
    }

    /// As it goes in the cfg.
    pub fn to_value(&self) -> String {
        format!("{} {} {}", self.event, self.action, self.value)
    }

    pub fn parse(s: &str) -> crate::Result<Self> {
        let mut it = s.trim().splitn(3, ' ');
        let (e, a, v) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
        Rule::new(e, a, v)
    }
}

fn in_range(v: &str, min: f64, max: f64, whole: bool) -> crate::Result<()> {
    let n: f64 = v.parse().with_context(|| format!("invalid number in a rule: '{v}'"))?;
    if !n.is_finite() || n < min || n > max || (whole && n.fract() != 0.0) {
        bail!("number out of range in a rule: '{v}' ({min} to {max})");
    }
    Ok(())
}

/// The number of a `modo_regla_N` key, if it is one.
pub fn rule_index(key: &str) -> Option<&str> {
    key.strip_prefix("modo_regla_")
}

pub fn key(i: usize) -> String {
    format!("modo_regla_{}", i + 1)
}

/// Checks one pair of a cfg. Empty means the slot is free.
pub fn check_pair(key: &str, value: &str) -> crate::Result<()> {
    let Some(n) = rule_index(key) else { return Ok(()) };
    if !n.parse::<usize>().is_ok_and(|n| (1..=MAX_RULES).contains(&n)) {
        bail!("invalid rule '{key}': from 1 to {MAX_RULES}");
    }
    if !value.trim().is_empty() {
        Rule::parse(value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_catalog() {
        for s in [
            "spawn give weapon_awp",
            "spawn give item_assaultsuit",
            "kill health 25",
            "headshot money -300",
            "round_start speed 1.5",
            "kill message Well done, keep going!",
        ] {
            assert_eq!(Rule::parse(s).unwrap().to_value(), s);
        }
    }

    #[test]
    fn refuses_everything_else() {
        for s in [
            "",
            "spawn",
            "connect give weapon_awp",
            "spawn exec weapon_awp",
            "spawn give ak47",
            "spawn give weapon_;quit",
            "spawn give weapon_AK47",
            "kill health 99999",
            "kill health 1.5",
            "kill health abc",
            "round_start speed 0",
            "round_start speed nan",
            "kill message a;quit",
            "kill message 100%s",
            "kill message \"x\"",
        ] {
            assert!(Rule::parse(s).is_err(), "{s}");
        }
    }

    #[test]
    fn only_numbered_slots() {
        assert!(check_pair("sv_gravity", "whatever;").is_ok(), "not a rule: another check handles it");
        assert!(check_pair("modo_regla_1", "").is_ok());
        assert!(check_pair("modo_regla_16", "kill health 5").is_ok());
        assert!(check_pair("modo_regla_17", "kill health 5").is_err());
        assert!(check_pair("modo_regla_x", "kill health 5").is_err());
        assert!(check_pair("modo_regla_1", "kill exec 5").is_err());
    }
}
