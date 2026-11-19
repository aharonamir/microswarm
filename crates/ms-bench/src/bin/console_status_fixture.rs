//! Test fixture that creates console view scenarios and streams committed events.
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::BufRead,
    path::{Path, PathBuf},
    thread,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::mpsc,
};

const SESSION_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const RESEARCHER_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const TESTER_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
const MEMBER_IDS: [&str; 6] = [
    RESEARCHER_ID,
    TESTER_ID,
    "01ARZ3NDEKTSV4RRFFQ69G5FAY",
    "01ARZ3NDEKTSV4RRFFQ69G5FAZ",
    "01ARZ3NDEKTSV4RRFFQ69G5FB0",
    "01ARZ3NDEKTSV4RRFFQ69G5FB1",
];
const SUPERVISOR_SCHEMA: &str =
    include_str!("../../../../specs/001-microswarm/contracts/supervisor-db.sql");
const SESSION_SCHEMA: &str =
    include_str!("../../../../specs/001-microswarm/contracts/session-db.sql");

fn init_home(home: &Path, scenario: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    if home.exists() {
        fs::remove_dir_all(home)?;
    }
    fs::create_dir_all(home.join("sessions"))?;
    let db = Connection::open(home.join("supervisor.db"))?;
    db.execute_batch("PRAGMA foreign_keys=ON;")?;
    db.execute_batch(SUPERVISOR_SCHEMA)?;
    db.execute_batch("PRAGMA user_version=3;")?;
    let now = ms_runtime::utc_now();
    let lead_scope = if scenario == "default" {
        "{}".to_owned()
    } else {
        fixture_scope(SESSION_ID, "rw", false)
    };
    insert_session(
        &db,
        SESSION_ID,
        SESSION_ID,
        None,
        "lead",
        "lead",
        Some("web"),
        "ready",
        &now,
        &lead_scope,
    )?;
    init_session_events(home, SESSION_ID, SESSION_ID, "lead")?;
    if scenario != "default" {
        Connection::open(
            home.join("sessions")
                .join(SESSION_ID)
                .join("control/inbound.db"),
        )?
        .execute(
            "INSERT INTO inbound(kind,body) VALUES('user_message',?1)",
            [json!({"text":"Audit the auth module"}).to_string()],
        )?;
    }

    if scenario == "demo" || scenario == "hostile" || scenario == "load" {
        let count = if scenario == "load" { 6 } else { 2 };
        for index in 0..count {
            let id = MEMBER_IDS[index];
            let role = if scenario == "hostile" {
                "<img src=x onerror=alert(1)>"
            } else {
                match index {
                    0 => "researcher",
                    1 => "tester",
                    n => ["analyst", "builder", "reviewer", "writer"][n - 2],
                }
            };
            let task = if scenario == "hostile" {
                "<script>alert('hostile')</script>"
            } else {
                [
                    "Inspect the authentication refresh path",
                    "Run the auth tests and report failures",
                    "Review the session store",
                    "Check the API handlers",
                    "Inspect docs",
                    "Review security tests",
                ][index]
            };
            let granted_scope = if scenario == "default" {
                "{}".to_owned()
            } else {
                fixture_scope(id, "ro", index <= 1)
            };
            let requested_scope = if scenario == "default" {
                "{\"mounts\":[{\"path\":\"/work\",\"mode\":\"rw\"}]}".to_owned()
            } else {
                fixture_scope(id, "rw", false)
            };
            insert_session(
                &db,
                id,
                SESSION_ID,
                Some(SESSION_ID),
                role,
                "member",
                None,
                "busy",
                &now,
                &granted_scope,
            )?;
            db.execute("INSERT INTO delegations(id,lead_id,member_id,role,task,scope_request,policy,timeout_s,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'allow',600,'active',?7,?7)", params![format!("D{:03}", index+1), SESSION_ID, id, role, task, requested_scope, now])?;
            init_session_events(home, id, SESSION_ID, role)?;
        }
        seed_lead_events(home, &db, &now)?;
        let mut seq = 1i64;
        for index in 0..count {
            let id = MEMBER_IDS[index];
            let role = db.query_row("SELECT role FROM sessions WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })?;
            seed_member_events(home, id, SESSION_ID, &role, &mut seq, scenario, &now)?;
        }
        if scenario == "demo" {
            seed_approval(&db, &offset_timestamp(&now, 20)?)?;
            seed_ended_and_telegram(&db, home, &now)?;
        }
        if scenario == "load" {
            let mut seq = 100;
            for index in 0..1982 {
                let id = MEMBER_IDS[index % count];
                let role = if index % count == 0 {
                    "researcher"
                } else if index % count == 1 {
                    "tester"
                } else {
                    "reviewer"
                };
                append_agent_event(
                    home,
                    id,
                    SESSION_ID,
                    role,
                    seq,
                    "tool.call",
                    json!({"call_id":format!("call-{index}"),"server":"filesystem","tool":"fs.read","args":{"path":format!("/work/src/file-{index}.rs")}}),
                    ms_runtime::utc_now(),
                )?;
                seq += 1;
            }
        }
    }
    Ok(db)
}

fn insert_session(
    db: &Connection,
    id: &str,
    root: &str,
    parent: Option<&str>,
    role: &str,
    kind: &str,
    channel: Option<&str>,
    status: &str,
    now: &str,
    scope: &str,
) -> Result<(), rusqlite::Error> {
    db.execute("INSERT INTO sessions(id,root_id,parent_id,role,kind,channel,status,scope_json,created_at,last_active_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)", params![id,root,parent,role,kind,channel,status,scope,now])?;
    Ok(())
}

fn fixture_scope(id: &str, mode: &str, narrowed: bool) -> String {
    let reductions = if narrowed {
        json!([{"dimension":"mount","item":"/work","reason":"mode_downgraded"}])
    } else {
        json!([])
    };
    json!({
        "version": 1,
        "session_id": id,
        "derived_from": "delegation",
        "source_ref": "code-review-team@1.2.0",
        "mounts": [{"host_path":"/tmp","container_path":"/work","mode":mode}],
        "credentials": [],
        "tools": [],
        "delegation": null,
        "limits": {"memory_mb":128,"pids":32,"tmp_mb":32,"max_turns":20,"max_tokens":4000},
        "narrowed": reductions,
        "hash": format!("sha256:{}", "0".repeat(64))
    })
    .to_string()
}

fn init_session_events(
    home: &Path,
    id: &str,
    root: &str,
    role: &str,
) -> Result<Connection, Box<dyn std::error::Error>> {
    let path = home.join("sessions").join(id).join("out");
    fs::create_dir_all(&path)?;
    let control = home.join("sessions").join(id).join("control");
    fs::create_dir_all(&control)?;
    let inbound = Connection::open(control.join("inbound.db"))?;
    let inbound_start = SESSION_SCHEMA
        .find("CREATE TABLE inbound (")
        .ok_or("inbound schema missing")?;
    let inbound_end = SESSION_SCHEMA[inbound_start..]
        .find("CREATE TABLE outbound (")
        .ok_or("inbound schema end missing")?
        + inbound_start;
    inbound.execute_batch(&SESSION_SCHEMA[inbound_start..inbound_end])?;
    drop(inbound);
    let db = Connection::open(path.join("events.db"))?;
    let start = SESSION_SCHEMA
        .find("CREATE TABLE events (")
        .ok_or("events schema missing")?;
    let end = SESSION_SCHEMA[start..]
        .find("-- ───────────────────────── Body shapes")
        .ok_or("events schema end missing")?
        + start;
    db.execute_batch(&SESSION_SCHEMA[start..end])?;
    db.execute_batch("PRAGMA user_version=1;")?;
    let _ = (root, role);
    Ok(db)
}

fn offset_timestamp(base: &str, seconds: i64) -> Result<String, rusqlite::Error> {
    let db = Connection::open_in_memory()?;
    db.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1,?2)",
        [base, &format!("+{seconds} seconds")],
        |row| row.get(0),
    )
}

fn append_agent_event(
    home: &Path,
    id: &str,
    root: &str,
    role: &str,
    seq: i64,
    kind: &str,
    data: Value,
    ts: String,
) -> Result<Value, Box<dyn std::error::Error>> {
    let db = Connection::open(home.join("sessions").join(id).join("out/events.db"))?;
    let mono = seq.saturating_mul(1000);
    db.execute("INSERT INTO events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![seq,ts,mono,id,root,role,kind,data.to_string()])?;
    Ok(
        json!({"seq":seq,"ts":ts,"ts_mono":mono,"session_id":id,"root_session_id":root,"agent_role":role,"source":"agent","type":kind,"data":data}),
    )
}

fn seed_lead_events(
    home: &Path,
    db: &Connection,
    now: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for (seq, offset, kind, data) in [
        (1, 0, "turn.start", json!({})),
        (
            2,
            1,
            "delegation.request",
            json!({"delegation_id":"D001","role":"researcher","task":"Inspect the authentication refresh path","scope_request":{"mounts":[{"path":"/work","mode":"rw"}]}}),
        ),
        (
            3,
            2,
            "delegation.request",
            json!({"delegation_id":"D002","role":"tester","task":"Run the auth tests and report failures","scope_request":{"mounts":[{"path":"/work","mode":"rw"}]}}),
        ),
        (4, 6, "turn.end", json!({})),
    ] {
        append_agent_event(
            home,
            SESSION_ID,
            SESSION_ID,
            "lead",
            seq,
            kind,
            data,
            offset_timestamp(now, offset)?,
        )?;
    }
    for (seq, offset, kind, data) in [
        (
            100,
            3,
            "delegation.spawned",
            json!({"delegation_id":"D001","member_id":RESEARCHER_ID,"role":"researcher","task":"Inspect the authentication refresh path"}),
        ),
        (
            101,
            4,
            "delegation.spawned",
            json!({"delegation_id":"D002","member_id":TESTER_ID,"role":"tester","task":"Run the auth tests and report failures"}),
        ),
        (
            102,
            5,
            "scope.narrowed",
            json!({"delegation_id":"D001","reason":"mode_downgraded","dimension":"mounts","requested":"rw","granted":"ro","narrowed":[{"dimension":"mount","item":"/work","reason":"mode_downgraded"}]}),
        ),
        (
            103,
            5,
            "scope.narrowed",
            json!({"delegation_id":"D002","reason":"mode_downgraded","dimension":"mounts","requested":"rw","granted":"ro","narrowed":[{"dimension":"mount","item":"/work","reason":"mode_downgraded"}]}),
        ),
    ] {
        let ts = offset_timestamp(now, offset)?;
        db.execute("INSERT INTO supervisor_events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES(?1,?2,?3,?4,?4,'lead',?5,?6)", params![seq,ts,offset*1000,SESSION_ID,kind,data.to_string()])?;
    }
    Ok(())
}

fn seed_member_events(
    home: &Path,
    id: &str,
    root: &str,
    role: &str,
    seq: &mut i64,
    scenario: &str,
    base: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = offset_timestamp(base, 7)?;
    let path = if scenario == "hostile" {
        "<img src=x>"
    } else {
        "/work/src/auth/refresh.rs"
    };
    for _ in 0..if scenario == "demo" && role == "researcher" {
        3
    } else {
        1
    } {
        append_agent_event(
            home,
            id,
            root,
            role,
            *seq,
            "tool.call",
            json!({"call_id":format!("c{seq}"),"server":"filesystem","tool":"fs.read","args":{"path":path}}),
            now.clone(),
        )?;
        *seq += 1;
        append_agent_event(
            home,
            id,
            root,
            role,
            *seq,
            "permit.decision",
            json!({"call_id":format!("c{}",*seq-1),"decision":"allow","rule":"tool policy"}),
            now.clone(),
        )?;
        *seq += 1;
        append_agent_event(
            home,
            id,
            root,
            role,
            *seq,
            "tool.result",
            json!({"call_id":format!("c{}",*seq-2),"ok":true,"duration_ms":80,"result":{"text":"ok"}}),
            now.clone(),
        )?;
        *seq += 1;
    }
    if scenario == "demo" && role == "tester" {
        append_agent_event(
            home,
            id,
            root,
            role,
            *seq,
            "tool.call",
            json!({"call_id":"pending-shell","server":"shell","tool":"shell.run","args":{"command":"cargo test -p auth"}}),
            now.clone(),
        )?;
        *seq += 1;
        append_agent_event(
            home,
            id,
            root,
            role,
            *seq,
            "tool.call",
            json!({"call_id":"denied-fetch","server":"web","tool":"web.fetch","args":{"url":"https://example.com"}}),
            now.clone(),
        )?;
        *seq += 1;
        append_agent_event(
            home,
            id,
            root,
            role,
            *seq,
            "permit.decision",
            json!({"call_id":"denied-fetch","decision":"deny","rule":"tool policy"}),
            now.clone(),
        )?;
        *seq += 1;
    }
    Ok(())
}

fn seed_approval(db: &Connection, now: &str) -> Result<(), rusqlite::Error> {
    let expires = "2026-12-31T23:59:59Z";
    db.execute("INSERT INTO approvals(id,code,root_session_id,requesting_session_id,kind,payload,status,expires_at,created_at) VALUES('01ARZ3NDEKTSV4RRFFQ69G5FB2','K7QF',?1,?2,'tool_call',?3,'pending',?4,?5)", params![SESSION_ID,TESTER_ID,json!({"server":"shell","tool":"shell.run","args":{"command":"cargo test -p auth"}}).to_string(),expires,now])?;
    db.execute("INSERT INTO supervisor_events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES(120,?1,20000,?2,?2,'tester','approval.requested',?3)", params![now,SESSION_ID,json!({"approval_id":"01ARZ3NDEKTSV4RRFFQ69G5FB2","requesting_session_id":TESTER_ID,"code":"K7QF","kind":"tool_call","server":"shell","tool":"shell.run","args":{"command":"cargo test -p auth"}}).to_string()])?;
    Ok(())
}

fn seed_ended_and_telegram(
    db: &Connection,
    home: &Path,
    now: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let ended = "01ARZ3NDEKTSV4RRFFQ69G5FB3";
    insert_session(
        db,
        ended,
        ended,
        None,
        "lead",
        "lead",
        Some("web"),
        "closed",
        now,
        "{}",
    )?;
    db.execute(
        "UPDATE sessions SET closed_at=?1,close_reason='session_end' WHERE id=?2",
        params![now, ended],
    )?;
    let telegram = "01ARZ3NDEKTSV4RRFFQ69G5FB4";
    insert_session(
        db,
        telegram,
        telegram,
        None,
        "lead",
        "lead",
        Some("telegram"),
        "ready",
        now,
        "{}",
    )?;
    init_session_events(home, ended, ended, "lead")?;
    init_session_events(home, telegram, telegram, "lead")?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let home = PathBuf::from(
        args.next()
            .ok_or("usage: console_status_fixture MS_HOME [--scenario demo|hostile|load]")?,
    );
    let scenario = match (args.next().as_deref(), args.next()) {
        (Some("--scenario"), Some(value)) => value,
        _ => "default".to_owned(),
    };
    let db = init_home(&home, &scenario)?;
    let listener = UnixListener::bind(home.join("supervisor.sock"))?;
    println!("READY");
    use std::io::Write;
    std::io::stdout().flush()?;
    let (stream, _) = listener.accept().await?;
    let (read, mut write) = stream.into_split();
    let mut request = String::new();
    BufReader::new(read).read_line(&mut request).await?;
    if serde_json::from_str::<Value>(&request)?["op"] != "event_stream" {
        return Err("console follower did not request event_stream".into());
    }
    println!("CONNECTED");
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    std::io::stdout().flush()?;
    let mut pending_event = None;
    let mut next_seq = 100_000;
    while let Some(line) = receiver.recv().await {
        if let Some(role) = line.strip_prefix("emit ") {
            let (id, role) = match role {
                "researcher" => (RESEARCHER_ID, "researcher"),
                "tester" => (TESTER_ID, "tester"),
                _ => return Err("unknown fixture session role".into()),
            };
            let event = append_agent_event(
                &home,
                id,
                SESSION_ID,
                role,
                next_seq,
                "tool.result",
                json!({"call_id":format!("live-{next_seq}"),"ok":true,"duration_ms":20,"result":{"text":"fixture live step"}}),
                ms_runtime::utc_now(),
            )?;
            next_seq += 1;
            write.write_all(format!("{}\n", event).as_bytes()).await?;
            write.flush().await?;
            println!("{{\"emitted\":{}}}", event["seq"]);
            std::io::stdout().flush()?;
            continue;
        }
        let command: Value = serde_json::from_str(&line)?;
        match command["op"].as_str() {
            Some("commit") => {
                let status = command["status"].as_str().ok_or("missing status")?;
                let seq = command["seq"].as_i64().ok_or("missing seq")?;
                let ts = ms_runtime::utc_now();
                let data = json!({"status":status});
                let transaction = db.unchecked_transaction()?;
                transaction.execute(
                    "UPDATE sessions SET status=?1,last_active_at=?2 WHERE id=?3",
                    params![status, ts, SESSION_ID],
                )?;
                transaction.execute("INSERT INTO supervisor_events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES(?1,?2,?1,?3,?3,'lead','turn.started',?4)",params![seq,ts,SESSION_ID,data.to_string()])?;
                transaction.commit()?;
                pending_event = Some(
                    json!({"seq":seq,"ts":ts,"ts_mono":seq,"session_id":SESSION_ID,"root_session_id":SESSION_ID,"agent_role":"lead","source":"supervisor","type":"turn.started","data":data}),
                );
                println!("{{\"committed\":{seq}}}");
                std::io::stdout().flush()?;
            }
            Some("deliver") => {
                let event = pending_event
                    .take()
                    .ok_or("no committed event to deliver")?;
                write.write_all(format!("{}\n", event).as_bytes()).await?;
                write.flush().await?;
                println!("{{\"delivered\":{}}}", event["seq"]);
                std::io::stdout().flush()?;
            }
            _ => return Err("unknown fixture command".into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn test_home(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "ms-console-fixture-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn demo_uses_authoritative_schema_and_contains_required_scenarios() {
        let home = test_home("demo");
        let db = init_home(&home, "demo").unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sessions WHERE parent_id IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM approvals WHERE status='pending'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sessions WHERE channel='telegram'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sessions WHERE status='closed'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        drop(db);
        let reach = ms_console::read::scope(&home, TESTER_ID, true).unwrap();
        assert_eq!(reach["role"], "tester");
        assert_eq!(reach["rows"][0]["granted"][0]["mode"], "ro");
        let events = Connection::open(
            home.join("sessions")
                .join(RESEARCHER_ID)
                .join("out/events.db"),
        )
        .unwrap();
        assert_eq!(
            events
                .query_row(
                    "SELECT count(*) FROM events WHERE type='tool.call'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
        drop(events);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn load_fixture_has_two_thousand_agent_events_and_six_members() {
        let home = test_home("load");
        let db = init_home(&home, "load").unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sessions WHERE parent_id IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            6
        );
        drop(db);
        let mut count = 0;
        for id in MEMBER_IDS {
            let events =
                Connection::open(home.join("sessions").join(id).join("out/events.db")).unwrap();
            count += events
                .query_row("SELECT count(*) FROM events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0);
        }
        assert_eq!(count, 2_000);
        fs::remove_dir_all(home).unwrap();
    }
}
