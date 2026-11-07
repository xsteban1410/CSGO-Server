//! Client of the helper agent: JSON lines over its private socket. See `go/cmd/agent`.
use crate::paths;
use anyhow::{anyhow, Context};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn socket_path() -> PathBuf {
    paths::runtime_dir().join("agent.sock")
}

fn call_at(path: &Path, request: &Value) -> crate::Result<Value> {
    let mut s = UnixStream::connect(path).context("the agent is not running")?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.set_write_timeout(Some(Duration::from_secs(5)))?;
    writeln!(s, "{request}")?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).context("the agent did not answer")?;
    let answer: Value = serde_json::from_str(&line).context("the agent gave an invalid answer")?;
    if answer["ok"].as_bool() != Some(true) {
        return Err(anyhow!("{}", answer["error"].as_str().unwrap_or("the agent refused the request")));
    }
    Ok(answer.get("result").cloned().unwrap_or(Value::Null))
}

/// One request. `extra` holds the method's fields: since, limit, days, id, tasks or data.
pub fn call(method: &str, extra: Value) -> crate::Result<Value> {
    let mut request = if extra.is_object() { extra } else { json!({}) };
    request["method"] = json!(method);
    call_at(&socket_path(), &request)
}

fn typed<T: DeserializeOwned>(method: &str, extra: Value) -> crate::Result<T> {
    Ok(serde_json::from_value(call(method, extra)?)?)
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Player {
    pub steamid: String,
    pub name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub connections: u32,
    pub seconds: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Sanction {
    pub steamid: String,
    pub name: String,
    pub kind: String,
    pub reason: String,
    pub notes: String,
    pub minutes: u32,
    pub time: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Count {
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub maps: Vec<Count>,
    pub modes: Vec<Count>,
    pub per_day: Vec<Count>,
    pub per_hour: Vec<Count>,
    pub avg_players: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Task {
    pub id: String,
    pub action: String,
    pub arg: String,
    pub every: String,
    pub at: String,
    pub weekday: u8,
    pub hours: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TaskRun {
    pub id: String,
    pub start: i64,
    pub end: i64,
    pub ok: bool,
    pub output: String,
}

pub fn players() -> crate::Result<Vec<Player>> {
    Ok(typed::<Option<_>>("players", json!({}))?.unwrap_or_default())
}

pub fn sanctions() -> crate::Result<Vec<Sanction>> {
    Ok(typed::<Option<_>>("sanctions", json!({}))?.unwrap_or_default())
}

pub fn add_sanction(s: &Sanction) -> crate::Result<()> {
    call("add_sanction", json!({ "data": s })).map(drop)
}

pub fn summary(days: u32) -> crate::Result<Summary> {
    typed("summary", json!({ "days": days }))
}

pub fn erase_stats() -> crate::Result<()> {
    call("erase_stats", json!({})).map(drop)
}

pub fn tasks() -> crate::Result<Vec<Task>> {
    Ok(typed::<Option<_>>("tasks", json!({}))?.unwrap_or_default())
}

/// The agent checks them and refuses the whole list if one is wrong.
pub fn set_tasks(tasks: &[Task]) -> crate::Result<()> {
    call("set_tasks", json!({ "tasks": tasks })).map(drop)
}

pub fn run_task(id: &str) -> crate::Result<()> {
    call("run_task", json!({ "id": id })).map(drop)
}

pub fn task_history() -> crate::Result<Vec<TaskRun>> {
    Ok(typed::<Option<_>>("task_history", json!({}))?.unwrap_or_default())
}

/// A field that a spreadsheet could run as a formula. Player names come from anyone who joins.
fn csv_field(s: &str) -> String {
    let safe = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    if safe.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn csv(header: &[&str], rows: impl Iterator<Item = Vec<String>>) -> String {
    let mut out = header.join(",") + "\n";
    for r in rows {
        out += &r.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(",");
        out.push('\n');
    }
    out
}

pub fn players_csv(list: &[Player]) -> String {
    let rows = list.iter().map(|p| {
        vec![
            p.steamid.clone(),
            p.name.clone(),
            p.first_seen.to_string(),
            p.last_seen.to_string(),
            p.connections.to_string(),
            p.seconds.to_string(),
        ]
    });
    csv(&["steamid", "name", "first_seen", "last_seen", "connections", "seconds"], rows)
}

pub fn sanctions_csv(list: &[Sanction]) -> String {
    let rows = list.iter().map(|s| {
        vec![
            s.steamid.clone(),
            s.name.clone(),
            s.kind.clone(),
            s.reason.clone(),
            s.notes.clone(),
            s.minutes.to_string(),
            s.time.to_string(),
        ]
    });
    csv(&["steamid", "name", "kind", "reason", "notes", "minutes", "time"], rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn answering(answer: &'static str) -> (PathBuf, std::thread::JoinHandle<String>) {
        let dir = std::env::temp_dir().join(format!("csls-agent-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.sock");
        let _ = std::fs::remove_file(&path);
        let l = UnixListener::bind(&path).unwrap();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut got = String::new();
            BufReader::new(s.try_clone().unwrap()).read_line(&mut got).unwrap();
            writeln!(s, "{answer}").unwrap();
            got
        });
        (path, h)
    }

    #[test]
    fn sends_the_method_and_reads_the_result() {
        let (p, h) = answering(r#"{"ok":true,"result":{"a":1}}"#);
        let r = call_at(&p, &json!({"method":"status"})).unwrap();
        assert_eq!(r["a"], 1);
        assert!(h.join().unwrap().contains("\"status\""));
    }

    #[test]
    fn an_error_from_the_agent_is_an_error_here() {
        let (p, h) = answering(r#"{"ok":false,"error":"stats are off"}"#);
        let e = call_at(&p, &json!({"method":"players"})).unwrap_err();
        assert_eq!(e.to_string(), "stats are off");
        h.join().unwrap();
    }

    #[test]
    fn no_agent_says_so() {
        let e = call_at(Path::new("/nonexistent/a.sock"), &json!({})).unwrap_err();
        assert!(format!("{e:#}").contains("not running"));
    }

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=cmd|' /C calc'!A0"), "'=cmd|' /C calc'!A0");
        assert_eq!(csv_field("-1"), "'-1");
        let p = Player { steamid: "S".into(), name: "@x,y".into(), ..Default::default() };
        assert_eq!(players_csv(&[p]).lines().nth(1), Some("S,\"'@x,y\",0,0,0,0"));
    }
}
