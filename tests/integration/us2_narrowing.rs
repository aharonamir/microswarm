#[path = "us2_support.rs"]
mod support;

#[test]
fn request_outside_lead_scope_is_narrowed_and_not_mounted() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher and request /outside"])
        .unwrap();
    assert!(output.status.success());
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let scope = std::fs::read_to_string(
        harness
            .home
            .join("sessions")
            .join(&members[0].0)
            .join("control/scope.json"),
    )
    .unwrap();
    let scope: serde_json::Value = serde_json::from_str(&scope).unwrap();
    let mounts = scope["mounts"].as_array().unwrap();
    assert!(mounts.iter().all(|mount| {
        mount["container_path"] != "/outside"
            && !mount["host_path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("/outside")
    }));
    assert!(
        scope["narrowed"].to_string().contains("not_in_lead_scope")
            || scope["narrowed"].to_string().contains("ceiling")
    );
    assert!(support::has_supervisor_event(&harness, "scope.narrowed"));
}
