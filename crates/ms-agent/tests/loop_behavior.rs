use ms_agent::{
    agent_loop::{self, TurnError, TurnOutput, TurnProcessor},
    boot::AgentContext,
};
use ms_proto::{InboundMessage, Scope};
use ms_store::{SessionStore, open_session_dir};
use serde_json::json;
use std::{future::Future, path::PathBuf, pin::Pin};

struct MustNotProcess;
impl TurnProcessor for MustNotProcess {
    fn process<'a>(
        &'a mut self,
        _: &'a mut AgentContext,
        _: InboundMessage,
        _: Vec<ms_store::HistoryEntry>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnOutput, TurnError>> + 'a>> {
        Box::pin(async { Err(TurnError::Transient("non-user input reached model".into())) })
    }
}

#[test]
fn unsupported_and_malformed_inbound_are_acknowledged_with_error_events() {
    let dir = std::env::temp_dir().join(format!("ms-loop-invalid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let supervisor = open_session_dir(&dir).unwrap();
    for kind in [
        "approval_result",
        "task",
        "member_reply",
        "delegation_result",
        "member_failed",
    ] {
        supervisor
            .inbound()
            .execute("INSERT INTO inbound(kind, body) VALUES (?1, '{}')", [kind])
            .unwrap();
    }
    supervisor
        .inbound()
        .execute(
            "INSERT INTO inbound(kind, body) VALUES ('user_message', '{}')",
            [],
        )
        .unwrap();
    supervisor
        .inbound()
        .execute(
            "INSERT INTO inbound(kind, body) VALUES ('control', '{\"op\":\"shutdown\"}')",
            [],
        )
        .unwrap();
    let scope: Scope = serde_json::from_value(json!({
        "version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config",
        "source_ref":"test","mounts":[],"credentials":[],"tools":[],"delegation":null,
        "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},
        "narrowed":[],"hash":"placeholder"
    }))
    .unwrap();
    let context = AgentContext {
        session_id: scope.session_id.clone(),
        root_session_id: scope.session_id.clone(),
        kind: "lead".into(),
        role: "lead".into(),
        proxy_url: None,
        scope,
        store: SessionStore::open_dir(PathBuf::from(&dir)).unwrap(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(
        runtime
            .block_on(agent_loop::run(context, MustNotProcess))
            .is_ok()
    );
    let cursor: i64 = supervisor
        .outbound()
        .query_row(
            "SELECT last_inbound_id FROM cursor WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cursor, 7);
    let errors: i64 = supervisor
        .events()
        .query_row(
            "SELECT count(*) FROM events WHERE type='inbound.error'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(errors, 6);
    drop(supervisor);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn non_transient_store_failures_do_not_use_transient_exit_code() {
    let error =
        ms_agent::agent_loop::LoopError::Store(ms_store::StoreError::InvalidContract("test"));
    assert_eq!(error.exit_code(), 77);
}
