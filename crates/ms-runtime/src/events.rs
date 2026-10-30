use ms_proto::{SupervisorEvent, SupervisorEventType};
use ms_store::SupervisorStore;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub fn emit_supervisor_event(
    store: &mut SupervisorStore,
    session_id: &str,
    root_session_id: &str,
    role: &str,
    event_type: SupervisorEventType,
    data: Value,
) -> Result<(), String> {
    let ts_mono = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX);
    store
        .append_supervisor_event(&SupervisorEvent {
            seq: 0,
            ts: crate::utc_now_precise(),
            ts_mono,
            session_id: session_id.into(),
            root_session_id: root_session_id.into(),
            agent_role: role.into(),
            source: Default::default(),
            event_type,
            data,
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[derive(Clone)]
struct EventRow {
    order_time: f64,
    source: &'static str,
    source_seq: i64,
    value: Value,
}

pub async fn stream<W: AsyncWrite + Unpin>(
    home: &Path,
    request: &Value,
    writer: &mut W,
) -> Result<(), String> {
    let follow = request
        .get("follow")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut cursors = request
        .get("after")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(session, seq)| seq.as_i64().map(|seq| (session.clone(), seq)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    loop {
        for event in merged_events(home)? {
            let session = event["session_id"]
                .as_str()
                .ok_or("event has no session_id")?
                .to_owned();
            let seq = event["seq"].as_i64().ok_or("event has no seq")?;
            if seq <= cursors.get(&session).copied().unwrap_or_default() {
                continue;
            }
            let mut encoded = serde_json::to_vec(&event).map_err(|error| error.to_string())?;
            encoded.push(b'\n');
            writer
                .write_all(&encoded)
                .await
                .map_err(|error| error.to_string())?;
            writer.flush().await.map_err(|error| error.to_string())?;
            cursors.insert(session, seq);
        }
        if !follow {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn merged_events(home: &Path) -> Result<Vec<Value>, String> {
    let db_path = home.join("supervisor.db");
    let database = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    let mut rows = Vec::<EventRow>::new();
    {
        let mut statement = database
            .prepare("SELECT seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data FROM supervisor_events")
            .map_err(|error| error.to_string())?;
        let events = statement
            .query_map([], |row| {
                let seq: i64 = row.get(0)?;
                let ts: String = row.get(1)?;
                let ts_mono: i64 = row.get(2)?;
                let session: String = row.get(3)?;
                let root: String = row.get(4)?;
                let role: String = row.get(5)?;
                let kind: String = row.get(6)?;
                let data: String = row.get(7)?;
                Ok((seq, ts, ts_mono, session, root, role, kind, data))
            })
            .map_err(|error| error.to_string())?;
        for row in events {
            let (seq, ts, ts_mono, session, root, role, kind, data) =
                row.map_err(|error| error.to_string())?;
            rows.push(EventRow {
                order_time: wall_time(&database, &ts)?,
                source: "supervisor",
                source_seq: seq,
                value: json!({"seq":seq,"ts":ts,"ts_mono":ts_mono,"session_id":session,
                    "root_session_id":root,"agent_role":role,"source":"supervisor","type":kind,
                    "data":serde_json::from_str::<Value>(&data).map_err(|error| error.to_string())?}),
            });
        }
    }
    let sessions = {
        let mut statement = database
            .prepare("SELECT id,root_id,role FROM sessions ORDER BY id")
            .map_err(|error| error.to_string())?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for (session, root, role) in sessions {
        let path: PathBuf = home.join("sessions").join(&session).join("out/events.db");
        if !path.is_file() {
            continue;
        }
        let events_db = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut statement = events_db
            .prepare(
                "SELECT seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data FROM events",
            )
            .map_err(|error| error.to_string())?;
        let events = statement
            .query_map([], |row| {
                let seq: i64 = row.get(0)?;
                let ts: String = row.get(1)?;
                let ts_mono: i64 = row.get(2)?;
                let session: String = row.get(3)?;
                let root: String = row.get(4)?;
                let role: String = row.get(5)?;
                let kind: String = row.get(6)?;
                let data: String = row.get(7)?;
                Ok((seq, ts, ts_mono, session, root, role, kind, data))
            })
            .map_err(|error| error.to_string())?;
        for row in events {
            let (seq, ts, ts_mono, claimed_session, claimed_root, claimed_role, kind, data) =
                row.map_err(|error| error.to_string())?;
            if claimed_session != session || claimed_root != root || claimed_role != role {
                continue;
            }
            let Ok(order_time) = wall_time(&database, &ts) else {
                continue;
            };
            rows.push(EventRow {
                order_time,
                source: "agent",
                source_seq: seq,
                value: json!({"seq":seq,"ts":ts,"ts_mono":ts_mono,"session_id":session,
                    "root_session_id":root,"agent_role":role,"source":"agent","type":kind,
                    "data":serde_json::from_str::<Value>(&data).map_err(|error| error.to_string())?}),
            });
        }
    }
    rows.sort_by(|left, right| {
        left.order_time
            .total_cmp(&right.order_time)
            .then(left.source.cmp(right.source))
            .then(left.source_seq.cmp(&right.source_seq))
            .then(
                left.value["session_id"]
                    .as_str()
                    .cmp(&right.value["session_id"].as_str()),
            )
    });
    let ledger_path = home.join("event-stream.db");
    let mut ledger = Connection::open(&ledger_path).map_err(|error| error.to_string())?;
    std::fs::set_permissions(&ledger_path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    ledger
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    ledger.execute_batch("CREATE TABLE IF NOT EXISTS merged_events (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, seq INTEGER NOT NULL, source TEXT NOT NULL, source_seq INTEGER NOT NULL, body TEXT NOT NULL, UNIQUE(session_id,seq), UNIQUE(session_id,source,source_seq));")
        .map_err(|error| error.to_string())?;
    let transaction = ledger.transaction().map_err(|error| error.to_string())?;
    let mut sequence = BTreeMap::<String, i64>::new();
    for mut row in rows {
        let session = row.value["session_id"]
            .as_str()
            .ok_or("event has no session_id")?
            .to_owned();
        let next = if let Some(last) = sequence.get(&session) {
            *last
        } else {
            transaction
                .query_row(
                    "SELECT COALESCE(MAX(seq),0) FROM merged_events WHERE session_id=?1",
                    [&session],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?
        };
        let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM merged_events WHERE session_id=?1 AND source=?2 AND source_seq=?3)", rusqlite::params![session,row.source,row.source_seq], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if exists {
            sequence.insert(session, next);
            continue;
        }
        row.value["seq"] = json!(next + 1);
        transaction.execute("INSERT INTO merged_events(session_id,seq,source,source_seq,body) VALUES (?1,?2,?3,?4,?5)", rusqlite::params![session,next+1,row.source,row.source_seq,row.value.to_string()])
            .map_err(|error| error.to_string())?;
        sequence.insert(session, next + 1);
    }
    transaction.commit().map_err(|error| error.to_string())?;
    let mut statement = ledger
        .prepare("SELECT body FROM merged_events ORDER BY id")
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .map(|row| {
            serde_json::from_str(&row.map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn wall_time(db: &Connection, ts: &str) -> Result<f64, String> {
    db.query_row("SELECT julianday(?1)", [ts], |row| row.get(0))
        .map_err(|error| error.to_string())
}
