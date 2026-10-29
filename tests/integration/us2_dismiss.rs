#[path = "us2_support.rs"]
mod support;

#[test]
fn dismiss_waits_for_reply_then_keeps_closed_member_record() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher, send task, dismiss member"])
        .unwrap();
    assert!(output.status.success());
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let containers = harness.containers_for(&members[0].0).unwrap();
    assert_eq!(containers.len(), 1);
    assert!(!harness.container_running(&containers[0]).unwrap());
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let status: String = store
        .connection()
        .query_row(
            "SELECT status FROM sessions WHERE id=?1",
            [&members[0].0],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "closed");
    assert!(harness.home.join("sessions").join(&members[0].0).is_dir());
    assert!(support::has_supervisor_event(
        &harness,
        "delegation.dismissed"
    ));
}
