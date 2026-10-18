use ms_proto::Scope;
use ms_store::open_session_dir;
use serde_json::json;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn mismatched_scope_hash_exits_with_config_code() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let session_dir = std::env::temp_dir().join(format!(
        "microswarm-agent-boot-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir_all(&session_dir).unwrap();
    fs::create_dir_all(session_dir.join("control")).unwrap();
    fs::write(
        session_dir.join("control/scope.json"),
        r#"{
          "version": 1,
          "session_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
          "derived_from": "config",
          "source_ref": "config",
          "mounts": [],
          "credentials": [],
          "tools": [],
          "limits": {"memory_mb": 256, "pids": 64, "tmp_mb": 64, "max_turns": 20, "max_tokens": 4096},
          "narrowed": [],
          "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_microswarm-agent"))
        .env("MS_SESSION_ID", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
        .env("MS_KIND", "lead")
        .env("MS_ROLE", "lead")
        .env("MS_SESSION_DIR", &session_dir)
        .output()
        .unwrap();

    let _ = fs::remove_dir_all(session_dir);
    assert_eq!(output.status.code(), Some(78));
}

#[test]
fn role_kind_is_separate_from_event_role_name() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let session_dir = std::env::temp_dir().join(format!(
        "microswarm-agent-role-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir_all(session_dir.join("control")).unwrap();
    let host = open_session_dir(&session_dir).unwrap();
    let mut scope: Scope = serde_json::from_value(json!({
        "version":1, "session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV", "derived_from":"delegation",
        "source_ref":"fixture", "mounts":[], "credentials":[], "tools":[], "delegation":null,
        "limits":{"memory_mb":256,"pids":64,"tmp_mb":64,"max_turns":20,"max_tokens":4096},
        "narrowed":[], "hash":"placeholder"
    }))
    .unwrap();
    scope.update_hash();
    fs::write(
        session_dir.join("control/scope.json"),
        serde_json::to_vec(&scope).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_microswarm-agent"))
        .env("MS_SESSION_ID", &scope.session_id)
        .env("MS_KIND", "member")
        .env("MS_ROLE", "researcher")
        .env("MS_SESSION_DIR", &session_dir)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(78),
        "model config is intentionally absent after boot"
    );
    let agent_role: String = host
        .events()
        .query_row(
            "SELECT agent_role FROM events WHERE type='agent.ready'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(agent_role, "researcher");
    drop(host);
    let _ = fs::remove_dir_all(session_dir);
}
