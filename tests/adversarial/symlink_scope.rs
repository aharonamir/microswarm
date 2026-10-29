#[path = "../integration/us2_support.rs"]
mod support;

#[test]
fn symlink_in_lead_mount_does_not_widen_member_scope() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let outside = harness.home.join("outside-secret.txt");
    std::fs::write(&outside, "not mounted").unwrap();
    std::os::unix::fs::symlink(&outside, harness.home.join("work/escape-link")).unwrap();
    harness
        .chat(None, &["spawn researcher with /work/escape-link scope"])
        .unwrap();
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
    let mounts = scope["mounts"].as_array().unwrap();
    assert!(
        mounts
            .iter()
            .all(|mount| mount["host_path"] != outside.to_string_lossy().as_ref())
    );
}
