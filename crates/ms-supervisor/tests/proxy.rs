use ms_proto::{DerivedFrom, Limits, Scope};
use ms_store::{SessionRecord, SupervisorStore};
use ms_supervisor::proxy::SessionProxy;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn session() -> SessionRecord {
    SessionRecord {
        id: "01J00000000000000000000000".into(),
        root_id: "01J00000000000000000000000".into(),
        parent_id: None,
        role: "lead".into(),
        kind: "lead".into(),
        channel: None,
        channel_thread: None,
        status: "ready".into(),
        scope_hash: None,
        scope: None,
        created_at: "2026-10-19T00:00:00Z".into(),
        last_active_at: "2026-10-19T00:00:00Z".into(),
    }
}

#[test]
#[ignore = "red: T041 — credential proxy authorization and forwarding are not implemented"]
fn proxy_uses_the_authoritative_scope_and_allows_only_named_credentials() {
    let dir = std::env::temp_dir().join(format!(
        "ms-proxy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    let mut record = session();
    let scope = Scope {
        version: 1,
        session_id: record.id.clone(),
        derived_from: DerivedFrom::Config,
        source_ref: "config".into(),
        mounts: vec![],
        credentials: vec!["model-key".into()],
        tools: vec![],
        delegation: None,
        limits: Limits {
            memory_mb: 256,
            pids: 64,
            tmp_mb: 128,
            max_turns: 30,
            max_tokens: 65_536,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    let mut scope = scope;
    scope.update_hash();
    record.scope_hash = Some(scope.hash.clone());
    record.scope = Some(serde_json::to_value(&scope).unwrap());
    store.insert_session(&record).unwrap();
    let proxy = SessionProxy::from_store(&store, &record.id).unwrap();
    assert!(proxy.credential_allowed(Some("model-key")));
    assert!(!proxy.credential_allowed(Some("other-key")));
    assert!(!proxy.credential_allowed(None));
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
