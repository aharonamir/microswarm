use ms_proto::{DerivedFrom, Limits, Mount, MountMode, Scope};
use ms_supervisor::runtime::ContainerProfile;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn scope() -> Scope {
    Scope {
        version: 1,
        session_id: "01J00000000000000000000000".into(),
        derived_from: DerivedFrom::Config,
        source_ref: "test".into(),
        mounts: vec![Mount {
            host_path: PathBuf::from("/tmp/project"),
            container_path: PathBuf::from("/work"),
            mode: MountMode::ReadWrite,
        }],
        credentials: vec![],
        tools: vec![],
        delegation: None,
        limits: Limits {
            memory_mb: 384,
            pids: 64,
            tmp_mb: 96,
            max_turns: 8,
            max_tokens: 4096,
        },
        narrowed: vec![],
        hash: "".into(),
    }
}

#[test]
#[ignore = "red: T036 — secure container runtime profile is not implemented"]
fn runtime_profile_enforces_mount_and_process_boundaries() {
    let session_dir = std::env::temp_dir().join(format!(
        "ms-runtime-profile-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&session_dir).unwrap();
    let profile = ContainerProfile::for_session(
        &scope(),
        "microswarm-agent:dev",
        &session_dir,
        "ms-net-session",
    )
    .unwrap();
    assert!(profile.read_only_rootfs);
    assert!(profile.no_new_privileges);
    assert_eq!(profile.cap_drop, ["ALL"]);
    assert_eq!(profile.memory_bytes, 384 * 1024 * 1024);
    assert_eq!(profile.pids_limit, 64);
    assert_eq!(profile.network, "ms-net-session");
    assert!(profile.tmpfs.contains_key("/tmp"));
    assert!(
        profile
            .mounts
            .iter()
            .any(|mount| { mount.target == "/session/control" && mount.read_only })
    );
    assert!(
        profile
            .mounts
            .iter()
            .any(|mount| { mount.target == "/session/out" && !mount.read_only })
    );
    assert!(
        profile
            .mounts
            .iter()
            .any(|mount| { mount.target == "/work" && !mount.read_only })
    );
    fs::remove_dir_all(session_dir).unwrap();
}
