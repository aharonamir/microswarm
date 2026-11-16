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
    let task_ids = member
        .inbound()
        .prepare("SELECT id FROM inbound WHERE kind='task' ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        task_ids.len(),
        2,
        "spawn task and follow-up task are distinct"
    );
    let reply_task_ids = member
        .outbound()
        .prepare("SELECT in_reply_to FROM outbound WHERE kind='reply' ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get::<_, Option<i64>>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(reply_task_ids, vec![Some(task_ids[0]), Some(task_ids[1])]);
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
    let initial_reply: String = lead
        .inbound()
        .query_row(
            "SELECT json_extract(body,'$.initial_reply') FROM inbound WHERE kind='delegation_result' AND json_extract(body,'$.initial_reply') IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let routed_reply: String = lead
        .inbound()
        .query_row(
            "SELECT json_extract(body,'$.text') FROM inbound WHERE kind='member_reply'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(routed_reply, "role instructions reached model");
    assert_eq!(initial_reply, "role instructions reached model");
    assert_eq!(
        member_reply, 1,
        "initial reply is in delegation_result, not replayed as member_reply"
    );
    assert!(support::has_supervisor_event(&harness, "delegation.reply"));
}
