#[path = "us2_support.rs"]
mod support;

#[test]
#[ignore = "red: T049 — member request must be intersected with lead scope"]
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
    assert!(!scope.contains("/outside"));
    assert!(scope.contains("not_in_lead_scope") || scope.contains("ceiling"));
}
