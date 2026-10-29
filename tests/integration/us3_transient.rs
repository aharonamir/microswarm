#[path = "us3_support.rs"]
mod support;

use std::time::{Duration, Instant};

#[test]
#[ignore = "red: T067 — transient failures must restart and replay unacknowledged work"]
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
#[ignore = "red: T067 — transient restart attempts must stop after three retries"]
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
