use ms_proto::DerivedFrom;
use ms_store::{SessionRecord, SupervisorStore};
use ms_supervisor::{
    config::load_config_dir,
    scope::{build_lead_scope, install_lead_scope},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ms-scope-install-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[ignore = "red: T038 — lead scope generation and authoritative persistence are not implemented"]
fn lead_scope_is_ceiling_intersected_and_persisted_before_session_start() {
    let home = temp_dir();
    let config_dir = home.join("config");
    let session_dir = home.join("sessions/session-1");
    let work_dir = home.join("work");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(session_dir.join("control")).unwrap();
    fs::create_dir_all(session_dir.join("out")).unwrap();
    fs::create_dir_all(&work_dir).unwrap();
    fs::write(
        config_dir.join("microswarm.toml"),
        format!(
            r#"
[runtime]
socket = "unix:///run/user/1000/podman/podman.sock"
image = "microswarm-agent:dev"
[lead.scope]
mounts = [{{ host = "{}", path = "/work", mode = "rw" }}]
credentials = ["allowed", "blocked"]
tools = [{{ server = "microswarm-tools", tools = ["fs.read", "fs.write"], policy = "ask" }}]
delegation = {{ enabled = true, max_members = 4 }}
"#,
            work_dir.display()
        ),
    )
    .unwrap();
    fs::write(config_dir.join("policy.toml"), "").unwrap();
    fs::write(config_dir.join("tools.toml"), "").unwrap();
    fs::write(
        config_dir.join("ceiling.toml"),
        r#"
paths = []
credentials = ["blocked"]
tools = ["microswarm-tools/fs.write"]
"#,
    )
    .unwrap();

    let config = load_config_dir(&config_dir).unwrap();
    let scope = build_lead_scope(&config, "01J00000000000000000000000").unwrap();
    assert_eq!(scope.derived_from, DerivedFrom::Config);
    assert_eq!(scope.credentials, ["allowed"]);
    assert!(scope.tools[0].tools.contains("fs.read"));
    assert!(!scope.tools[0].tools.contains("fs.write"));
    assert_eq!(scope.hash, scope.canonical_hash());

    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    let record = SessionRecord {
        id: scope.session_id.clone(),
        root_id: scope.session_id.clone(),
        parent_id: None,
        role: "lead".into(),
        kind: "lead".into(),
        channel: Some("cli".into()),
        channel_thread: Some("thread-1".into()),
        status: "requested".into(),
        scope_hash: None,
        scope: None,
        created_at: "2026-10-19T00:00:00Z".into(),
        last_active_at: "2026-10-19T00:00:00Z".into(),
    };
    install_lead_scope(&mut store, record, &session_dir, &scope).unwrap();
    let file_scope: ms_proto::Scope =
        serde_json::from_slice(&fs::read(session_dir.join("control/scope.json")).unwrap()).unwrap();
    assert_eq!(file_scope.hash, scope.hash);
    let (stored_hash, stored_json): (String, String) = store
        .connection()
        .query_row(
            "SELECT scope_hash, scope_json FROM sessions WHERE id=?1",
            [&scope.session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored_hash, scope.hash);
    assert_eq!(
        serde_json::from_str::<ms_proto::Scope>(&stored_json).unwrap(),
        scope
    );
    drop(store);
    fs::remove_dir_all(home).unwrap();
}
