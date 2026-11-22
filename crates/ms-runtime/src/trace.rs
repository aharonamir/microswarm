use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct TraceSession {
    pub id: String,
    pub root: String,
    pub parent: Option<String>,
    pub role: String,
    pub status: String,
    pub scope: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct TraceData {
    pub sessions: Vec<TraceSession>,
    pub events: Vec<Value>,
}

type CachedTrace = (PathBuf, String, Instant, TraceData);
static TRACE_CACHE: OnceLock<Mutex<VecDeque<CachedTrace>>> = OnceLock::new();

/// Shares the expensive read-only history build across concurrent console views.
pub fn build_cached(home: &Path, id: &str) -> Result<TraceData, String> {
    let cache = TRACE_CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    let mut entries = cache.lock().map_err(|error| error.to_string())?;
    if let Some(index) = entries.iter().position(|entry| {
        entry.0 == home && entry.1 == id && entry.2.elapsed() < Duration::from_millis(900)
    }) {
        let entry = entries.remove(index).expect("cached trace index exists");
        let trace = entry.3.clone();
        entries.push_front(entry);
        return Ok(trace);
    }
    let trace = build(home, id, None)?;
    entries.push_front((
        home.to_path_buf(),
        id.to_owned(),
        Instant::now(),
        trace.clone(),
    ));
    entries.truncate(16);
    Ok(trace)
}

pub fn build(
    home: &Path,
    id: &str,
    filters: Option<&BTreeSet<String>>,
) -> Result<TraceData, String> {
    let db =
        Connection::open_with_flags(home.join("supervisor.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let root: String = db
        .query_row("SELECT root_id FROM sessions WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("session {id:?} not found"))?;
    let mut stmt = db.prepare("SELECT id,root_id,parent_id,role,status,scope_json FROM sessions WHERE root_id=?1 OR id=?1 ORDER BY created_at,id").map_err(|e|e.to_string())?;
    let sessions = stmt
        .query_map([&root], |row| {
            let scope: Option<String> = row.get(5)?;
            Ok(TraceSession {
                id: row.get(0)?,
                root: row.get(1)?,
                parent: row.get(2)?,
                role: row.get(3)?,
                status: row.get(4)?,
                scope: scope.and_then(|s| serde_json::from_str(&s).ok()),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if sessions.is_empty() {
        return Err(format!("session {id:?} not found"));
    }
    let valid_ids = sessions
        .iter()
        .map(|s| s.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut events = Vec::<Value>::new();
    {
        let mut q=db.prepare("SELECT seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data FROM supervisor_events WHERE root_session_id=?1 ORDER BY ts_mono,seq").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([&root], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (seq, ts, mono, sid, rid, role, kind, data) = row.map_err(|e| e.to_string())?;
            if valid_ids.contains(sid.as_str()) {
                push(
                    &mut events,
                    json!({"seq":seq,"ts":ts,"ts_mono":mono,"session_id":sid,"root_session_id":rid,"agent_role":role,"source":"supervisor","type":kind,"data":serde_json::from_str::<Value>(&data).map_err(|e|e.to_string())?}),
                    filters,
                );
            }
        }
    }
    for session in &sessions {
        let path = home
            .join("sessions")
            .join(&session.id)
            .join("out/events.db");
        if !path.is_file() {
            continue;
        }
        let agent = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        let mut q=agent.prepare("SELECT seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data FROM events ORDER BY ts_mono,seq").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (seq, ts, mono, sid, rid, role, kind, data) = row.map_err(|e| e.to_string())?;
            if sid != session.id || rid != session.root || role != session.role {
                continue;
            }
            push(
                &mut events,
                json!({"seq":seq,"ts":ts,"ts_mono":mono,"session_id":sid,"root_session_id":rid,"agent_role":role,"source":"agent","type":kind,"data":serde_json::from_str::<Value>(&data).map_err(|e|e.to_string())?}),
                filters,
            );
        }
    }
    events.sort_by(|a, b| {
        a["ts"]
            .as_str()
            .cmp(&b["ts"].as_str())
            .then(a["source"].as_str().cmp(&b["source"].as_str()))
            .then(a["seq"].as_i64().cmp(&b["seq"].as_i64()))
    });
    Ok(TraceData { sessions, events })
}
fn push(events: &mut Vec<Value>, event: Value, filters: Option<&BTreeSet<String>>) {
    let kind = event["type"].as_str().unwrap_or_default();
    if filters.is_none_or(|set| set.contains(kind)) {
        events.push(event);
    }
}
