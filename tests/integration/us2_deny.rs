#[path = "us2_support.rs"]
mod support;

#[test]
#[ignore = "red: T048 — denied role must emit delegation.denied and never spawn"]
fn deny_role_returns_denial_without_creating_a_member() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness.chat(None, &["spawn forbidden role"]).unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("denied"));
    assert!(support::members(&harness).is_empty());
}
