use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone)]
struct Session {
    id: String,
    root: String,
    parent: Option<String>,
    role: String,
    status: String,
    scope: Option<Value>,
}

pub fn run(home: &Path, args: &[String]) -> Result<(), String> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with('-'))
        .ok_or("trace requires a session id")?;
    let json_output = args.iter().any(|arg| arg == "--json");
    let filters = args
        .windows(2)
        .find(|pair| pair[0] == "--events")
        .map(|pair| {
            pair[1]
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<BTreeSet<_>>()
        });
    let db =
        Connection::open_with_flags(home.join("supervisor.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let requested: Option<(String,)> = db
        .query_row("SELECT root_id FROM sessions WHERE id=?1", [id], |row| {
            Ok((row.get(0)?,))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let root = requested
        .ok_or_else(|| format!("session {id:?} not found"))?
        .0;
    let mut stmt = db.prepare("SELECT id,root_id,parent_id,role,status,scope_json FROM sessions WHERE root_id=?1 OR id=?1 ORDER BY created_at,id").map_err(|e| e.to_string())?;
    let sessions = stmt
        .query_map([&root], |row| {
            let scope: Option<String> = row.get(5)?;
            Ok(Session {
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
        let mut q = db.prepare("SELECT seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data FROM supervisor_events WHERE root_session_id=?1 ORDER BY ts_mono,seq").map_err(|e| e.to_string())?;
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
                push_event(
                    &mut events,
                    json!({"seq":seq,"ts":ts,"ts_mono":mono,"session_id":sid,"root_session_id":rid,"agent_role":role,"source":"supervisor","type":kind,"data":serde_json::from_str::<Value>(&data).map_err(|e|e.to_string())?}),
                    &filters,
                )?;
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
        let mut q = agent.prepare("SELECT seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data FROM events ORDER BY ts_mono,seq").map_err(|e| e.to_string())?;
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
            // Container-writable event rows cannot choose their session identity.
            if sid != session.id || rid != session.root || role != session.role {
                continue;
            }
            push_event(
                &mut events,
                json!({"seq":seq,"ts":ts,"ts_mono":mono,"session_id":sid,"root_session_id":rid,"agent_role":role,"source":"agent","type":kind,"data":serde_json::from_str::<Value>(&data).map_err(|e|e.to_string())?}),
                &filters,
            )?;
        }
    }
    events.sort_by(|a, b| {
        a["ts"]
            .as_str()
            .cmp(&b["ts"].as_str())
            .then(a["source"].as_str().cmp(&b["source"].as_str()))
            .then(a["seq"].as_i64().cmp(&b["seq"].as_i64()))
    });
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&events).map_err(|e| e.to_string())?
        );
    } else {
        render_text(&sessions, &events);
    }
    Ok(())
}

use rusqlite::OptionalExtension;

fn push_event(
    events: &mut Vec<Value>,
    event: Value,
    filters: &Option<BTreeSet<&str>>,
) -> Result<(), String> {
    let kind = event["type"].as_str().unwrap_or_default();
    if filters.as_ref().is_some_and(|set| !set.contains(kind)) {
        return Ok(());
    }
    events.push(event);
    Ok(())
}

fn render_text(sessions: &[Session], events: &[Value]) {
    for (index, session) in sessions.iter().enumerate() {
        let scope = session
            .scope
            .as_ref()
            .map(scope_summary)
            .unwrap_or_else(|| "scope unavailable".into());
        let branch = if session.parent.is_some() {
            if index + 1 == sessions.len() {
                "└─"
            } else {
                "├─"
            }
        } else {
            ""
        };
        println!(
            "{branch} {} {} {}  {scope}",
            session.id, session.role, session.status
        );
        let rows = events
            .iter()
            .filter(|event| event["session_id"] == session.id);
        for event in rows {
            println!(
                "{}  {} {}",
                if session.parent.is_some() {
                    "   │"
                } else {
                    "├─"
                },
                event["seq"],
                format_event(event)
            );
        }
    }
}

fn scope_summary(scope: &Value) -> String {
    let mounts = scope
        .get("mounts")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let tools = scope
        .get("tools")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let creds = scope
        .get("credentials")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!("scope: {mounts} mounts · {tools} tools · {creds} creds")
}

fn format_event(event: &Value) -> String {
    let kind = event["type"].as_str().unwrap_or("event");
    let data = &event["data"];
    match kind {
        "tool.call" => format!(
            "tool.call {} {}",
            data["server"].as_str().unwrap_or(""),
            data["tool"].as_str().unwrap_or("")
        ),
        "model.call" => format!(
            "model.call {}ms in {} / out {}",
            data["duration_ms"].as_i64().unwrap_or(0),
            data["input_tokens"].as_i64().unwrap_or(0),
            data["output_tokens"].as_i64().unwrap_or(0)
        ),
        _ => kind.to_owned(),
    }
}
