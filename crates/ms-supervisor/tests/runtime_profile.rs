use ms_proto::{DerivedFrom, Limits, Mount, MountMode, Scope};
use ms_supervisor::runtime::ContainerProfile;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn scope(work_path: &std::path::Path) -> Scope {
    Scope {
        version: 1,
        session_id: "01J00000000000000000000000".into(),
        derived_from: DerivedFrom::Config,
        source_ref: "test".into(),
        mounts: vec![Mount {
            host_path: work_path.to_path_buf(),
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
fn runtime_profile_enforces_mount_and_process_boundaries() {
    let session_dir = std::env::temp_dir().join(format!(
        "ms-runtime-profile-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&session_dir).unwrap();
    fs::create_dir_all(session_dir.join("control")).unwrap();
    fs::create_dir_all(session_dir.join("out")).unwrap();
    let work_path = session_dir.join("work");
    fs::create_dir_all(&work_path).unwrap();
    let profile = ContainerProfile::for_session(
        &scope(&work_path),
        "microswarm-agent:dev",
        &session_dir,
        "ms-net-session",
        "192.168.112.1",
        "1000:1000",
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
    let config = profile.container_config(&["MS_KIND=lead".into()]);
    let host = config.host_config.unwrap();
    assert_eq!(
        config.labels.unwrap()["ms.session"],
        scope(&work_path).session_id
    );
    assert_eq!(config.user.as_deref(), Some("1000:1000"));
    assert_eq!(host.readonly_rootfs, Some(true));
    assert_eq!(host.security_opt.unwrap(), ["no-new-privileges:true"]);
    assert_eq!(host.cap_drop.unwrap(), ["ALL"]);
    assert_eq!(host.memory, Some(384 * 1024 * 1024));
    assert_eq!(host.pids_limit, Some(64));
    assert_eq!(host.network_mode.as_deref(), Some("ms-net-session"));
    let mounts = host.mounts.unwrap();
    assert!(mounts.iter().any(|mount| {
        mount.target.as_deref() == Some("/session/control") && mount.read_only == Some(true)
    }));
    assert!(mounts.iter().any(|mount| {
        mount.target.as_deref() == Some("/session/out") && mount.read_only == Some(false)
    }));
    fs::remove_dir_all(session_dir).unwrap();
}

#[test]
fn runtime_profile_maps_container_host_names_to_the_host_gateway() {
    let session_dir = std::env::temp_dir().join(format!(
        "ms-runtime-host-alias-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(session_dir.join("control")).unwrap();
    fs::create_dir_all(session_dir.join("out")).unwrap();
    let work_path = session_dir.join("work");
    fs::create_dir_all(&work_path).unwrap();
    let profile = ContainerProfile::for_session(
        &scope(&work_path),
        "microswarm-agent:dev",
        &session_dir,
        "ms-net-session",
        "192.168.112.1",
        "1000:1000",
    )
    .unwrap();
    let config = profile.container_config(&[]);
    let host = config.host_config.unwrap();
    let extra_hosts = host.extra_hosts.unwrap();
    assert!(extra_hosts.contains(&"host.containers.internal:192.168.112.1".to_owned()));
    assert!(extra_hosts.contains(&"host.docker.internal:192.168.112.1".to_owned()));
    fs::remove_dir_all(session_dir).unwrap();
}
