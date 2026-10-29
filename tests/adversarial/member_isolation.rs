#[path = "../integration/us2_support.rs"]
mod support;

#[test]
fn member_network_is_internal_and_credentials_are_limited_to_its_scope() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        text.replace(
            "credentials = [\"fixture-model-key\"]",
            "credentials = [\"fixture-model-key\", \"lead-only-secret\"]",
        ),
    )
    .unwrap();
    harness.chat(None, &["spawn researcher"]).unwrap();
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let lead_id = members[0].2.as_deref().unwrap();
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
    let credentials = scope["credentials"].as_array().unwrap();
    assert!(credentials.iter().any(|name| name == "fixture-model-key"));
    assert!(credentials.iter().all(|name| name != "lead-only-secret"));
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

    let lead_network = format!("ms-net-{lead_id}");
    let lead_output = std::process::Command::new(runtime)
        .args([
            "network",
            "inspect",
            &lead_network,
            "--format",
            "{{.Internal}} {{len .Containers}}",
        ])
        .output()
        .unwrap();
    assert!(lead_output.status.success());
    let lead_details = String::from_utf8_lossy(&lead_output.stdout);
    assert!(
        lead_details.starts_with("true") && lead_details.contains("1"),
        "lead must be alone on its separate internal network: {lead_details}"
    );
}
