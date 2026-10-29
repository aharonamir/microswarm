#[path = "../integration/harness.rs"]
mod harness;

use harness::IntegrationHarness;
use std::process::Command;

#[test]
fn agent_has_no_secrets_and_only_proxy_egress() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let chat = harness
        .chat(None, &["verify container isolation"])
        .expect("send test message");
    assert!(
        chat.status.success(),
        "{}",
        String::from_utf8_lossy(&chat.stderr)
    );
    let sessions = harness.session_dirs().expect("list session directories");
    let session_id = sessions[0].file_name().unwrap().to_string_lossy();
    let containers = harness
        .containers_for(&session_id)
        .expect("list session containers");
    assert_eq!(containers.len(), 1);

    let (runtime, inspect_args) = match harness.runtime {
        harness::Runtime::Docker => (
            "docker",
            vec!["inspect", "--format", "{{json .Config.Env}}"],
        ),
        harness::Runtime::Podman => (
            "podman",
            vec!["inspect", "--format", "{{json .Config.Env}}"],
        ),
    };
    let env = Command::new(runtime)
        .args(inspect_args)
        .arg(&containers[0])
        .output()
        .expect("inspect agent environment");
    assert!(env.status.success());
    let env = String::from_utf8(env.stdout)
        .expect("container env is UTF-8")
        .to_ascii_lowercase();
    for forbidden in ["api_key=", "token=", "secret=", "password="] {
        assert!(
            !env.contains(forbidden),
            "container env contains {forbidden}"
        );
    }

    // The probe lives in ms-tools so it can inspect /proc/1/environ and test
    // direct networking without adding shell utilities to the scratch image.
    let probe = Command::new(runtime)
        .args([
            "exec",
            &containers[0],
            "/microswarm-tools",
            "--security-probe",
        ])
        .output()
        .expect("run the in-container security probe");
    assert!(
        probe.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let result = String::from_utf8(probe.stdout).expect("security probe returns UTF-8 JSON");
    for invariant in [
        "no_secret_environment",
        "no_secret_proc_environment",
        "no_secret_files",
        "direct_egress_blocked",
        "proxy_requires_scoped_credential",
    ] {
        assert!(
            result.contains(&format!("\"{invariant}\":true")),
            "security invariant failed: {invariant}: {result}"
        );
    }
}
