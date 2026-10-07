use ms_proto::{
    AgentEvent, AgentEventType, InboundKind, InboundMessage, OutboundKind, OutboundMessage,
    SupervisorEvent, SupervisorEventType,
};
use ms_store::{
    HistoryEntry, SessionRecord, SessionStore, SupervisorStore, open_session_dir,
    open_supervisor_db,
};
use rusqlite::Connection;
use serde_json::json;
use std::{fs, path::PathBuf};

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("microswarm-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn agent_event() -> AgentEvent {
    AgentEvent {
        seq: 0,
        ts: "2026-10-07T00:00:00Z".into(),
        ts_mono: 0,
        session_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        root_session_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        agent_role: "lead".into(),
        source: Default::default(),
        event_type: AgentEventType::Ready,
        data: json!({}),
    }
}

#[test]
fn open_helpers_apply_wal_and_reject_unknown_schema_versions() {
    let dir = temp_dir("open");
    let sessions = open_session_dir(&dir).unwrap();
    assert_eq!(
        sessions
            .inbound()
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    drop(sessions);

    let db = open_supervisor_db(dir.join("supervisor.db")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);

    let bad_path = dir.join("bad.db");
    let bad = Connection::open(&bad_path).unwrap();
    bad.execute_batch("PRAGMA user_version=99").unwrap();
    drop(bad);
    assert!(open_supervisor_db(bad_path).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn session_store_commits_outbound_and_cursor_together_and_appends_gapless_events() {
    let dir = temp_dir("session");
    let mut store = SessionStore::open_dir(&dir).unwrap();
    let inbound_id = store
        .append_inbound(InboundMessage {
            id: None,
            kind: InboundKind::UserMessage,
            body: json!({"text":"hello"}),
            created_at: None,
        })
        .unwrap();

    store
        .commit_turn(
            inbound_id,
            &[OutboundMessage {
                id: None,
                kind: OutboundKind::Reply,
                body: json!({"text":"hi"}),
                in_reply_to: Some(inbound_id),
                created_at: None,
            }],
            &[HistoryEntry {
                role: "assistant".into(),
                content: json!("hi"),
            }],
        )
        .unwrap();
    store.append_agent_event(&agent_event()).unwrap();
    store.append_agent_event(&agent_event()).unwrap();
    drop(store);

    let outbound = Connection::open(dir.join("outbound.db")).unwrap();
    let last_inbound: i64 = outbound
        .query_row(
            "SELECT last_inbound_id FROM cursor WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let outbound_count: i64 = outbound
        .query_row("SELECT count(*) FROM outbound", [], |r| r.get(0))
        .unwrap();
    assert_eq!(last_inbound, inbound_id);
    assert_eq!(outbound_count, 1);
    let events = Connection::open(dir.join("events.db")).unwrap();
    let seqs = events
        .prepare("SELECT seq FROM events ORDER BY seq")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(seqs, vec![1, 2]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn supervisor_store_persists_sessions_and_gapless_supervisor_events() {
    let dir = temp_dir("supervisor");
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    store
        .insert_session(&SessionRecord {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            root_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            parent_id: None,
            role: "lead".into(),
            kind: "lead".into(),
            channel: Some("cli".into()),
            channel_thread: Some("main".into()),
            status: "pending".into(),
            created_at: "now".into(),
            last_active_at: "now".into(),
        })
        .unwrap();
    let event = SupervisorEvent {
        seq: 0,
        ts: "2026-10-07T00:00:00Z".into(),
        ts_mono: 0,
        session_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        root_session_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        agent_role: "lead".into(),
        source: Default::default(),
        event_type: SupervisorEventType::SessionCreated,
        data: json!({}),
    };
    assert_eq!(store.append_supervisor_event(&event).unwrap(), 1);
    assert_eq!(store.append_supervisor_event(&event).unwrap(), 2);
    store
        .upsert_tail_cursor("01ARZ3NDEKTSV4RRFFQ69G5FAV", 5, 2)
        .unwrap();
    assert_eq!(
        store.tail_cursor("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
        Some((5, 2))
    );
    let _ = fs::remove_dir_all(dir);
}
