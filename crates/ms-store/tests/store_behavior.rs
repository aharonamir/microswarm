use ms_proto::{
    AgentEvent, AgentEventType, OutboundKind, OutboundMessage, SupervisorEvent, SupervisorEventType,
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

fn append_inbound(connection: &Connection, text: &str) -> i64 {
    connection
        .execute(
            "INSERT INTO inbound(kind, body) VALUES ('user_message', ?1)",
            [json!({"text": text}).to_string()],
        )
        .unwrap();
    connection.last_insert_rowid()
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
        2
    );
    drop(db);

    let old_path = dir.join("old-supervisor.db");
    let old = Connection::open(&old_path).unwrap();
    old.execute_batch(
        "CREATE TABLE sessions(id TEXT PRIMARY KEY, scope_hash TEXT); PRAGMA user_version=1;",
    )
    .unwrap();
    drop(old);
    let migrated = open_supervisor_db(&old_path).unwrap();
    assert_eq!(
        migrated
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let has_scope_json: bool = migrated
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('sessions') WHERE name='scope_json')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(has_scope_json);

    let bad_path = dir.join("bad.db");
    let bad = Connection::open(&bad_path).unwrap();
    bad.execute_batch("PRAGMA user_version=99").unwrap();
    drop(bad);
    assert!(open_supervisor_db(bad_path).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn agent_session_opens_control_database_read_only_and_writes_only_under_out() {
    let dir = temp_dir("split-session");
    let supervisor = open_session_dir(&dir).unwrap();
    supervisor
        .inbound()
        .execute(
            "INSERT INTO inbound(kind, body) VALUES ('user_message', '{\"text\":\"hello\"}')",
            [],
        )
        .unwrap();
    let agent = ms_store::open_agent_session_dir(&dir).unwrap();
    assert_eq!(
        agent
            .inbound()
            .query_row("SELECT count(*) FROM inbound", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(agent.inbound().execute(
        "INSERT INTO inbound(kind, body) VALUES ('approval_result', '{\"decision\":\"approved\"}')",
        [],
    ).is_err(), "read-only agent connection must reject forged control rows");
    assert!(dir.join("control/inbound.db").exists());
    assert!(dir.join("out/outbound.db").exists());
    assert!(dir.join("out/events.db").exists());
    drop(agent);
    drop(supervisor);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unknown_inbound_kind_is_read_and_can_be_acknowledged() {
    let dir = temp_dir("unknown-inbound");
    let supervisor = open_session_dir(&dir).unwrap();
    supervisor
        .inbound()
        .execute_batch("PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    supervisor
        .inbound()
        .execute(
            "INSERT INTO inbound(kind, body) VALUES ('future_kind', '{\"value\":1}')",
            [],
        )
        .unwrap();
    let id = supervisor.inbound().last_insert_rowid();
    let mut agent = SessionStore::open_dir(&dir).unwrap();
    let message = agent.next_inbound(0).unwrap().unwrap();
    assert_eq!(
        message.kind,
        ms_proto::InboundKind::Unknown("future_kind".into())
    );
    agent.commit_turn(id, &[], &[]).unwrap();
    assert_eq!(agent.cursor().unwrap(), id);
    drop(agent);
    drop(supervisor);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn session_store_commits_outbound_and_cursor_together_and_appends_gapless_events() {
    let dir = temp_dir("session");
    let supervisor = open_session_dir(&dir).unwrap();
    let mut store = SessionStore::open_dir(&dir).unwrap();
    let inbound_id = append_inbound(supervisor.inbound(), "hello");

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
    drop(supervisor);

    let outbound = Connection::open(dir.join("out/outbound.db")).unwrap();
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
    let events = Connection::open(dir.join("out/events.db")).unwrap();
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
fn session_store_reads_the_next_unprocessed_inbound_in_id_order() {
    let dir = temp_dir("inbound-cursor");
    let supervisor = open_session_dir(&dir).unwrap();
    let store = SessionStore::open_dir(&dir).unwrap();
    let first_id = append_inbound(supervisor.inbound(), "first");
    let second_id = append_inbound(supervisor.inbound(), "second");

    assert_eq!(store.next_inbound(0).unwrap().unwrap().id, Some(first_id));
    assert_eq!(
        store.next_inbound(first_id).unwrap().unwrap().id,
        Some(second_id)
    );
    assert!(store.next_inbound(second_id).unwrap().is_none());
    drop(store);
    drop(supervisor);
    let _ = std::fs::remove_dir_all(dir);
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
            scope_hash: Some("sha256:authoritative".into()),
            scope: Some(json!({"credentials":["model-key"],"hash":"sha256:authoritative"})),
            created_at: "now".into(),
            last_active_at: "now".into(),
        })
        .unwrap();
    let db = open_supervisor_db(dir.join("supervisor.db")).unwrap();
    let (scope_hash, scope_json): (String, String) = db
        .query_row(
            "SELECT scope_hash, scope_json FROM sessions WHERE id=?1",
            ["01ARZ3NDEKTSV4RRFFQ69G5FAV"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(scope_hash, "sha256:authoritative");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&scope_json).unwrap()["credentials"][0],
        "model-key"
    );
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
