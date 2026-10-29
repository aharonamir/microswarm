use ms_proto::DerivedFrom;
use ms_proto::{Limits, Mount, MountMode, Permission, Scope, ScopeCeiling, ToolGrant, ToolNames};
use ms_store::{SessionRecord, SupervisorStore};
use ms_supervisor::{
    config::load_config_dir,
    scope::{build_lead_scope, build_member_scope, install_lead_scope, install_member_scope},
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
fn member_scope_translates_paths_and_applies_stricter_mount_and_tool_grants() {
    let home = temp_dir();
    let work = home.join("work");
    fs::create_dir_all(work.join("src")).unwrap();
    let lead = lead_scope(&work, MountMode::ReadOnly);
    let template = serde_json::json!({
        "mounts":[{"path":"/work","mode":"rw"}],
        "credentials":["web-key"],
        "tools":[{"server":"microswarm-tools","tools":["fs.read","fs.list"]}],
        "limits":{"max_turns":20,"memory_mb":128}
    });
    let request = serde_json::json!({
        "mounts":[{"path":"/work/src","mode":"rw"}],
        "credentials":["web-key"],
        "tools":[{"server":"microswarm-tools","tools":["fs.read","fs.write"]}]
    });
    let result = build_member_scope(
        &lead,
        &template,
        &request,
        "01J00000000000000000000001",
        "01J00000000000000000000002",
        &ScopeCeiling::default(),
    )
    .unwrap();
    assert_eq!(result.derived_from, DerivedFrom::Delegation);
    assert!(result.delegation.is_none());
    assert_eq!(result.mounts.len(), 1);
    assert_eq!(result.mounts[0].host_path, work.join("src"));
    assert_eq!(result.mounts[0].container_path, PathBuf::from("/work/src"));
    assert_eq!(result.mounts[0].mode, MountMode::ReadOnly);
    assert_eq!(result.credentials, ["web-key"]);
    assert!(result.tools[0].tools.contains("fs.read"));
    assert!(!result.tools[0].tools.contains("fs.write"));
    assert_eq!(result.limits.max_turns, 20);
    let lead_control = home.join("sessions/lead/control");
    let member_dir = home.join("sessions/member");
    fs::create_dir_all(&lead_control).unwrap();
    fs::create_dir_all(member_dir.join("control")).unwrap();
    fs::create_dir_all(member_dir.join("out")).unwrap();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    let mut stored_lead = lead_scope(&work, MountMode::ReadOnly);
    install_lead_scope(
        &mut store,
        SessionRecord {
            id: stored_lead.session_id.clone(),
            root_id: stored_lead.session_id.clone(),
            parent_id: None,
            role: "lead".into(),
            kind: "lead".into(),
            channel: Some("cli".into()),
            channel_thread: Some("scope-test".into()),
            status: "ready".into(),
            scope_hash: None,
            scope: None,
            created_at: "2026-10-29T00:00:00Z".into(),
            last_active_at: "2026-10-29T00:00:00Z".into(),
        },
        &home.join("sessions/lead"),
        &stored_lead,
    )
    .unwrap();
    stored_lead.update_hash();
    install_member_scope(
        &mut store,
        SessionRecord {
            id: result.session_id.clone(),
            root_id: stored_lead.session_id.clone(),
            parent_id: Some(stored_lead.session_id.clone()),
            role: "researcher".into(),
            kind: "member".into(),
            channel: None,
            channel_thread: None,
            status: "pending".into(),
            scope_hash: None,
            scope: None,
            created_at: "2026-10-29T00:00:01Z".into(),
            last_active_at: "2026-10-29T00:00:01Z".into(),
        },
        &member_dir,
        &result,
    )
    .unwrap();
    let stored: (String, String) = store
        .connection()
        .query_row(
            "SELECT scope_hash, scope_json FROM sessions WHERE id=?1",
            [&result.session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored.0, result.hash);
    assert_eq!(
        serde_json::from_slice::<Scope>(&fs::read(member_dir.join("control/scope.json")).unwrap())
            .unwrap(),
        result
    );
    assert_eq!(
        serde_json::from_str::<Scope>(&stored.1).unwrap(),
        result,
        "authoritative scope must be in supervisor.db before container creation"
    );
    drop(store);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn member_scope_rejects_nonempty_requests_with_no_surviving_grants() {
    let home = temp_dir();
    let work = home.join("work");
    fs::create_dir_all(&work).unwrap();
    let lead = lead_scope(&work, MountMode::ReadOnly);
    let template = serde_json::json!({
        "mounts":[{"path":"/work","mode":"ro"}],
        "credentials":[],
        "tools":[{"server":"microswarm-tools","tools":["fs.read"]}]
    });
    let request = serde_json::json!({
        "mounts":[{"path":"/outside","mode":"ro"}],
        "credentials":["not-granted"],
        "tools":[{"server":"unknown","tools":["run"]}]
    });
    let result = build_member_scope(
        &lead,
        &template,
        &request,
        "01J00000000000000000000001",
        "01J00000000000000000000002",
        &ScopeCeiling::default(),
    );
    assert!(result.unwrap_err().contains("scope_empty"));
    fs::remove_dir_all(home).unwrap();
}

fn lead_scope(work: &std::path::Path, mode: MountMode) -> Scope {
    let mut scope = Scope {
        version: 1,
        session_id: "01J00000000000000000000000".into(),
        derived_from: DerivedFrom::Config,
        source_ref: "config".into(),
        mounts: vec![Mount {
            host_path: work.to_path_buf(),
            container_path: PathBuf::from("/work"),
            mode,
        }],
        credentials: vec!["web-key".into()],
        tools: vec![ToolGrant {
            server: "microswarm-tools".into(),
            tools: ToolNames::Selected(["fs.read".into(), "fs.list".into()].into()),
            policy: Permission::Allow,
            overrides: Default::default(),
        }],
        delegation: Some(ms_proto::DelegationScope {
            enabled: true,
            max_members: 4,
        }),
        limits: Limits {
            memory_mb: 256,
            pids: 64,
            tmp_mb: 128,
            max_turns: 30,
            max_tokens: 16_384,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    scope.update_hash();
    scope
}

#[test]
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
        status: "pending".into(),
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
