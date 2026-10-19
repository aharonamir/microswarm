use ms_proto::{ApprovalBinding, InboundKind};
use ms_store::{SessionRecord, SupervisorStore, open_session_dir};
use ms_supervisor::approvals::{request_tool_approval, resolve_by_code};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ms-approval-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn record() -> SessionRecord {
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
#[ignore = "red: T042 — approval persistence, resolution, and inbound delivery are not implemented"]
fn approval_code_resolves_once_and_inbound_carries_exact_call_binding() {
    let dir = temp_dir();
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    let session_record = record();
    store.insert_session(&session_record).unwrap();
    let session = open_session_dir(dir.join("sessions").join(&session_record.id)).unwrap();
    let args = json!({"command":"echo hi"});
    let binding = ApprovalBinding::for_call("microswarm-tools", "shell.run", Some(&args));
    let approval = request_tool_approval(
        &mut store,
        &session_record.id,
        &session_record.root_id,
        &binding,
        Duration::from_secs(600),
    )
    .unwrap();
    assert_eq!(approval.code.len(), 4);
    assert_eq!(
        approval.payload["binding"],
        serde_json::to_value(&binding).unwrap()
    );
    resolve_by_code(&mut store, &session, &approval.code, true).unwrap();
    let (kind, body): (String, String) = session
        .inbound()
        .query_row(
            "SELECT kind, body FROM inbound ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<InboundKind>(&format!("\"{kind}\"")).unwrap(),
        InboundKind::ApprovalResult
    );
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["approval_id"], approval.id);
    assert_eq!(body["decision"], "approved");
    assert_eq!(body["binding"], serde_json::to_value(&binding).unwrap());
    assert!(resolve_by_code(&mut store, &session, &approval.code, true).is_err());
    drop(session);
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
