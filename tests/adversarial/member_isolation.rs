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
    harness.chat(None, &["spawn researcher twice"]).unwrap();
    let members = support::members(&harness);
    assert_eq!(members.len(), 2);
    let lead_id = members[0].2.as_deref().unwrap();
    let runtime = match harness.runtime {
        support::harness::Runtime::Docker => "docker",
        support::harness::Runtime::Podman => "podman",
    };
    let session_ids = std::iter::once(lead_id.to_owned())
        .chain(members.iter().map(|member| member.0.clone()))
        .collect::<Vec<_>>();
    for session_id in session_ids {
        let containers = harness.containers_for(&session_id).unwrap();
        assert_eq!(containers.len(), 1);
        let output = std::process::Command::new(runtime)
            .args([
                "inspect",
                "--format",
                "{{json .NetworkSettings.Networks}}",
                &containers[0],
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let attached: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("runtime network attachment JSON");
        let names = attached
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let expected = format!("ms-net-{session_id}");
        assert_eq!(names, [expected.as_str()]);
    }
    let mut member_networks = Vec::new();
    for member in &members {
        let scope: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                harness
                    .home
                    .join("sessions")
                    .join(&member.0)
                    .join("control/scope.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let credentials = scope["credentials"].as_array().unwrap();
        assert!(credentials.iter().any(|name| name == "fixture-model-key"));
        assert!(credentials.iter().all(|name| name != "lead-only-secret"));
        let network = format!("ms-net-{}", member.0);
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
        assert_eq!(
            details.trim(),
            "true 1",
            "member network is not isolated: {details}"
        );
        member_networks.push(network);
    }
    assert_ne!(member_networks[0], member_networks[1]);

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
