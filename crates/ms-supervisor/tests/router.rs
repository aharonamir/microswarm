use ms_proto::{InboundKind, OutboundKind};
use ms_store::{SessionRecord, SupervisorStore, open_agent_session_dir, open_session_dir};
use ms_supervisor::router::{
    append_inbound, outbound_after, outbound_data_version, resolve_or_create,
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ms-router-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn record(id: &str) -> SessionRecord {
    SessionRecord {
        id: id.into(),
        root_id: id.into(),
        parent_id: None,
        role: "lead".into(),
        kind: "lead".into(),
        channel: None,
        channel_thread: None,
        status: "pending".into(),
        scope_hash: None,
        scope: None,
        created_at: "2026-10-19T00:00:00Z".into(),
        last_active_at: "2026-10-19T00:00:00Z".into(),
    }
}

#[test]
fn router_reuses_channel_thread_appends_inbound_and_tails_new_replies() {
    let home = temp_dir();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    let first = resolve_or_create(&mut store, "cli", "thread-1", || {
        record("01J00000000000000000000000")
    })
    .unwrap();
    let second = resolve_or_create(&mut store, "cli", "thread-1", || {
        panic!("same thread must reuse the lead")
    })
    .unwrap();
    assert_eq!(first, second);

    let session_dir = home.join("sessions").join(&first);
    let supervisor_session = open_session_dir(&session_dir).unwrap();
    let inbound_id = append_inbound(
        &supervisor_session,
        InboundKind::UserMessage,
        json!({"text":"hello"}),
    )
    .unwrap();
    assert_eq!(inbound_id, 1);
    let agent_session = open_agent_session_dir(&session_dir).unwrap();
    let version_before = outbound_data_version(&supervisor_session).unwrap();
    agent_session.outbound().execute(
        "INSERT INTO outbound(kind, body, in_reply_to) VALUES ('reply', '{\"text\":\"hello back\"}', ?1)",
        [inbound_id],
    ).unwrap();
    assert_ne!(
        outbound_data_version(&supervisor_session).unwrap(),
        version_before
    );
    let replies = outbound_after(&supervisor_session, 0).unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].kind, OutboundKind::Reply);
    assert_eq!(replies[0].body["text"], "hello back");
    ms_supervisor::router::persist_tail_cursor(&mut store, &first, replies[0].id.unwrap(), 7)
        .unwrap();
    assert_eq!(
        ms_supervisor::router::tail_cursor(&store, &first).unwrap(),
        (1, 7)
    );
    drop(agent_session);
    drop(supervisor_session);
    drop(store);
    let store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    assert_eq!(
        ms_supervisor::router::tail_cursor(&store, &first).unwrap(),
        (1, 7)
    );
    drop(store);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn member_route_only_returns_members_owned_by_the_requested_lead() {
    let home = temp_dir();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    store.insert_session(&record("lead-a")).unwrap();
    let mut lead_b = record("lead-b");
    lead_b.root_id = "lead-b".into();
    store.insert_session(&lead_b).unwrap();
    store
        .insert_session(&SessionRecord {
            id: "member-a".into(),
            root_id: "lead-a".into(),
            parent_id: Some("lead-a".into()),
            role: "researcher".into(),
            kind: "member".into(),
            channel: None,
            channel_thread: None,
            status: "ready".into(),
            scope_hash: None,
            scope: None,
            created_at: "2026-10-19T00:00:00Z".into(),
            last_active_at: "2026-10-19T00:00:00Z".into(),
        })
        .unwrap();
    assert!(store.member_route("lead-a", "member-a").unwrap().is_some());
    assert!(store.member_route("lead-b", "member-a").unwrap().is_none());
    drop(store);
    fs::remove_dir_all(home).unwrap();
}
