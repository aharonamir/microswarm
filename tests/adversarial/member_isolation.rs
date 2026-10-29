#[path = "../integration/us2_support.rs"]
mod support;

#[test]
#[ignore = "red: T053 — members must have isolated networks and scoped credentials"]
fn member_network_is_internal_and_credentials_are_limited_to_its_scope() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    harness.chat(None, &["spawn researcher"]).unwrap();
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let scope: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            harness
                .home
                .join("sessions")
                .join(&members[0].0)
                .join("control/scope.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(scope["credentials"].as_array().unwrap().is_empty());
    let network = format!("ms-net-{}", members[0].0);
    let runtime = match harness.runtime {
        support::harness::Runtime::Docker => "docker",
        support::harness::Runtime::Podman => "podman",
    };
    let output = std::process::Command::new(runtime)
        .args([
            "network",
            "inspect",
            &network,
            "--format",
            "{{.Internal}} {{len .Containers}}",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let details = String::from_utf8_lossy(&output.stdout);
    assert!(
        details.starts_with("true"),
        "member network must block direct egress: {details}"
    );
    assert!(
        details.contains("1"),
        "member must be alone on its network: {details}"
    );
}
