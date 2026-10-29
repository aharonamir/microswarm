#[path = "us2_support.rs"]
mod support;

#[test]
#[ignore = "red: T046 — allow policy must spawn an isolated member session"]
fn allow_role_spawns_a_second_container_and_returns_member_id() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher for auth review"])
        .unwrap();
    assert!(output.status.success());
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].1, "researcher");
    assert!(members[0].2.is_some());
    assert_eq!(harness.containers_for(&members[0].0).unwrap().len(), 1);
}
