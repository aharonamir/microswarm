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
    fs::write(
        session_dir.join("scope.json"),
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
        .env("MS_ROLE", "lead")
        .env("MS_SESSION_DIR", &session_dir)
        .output()
        .unwrap();

    let _ = fs::remove_dir_all(session_dir);
    assert_eq!(output.status.code(), Some(78));
}
