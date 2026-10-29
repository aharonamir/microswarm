#[path = "us3_support.rs"]
mod support;

use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn transient_model_failure_restarts_and_reprocesses_one_unacknowledged_message() {
    let Some(harness) = support::start() else {
        return;
    };
    let mut chat = harness.start_chat(None).unwrap();
    let started = Instant::now();
    chat.send_line("transient once").unwrap();
    let lead = support::wait_for_lead_id(&harness.home, Duration::from_secs(5));
    chat.wait_for("fixture reply", Duration::from_secs(20))
        .unwrap();

    let (status, restart_count, _) = support::session_state(&harness.home, &lead);
    assert_eq!(status, "ready");
    assert_eq!(restart_count, 1);
    assert_eq!(harness.model.request_count(), 2);
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "restart backoff was skipped"
    );

    let session = ms_store::open_session_dir(harness.home.join("sessions").join(&lead)).unwrap();
    let (inbound, replies): (i64, i64) = session.outbound().query_row(
        "SELECT (SELECT last_inbound_id FROM cursor WHERE singleton=1),COUNT(*) FROM outbound WHERE kind='reply'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(
        inbound, 1,
        "the message must be acknowledged once after replay"
    );
    assert_eq!(replies, 1, "replay must not duplicate the reply");
}

#[test]
fn transient_restart_limit_is_three() {
    let Some(harness) = support::start() else {
        return;
    };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("transient always").unwrap();
    let lead = support::wait_for_lead_id(&harness.home, Duration::from_secs(5));
    support::wait_for_status(&harness.home, &lead, "failed", Duration::from_secs(25));
    assert_eq!(support::session_state(&harness.home, &lead).1, 3);
    assert_eq!(harness.model.request_count(), 4);
}

#[test]
fn member_transient_restart_does_not_report_failure_to_lead() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher and send retry member"])
        .unwrap();
    assert!(output.status.success());
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let (member, lead, restarts): (String, String, u8) = store
        .connection()
        .query_row(
            "SELECT id,parent_id,restart_count FROM sessions WHERE kind='member' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(restarts, 1);
    assert_eq!(support::session_state(&harness.home, &member).0, "ready");
    let lead_store = ms_store::open_session_dir(harness.home.join("sessions").join(lead)).unwrap();
    let failures: i64 = lead_store
        .inbound()
        .query_row(
            "SELECT COUNT(*) FROM inbound WHERE kind='member_failed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(failures, 0);
}

#[test]
fn failed_container_restart_resolves_the_session() {
    let Some(harness) = support::start() else {
        return;
    };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("transient always").unwrap();
    let lead = support::wait_for_lead_id(&harness.home, Duration::from_secs(5));
    support::wait_for_status(&harness.home, &lead, "crashed", Duration::from_secs(5));
    let container = harness.containers_for(&lead).unwrap().remove(0);
    let output = Command::new(support::runtime_name(&harness))
        .args(["rm", "-f", &container])
        .output()
        .unwrap();
    assert!(output.status.success());
    support::wait_for_status(&harness.home, &lead, "failed", Duration::from_secs(5));
    assert_eq!(support::session_state(&harness.home, &lead).1, 1);
}
