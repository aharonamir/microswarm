use crate::status::{SessionStatus, status_words};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row};
use serde_json::{Value, json};
use std::path::Path;

pub fn valid_session_id(id: &str) -> bool {
    id.len() == 26
        && id
            .bytes()
            .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
}

fn supervisor(home: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        home.join("supervisor.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())
}

pub fn sessions(home: &Path, group: &str) -> Result<Value, String> {
    if !matches!(group, "active" | "ended") {
        return Err("invalid session group".into());
    }
    let db = supervisor(home)?;
    let where_clause = if group == "active" {
        "status NOT IN ('closed','failed')"
    } else {
        "status IN ('closed','failed') AND closed_at >= datetime('now','-1 day')"
    };
    let mut query = db.prepare(&format!("SELECT id FROM sessions WHERE kind='lead' AND {where_clause} ORDER BY last_active_at DESC,id")).map_err(|e| e.to_string())?;
    let ids = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let views = ids
        .iter()
        .map(|id| session_view(home, &db, id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"sessions":views}))
}

pub fn session(home: &Path, id: &str) -> Result<Value, String> {
    if !valid_session_id(id) {
        return Err("invalid session id".into());
    }
    let db = supervisor(home)?;
    let session = session_view(home, &db, id)?;
    let mut query = db
        .prepare("SELECT id FROM sessions WHERE parent_id=?1 ORDER BY created_at,id")
        .map_err(|e| e.to_string())?;
    let members = query
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let team = members
        .iter()
        .map(|member| session_view(home, &db, member))
        .collect::<Result<Vec<_>, _>>()?;
    let approval_id = (session["open_approvals"].as_i64().unwrap_or(0) > 0).then_some("pending");
    let words = session["status_words"].clone();
    Ok(
        json!({"session":session,"team":team,"banner":{"words":words["en"],"detail":"Session state from the supervisor.","tone":words["tone"],"approval_id":approval_id}}),
    )
}

fn session_view(home: &Path, db: &Connection, id: &str) -> Result<Value, String> {
    let row = db.query_row("SELECT id,root_id,kind,role,channel,channel_thread,status,created_at,last_active_at,closed_at,close_reason,scope_json FROM sessions WHERE id=?1", [id], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,Option<String>>(4)?,row.get::<_,Option<String>>(5)?,row.get::<_,String>(6)?,row.get::<_,String>(7)?,row.get::<_,String>(8)?,row.get::<_,Option<String>>(9)?,row.get::<_,Option<String>>(10)?,row.get::<_,Option<String>>(11)?))).optional().map_err(|e|e.to_string())?.ok_or("session not found")?;
    let (
        sid,
        root,
        kind,
        role,
        channel,
        thread,
        status,
        created_at,
        last_active_at,
        closed_at,
        close_reason,
        scope_json,
    ) = row;
    let pending: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM approvals WHERE root_session_id=?1 AND status='pending'",
            [&root],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let waiting_team: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM delegations WHERE lead_id=?1 AND status='waiting_reply')",
            [&root],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let idle_minutes: i64 = db
        .query_row(
            "SELECT COALESCE(CAST((julianday('now')-julianday(?1))*1440 AS INTEGER),0)",
            [&last_active_at],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let ended_by_operator: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM supervisor_events WHERE session_id=?1 AND type='session.closed' AND json_extract(data,'$.by')='operator')", [&sid], |row| row.get(0)).unwrap_or(false);
    let status_input = SessionStatus {
        status: status.clone(),
        close_reason: close_reason.clone(),
        pending_approval: pending > 0,
        waiting_team,
        idle_minutes: (status == "ready" && idle_minutes > 5).then_some(idle_minutes.max(0) as u64),
        ended_by_operator,
        failure_reason: close_reason.clone(),
    };
    let words = status_words(&status_input);
    let (status_key, status_args) = status_translation(&words.en);
    let title = session_title(home, &sid).unwrap_or_else(|_| "New session".into());
    let scope = scope_json
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap_or(Value::Null);
    let members: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE parent_id=?1",
            [&sid],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(
        json!({"id":sid,"root_id":root,"title":truncate(title,60),"kind":kind,"role":role,"channel":channel,"channel_thread":thread,"status_words":{"en":words.en,"zh":words.zh,"key":status_key,"args":status_args,"tone":words.tone},"member_count":members,"open_approvals":pending,"created_at":created_at,"last_active_at":last_active_at,"closed_at":closed_at,"close_reason":close_reason,"in_flight":status=="busy","scope":scope}),
    )
}

fn session_title(home: &Path, id: &str) -> Result<String, String> {
    let path = home.join("sessions").join(id).join("control/inbound.db");
    if !path.is_file() {
        return Ok("New session".into());
    }
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    let text: Option<String> = db.query_row("SELECT json_extract(body,'$.text') FROM inbound WHERE kind='user_message' ORDER BY id LIMIT 1", [], |row| row.get(0)).optional().map_err(|e|e.to_string())?.flatten();
    Ok(text.unwrap_or_else(|| "New session".into()))
}

fn truncate(value: String, max: usize) -> String {
    let mut chars = value.chars();
    let short = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn status_translation(en: &str) -> (String, Value) {
    match en {
        "Waiting for you" => ("status.waiting_for_you".into(), json!({})),
        "Starting" => ("status.starting".into(), json!({})),
        "Working" => ("status.working".into(), json!({})),
        "Waiting on the team" => ("status.waiting_on_team".into(), json!({})),
        "Restarting after a crash" => ("status.restarting".into(), json!({})),
        "Ready for your next message" => ("status.ready".into(), json!({})),
        "Ended by you" => ("status.ended_by_you".into(), json!({})),
        "Ended · idle" => ("status.ended_idle".into(), json!({})),
        "Finished" => ("status.finished".into(), json!({})),
        "Stopped for safety" => ("status.stopped_safety".into(), json!({})),
        _ if en.starts_with("Idle for ") => (
            "status.idle".into(),
            json!({"minutes":en.trim_start_matches("Idle for ").trim_end_matches(" min").parse::<u64>().unwrap_or(0)}),
        ),
        _ if en.starts_with("Stopped · ") => (
            "status.stopped_failure".into(),
            json!({"reason":en.trim_start_matches("Stopped · ")}),
        ),
        _ => ("status.unknown".into(), json!({})),
    }
}

pub fn status_event(home: &Path, event: &Value) -> Option<Value> {
    let id = event["session_id"].as_str()?;
    let db = supervisor(home).ok()?;
    let view = session_view(home, &db, id).ok()?;
    let words = &view["status_words"];
    Some(
        json!({"type":"status.changed","at":event["ts"],"session_id":id,"root_session_id":view["root_id"],"status_words":{"key":words["key"],"args":words["args"],"tone":words["tone"]},"open_approvals":view["open_approvals"]}),
    )
}

pub fn trace(home: &Path, id: &str) -> Result<Value, String> {
    if !valid_session_id(id) {
        return Err("invalid session id".into());
    }
    let result = ms_runtime::trace::build(home, id, None)?;
    let sessions = result.sessions.iter().map(|session| json!({"id":session.id,"root":session.root,"parent":session.parent,"role":session.role,"status":session.status,"scope":session.scope})).collect::<Vec<_>>();
    Ok(json!({"sessions":sessions,"events":result.events}))
}

pub fn messages(home: &Path, id: &str, limit: usize) -> Result<Value, String> {
    if !valid_session_id(id) {
        return Err("invalid session id".into());
    }
    let root_db = supervisor(home)?;
    let mut items = Vec::<(String, Value)>::new();
    let inbound_path = home.join("sessions").join(id).join("control/inbound.db");
    if inbound_path.is_file() {
        let db = Connection::open_with_flags(
            inbound_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| e.to_string())?;
        let mut stmt=db.prepare("SELECT kind,body,created_at FROM inbound WHERE kind IN ('user_message','member_reply','member_failed','delegation_result','approval_result') ORDER BY created_at,id").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (kind, body, at) = row.map_err(|e| e.to_string())?;
            let body: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            let item = match kind.as_str() {
                "user_message" => {
                    json!({"kind":"user","at":at,"text":body["text"],"via":body["from"]})
                }
                "member_reply" => {
                    let member = body["member_id"].as_str().unwrap_or("");
                    let role: String = root_db
                        .query_row("SELECT role FROM sessions WHERE id=?1", [member], |row| {
                            row.get(0)
                        })
                        .unwrap_or_else(|_| "member".into());
                    json!({"kind":"lead","at":at,"text":body["text"],"from_role":role})
                }
                "member_failed" => {
                    json!({"kind":"team","at":at,"team":{"event":"member_failed","role":body["member_id"],"detail":body["reason"]}})
                }
                "delegation_result" => {
                    json!({"kind":"team","at":at,"team":{"event":"delegation_result","role":body["member_id"],"detail":body["status"]}})
                }
                _ => {
                    json!({"kind":"team","at":at,"team":{"event":"approval_result","detail":body["decision"]}})
                }
            };
            items.push((at, item));
        }
    }
    let outbound_path = home.join("sessions").join(id).join("out/outbound.db");
    if outbound_path.is_file() {
        let db = Connection::open_with_flags(
            outbound_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| e.to_string())?;
        let mut stmt=db.prepare("SELECT kind,body,created_at FROM outbound WHERE kind='reply' ORDER BY created_at,id").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (_, body, at) = row.map_err(|e| e.to_string())?;
            let body: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            items.push((
                at.clone(),
                json!({"kind":"lead","at":at,"text":body["text"]}),
            ));
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let start = items.len().saturating_sub(limit.clamp(1, 100));
    let next_before = if start > 0 {
        Some(items[start].0.clone())
    } else {
        None
    };
    Ok(
        json!({"items":items.into_iter().skip(start).map(|(_,item)|item).collect::<Vec<_>>(),"next_before":next_before}),
    )
}

pub fn approval(home: &Path, id: &str) -> Result<Value, String> {
    let db = supervisor(home)?;
    let row=db.query_row("SELECT id,code,root_session_id,requesting_session_id,kind,payload,status,expires_at,created_at,resolved_at,reissued_as FROM approvals WHERE id=?1 OR code=?1",[id],approval_row).optional().map_err(|e|e.to_string())?.ok_or("approval not found")?;
    Ok(json!({"approval":approval_value(home,row)?}))
}

pub fn host(home: &Path) -> Value {
    let config_path = if home.join("config/microswarm.toml").is_file() {
        home.join("config")
    } else {
        home.to_path_buf()
    };
    let config_file = config_path.join("microswarm.toml");
    let config = std::fs::read_to_string(&config_file)
        .unwrap_or_default()
        .parse::<toml::Value>()
        .unwrap_or(toml::Value::Table(Default::default()));
    let socket = config
        .get("runtime")
        .and_then(|v| v.get("socket"))
        .and_then(toml::Value::as_str)
        .unwrap_or("unknown");
    let image = config
        .get("runtime")
        .and_then(|v| v.get("image"))
        .and_then(toml::Value::as_str)
        .unwrap_or("unknown");
    let model = config
        .get("model")
        .and_then(|v| v.get("provider"))
        .and_then(toml::Value::as_str)
        .unwrap_or("not configured");
    let credential = config
        .get("model")
        .and_then(|v| v.get("credential"))
        .and_then(toml::Value::as_str)
        .unwrap_or("model credential");
    let checks = ms_runtime::doctor::run(ms_runtime::doctor::DoctorInput {
        config_path: config_file.to_string_lossy().as_ref(),
        runtime_socket: socket,
        agent_image: image,
        model,
        credential,
    });
    let doctor = checks
        .iter()
        .map(|check| json!({"check":check.name,"result":check.result,"detail":check.detail}))
        .collect::<Vec<_>>();
    let names = ms_runtime::credentials::list_credentials(home)
        .unwrap_or_else(|_| json!({"names":[]}))["names"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let credentials = names
        .into_iter()
        .filter_map(|name| name.as_str().map(|name| json!({"name":name,"used_by":[]})))
        .collect::<Vec<_>>();
    let channels=config.get("channels").and_then(toml::Value::as_array).cloned().unwrap_or_default().iter().map(|channel|json!({"kind":channel.get("kind").and_then(toml::Value::as_str).unwrap_or("unknown"),"detail":"Configured","enabled":true})).collect::<Vec<_>>();
    json!({"host":{"doctor":doctor,"channels":channels,"tools":[],"skills":[],"credentials":credentials,"limits":config.get("limits").cloned().unwrap_or(toml::Value::Table(Default::default())),"ceiling":{}}})
}

pub fn scope_summary(scope: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(mounts) = scope["mounts"].as_array() {
        for mount in mounts {
            let mode = mount["mode"].as_str().unwrap_or("ro");
            let path = mount["path"].as_str().unwrap_or("/");
            parts.push(format!(
                "{} {path}",
                if mode == "rw" { "Edits" } else { "Reads" }
            ));
        }
    }
    if let Some(credentials) = scope["credentials"].as_array() {
        if credentials
            .iter()
            .any(|item| item.as_str() == Some("web-search"))
        {
            parts.push("can search the web".into());
        } else if !credentials.is_empty() {
            parts.push(format!("uses {} credentials", credentials.len()));
        }
    }
    if parts.is_empty() {
        "No additional reach".into()
    } else {
        parts.join(", ")
    }
}

pub fn approvals(home: &Path, state: &str) -> Result<Value, String> {
    if !matches!(state, "pending" | "decided" | "expired") {
        return Err("invalid approval state".into());
    }
    let db = supervisor(home)?;
    let clause = match state {
        "pending" => "status='pending'",
        "decided" => "status IN ('approved','denied') AND created_at >= datetime('now','-1 day')",
        _ => "status IN ('timed_out','expired') AND created_at >= datetime('now','-1 day')",
    };
    let sql = format!(
        "SELECT id,code,root_session_id,requesting_session_id,kind,payload,status,expires_at,created_at,resolved_at,reissued_as FROM approvals WHERE {clause} ORDER BY created_at DESC"
    );
    let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], approval_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let approvals = rows
        .into_iter()
        .map(|row| approval_value(home, row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"approvals":approvals}))
}

pub fn pending_approvals(home: &Path) -> Result<i64, String> {
    let db = supervisor(home)?;
    db.query_row(
        "SELECT COUNT(*) FROM approvals WHERE status='pending'",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

type ApprovalRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);
fn approval_row(row: &Row<'_>) -> rusqlite::Result<ApprovalRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}
fn approval_value(home: &Path, row: ApprovalRow) -> Result<Value, String> {
    let (id, code, root, requester, kind, payload, status, expires, created, resolved, reissued) =
        row;
    let payload: Value = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
    let db = supervisor(home)?;
    let role: String = db
        .query_row(
            "SELECT role FROM sessions WHERE id=?1",
            [&requester],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let tool_call=(kind=="tool_call").then(||json!({"server":payload["binding"]["server"],"tool":payload["binding"]["tool"],"arguments":payload["arguments"],"argument_hash":payload["binding"]["argument_hash"]}));
    let spawn = (kind == "spawn").then(
        || json!({"role":payload["role"],"task":payload["task"],"requested":payload["scope"]}),
    );
    Ok(
        json!({"id":id,"code":code,"state":status,"kind":kind,"root_session_id":root,"requesting_session_id":requester,"requesting_role":role,"expires_at":expires,"created_at":created,"resolved_at":resolved,"reissued_as":reissued,"tool_call":tool_call,"spawn":spawn}),
    )
}
