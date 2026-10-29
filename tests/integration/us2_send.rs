#[path = "us2_support.rs"]
mod support;

#[test]
fn lead_sends_task_and_receives_member_reply() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher and send task"])
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("fixture reply"));
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let member =
        ms_store::open_session_dir(harness.home.join("sessions").join(&members[0].0)).unwrap();
    let inbound_kind: String = member
        .inbound()
        .query_row("SELECT kind FROM inbound WHERE kind='task'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(inbound_kind, "task");
    let lead = ms_store::open_session_dir(
        harness
            .home
            .join("sessions")
            .join(members[0].2.as_ref().unwrap()),
    )
    .unwrap();
    let member_reply: i64 = lead
        .inbound()
        .query_row(
            "SELECT COUNT(*) FROM inbound WHERE kind='member_reply'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(member_reply, 1);
    assert!(support::has_supervisor_event(&harness, "delegation.reply"));
}
