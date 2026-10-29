#[path = "../integration/harness.rs"]
mod harness;

use harness::{IntegrationHarness, Runtime};
use std::fs;
use std::process::Command;

#[test]
fn agent_container_cannot_reach_host_control_or_another_session() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };

    let other_session = harness.home.join("sessions/other-session");
    fs::create_dir_all(&other_session).expect("create another session directory");
    let chat = harness
        .chat(None, &["start an agent for the mount security test"])
        .expect("run chat");
    assert!(
        chat.status.success(),
        "{}",
        String::from_utf8_lossy(&chat.stderr)
    );

    let sessions = harness.session_dirs().expect("list session directories");
    let session_dir = sessions
        .iter()
        .find(|path| path != &&other_session)
        .expect("chat should create a session directory");
    let session_id = session_dir
        .file_name()
        .expect("session directory has a name")
        .to_string_lossy();
    let containers = harness
        .containers_for(&session_id)
        .expect("list session containers");
    assert_eq!(containers.len(), 1, "expected one session container");

    let (program, socket_paths) = match harness.runtime {
        Runtime::Docker => (
            "docker",
            vec![
                "/var/run/docker.sock".to_owned(),
                std::env::var("DOCKER_HOST").unwrap_or_default(),
            ],
        ),
        Runtime::Podman => (
            "podman",
            vec![
                "/run/podman/podman.sock".to_owned(),
                "/run/user/1000/podman/podman.sock".to_owned(),
                std::env::var("CONTAINER_HOST").unwrap_or_default(),
            ],
        ),
    };
    let output = Command::new(program)
        .args([
            "inspect",
            "--format",
            "{{range .Mounts}}{{.Destination}}|{{.Source}}|{{.RW}}{{\"\\n\"}}{{end}}",
            &containers[0],
        ])
        .output()
        .expect("inspect the agent container");
    assert!(
        output.status.success(),
        "{program} inspect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mounts = String::from_utf8(output.stdout).expect("inspect output is UTF-8");
    let control_mount = mounts
        .lines()
        .find(|line| line.starts_with("/session/control|"))
        .expect("control directory must be mounted into the agent");
    assert_eq!(
        control_mount.split('|').nth(2),
        Some("false"),
        "control mount must be read-only"
    );

    // Exercise the filesystem boundary from inside the container. A shell.run
    // command and the fs.write MCP tool both resolve to an ordinary OS write;
    // neither may alter the supervisor-owned control files or SQLite sidecars.
    for path in [
        "/session/control/inbound.db",
        "/session/control/scope.json",
        "/session/control/inbound.db-wal",
        "/session/control/inbound.db-shm",
    ] {
        let command = format!("printf forged > {path}");
        let result = Command::new(program)
            .args(["exec", &containers[0], "sh", "-c", &command])
            .output()
            .expect("run a write attempt inside the agent container");
        assert!(
            !result.status.success(),
            "container write to {path} must fail at the OS level: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let forbidden_sources = [
        std::env::var("HOME").unwrap_or_default() + "/.ssh",
        harness.home.join("supervisor.db").display().to_string(),
        other_session.display().to_string(),
    ]
    .into_iter()
    .chain(socket_paths)
    .filter(|path| !path.is_empty());
    for forbidden in forbidden_sources {
        assert!(
            !mounts.lines().any(|line| line.contains(&forbidden)),
            "host-only path {forbidden} must not be mounted into the agent: {mounts}"
        );
    }
}
