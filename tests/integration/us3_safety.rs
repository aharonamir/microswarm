#[path = "us3_support.rs"]
mod support;

use ms_proto::ExitReason;
use ms_supervisor::sessions::classify_exit;
use std::{process::Command, time::Duration};

#[test]
fn non_oom_signal_fails_member_safely_and_notifies_its_lead() {
    assert_eq!(classify_exit(77, false), ExitReason::Safety);
    assert_eq!(classify_exit(42, false), ExitReason::Safety);
    assert_eq!(classify_exit(143, false), ExitReason::Safety);
    assert_eq!(classify_exit(137, false), ExitReason::Safety);
    assert_eq!(classify_exit(137, true), ExitReason::Transient);

    let Some(harness) = support::start_with_skill() else {
        return;
    };
    harness.chat(None, &["spawn researcher"]).unwrap();
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let member: (String, String) = store
        .connection()
        .query_row(
            "SELECT id,parent_id FROM sessions WHERE kind='member' ORDER BY created_at LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let container = harness.containers_for(&member.0).unwrap().remove(0);
    let output = Command::new(support::runtime_name(&harness))
        .args(["kill", "--signal", "SIGTERM", &container])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !harness.container_running(&container).unwrap(),
        "SIGTERM did not stop member container"
    );
    let failure = support::wait_for_inbound(
        &harness.home,
        &member.1,
        "member_failed",
        Duration::from_secs(8),
    );
    assert_eq!(failure["member_id"], member.0);
    assert_eq!(failure["reason"], "safety");
    support::wait_for_status(&harness.home, &member.0, "failed", Duration::from_secs(3));
    assert_eq!(support::session_state(&harness.home, &member.0).1, 0);
    let lead_session =
        ms_store::open_session_dir(harness.home.join("sessions").join(&member.1)).unwrap();
    let notices: i64 = lead_session
        .inbound()
        .query_row(
            "SELECT COUNT(*) FROM inbound WHERE kind='member_failed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(notices, 1);
}

#[test]
fn supervisor_reconciles_an_exit_that_occurred_while_disconnected() {
    let Some(mut harness) = support::start() else {
        return;
    };
    harness.chat(None, &["hello"]).unwrap();
    let lead = support::lead_id(&harness.home);
    let container = harness.containers_for(&lead).unwrap().remove(0);
    let runtime = support::runtime_name(&harness);
    harness.stop_supervisor_for_test();
    let output = Command::new(runtime)
        .args(["kill", "--signal", "SIGTERM", &container])
        .output()
        .unwrap();
    assert!(output.status.success());
    harness.restart_supervisor().unwrap();
    support::wait_for_status(&harness.home, &lead, "failed", Duration::from_secs(5));
}

#[test]
fn supervisor_restarts_a_missing_container_transiently_after_restart() {
    let Some(mut harness) = support::start() else {
        return;
    };
    harness.chat(None, &["hello"]).unwrap();
    let lead = support::lead_id(&harness.home);
    let container = harness.containers_for(&lead).unwrap().remove(0);
    let runtime = support::runtime_name(&harness);
    harness.stop_supervisor_for_test();
    let output = Command::new(runtime)
        .args(["rm", "-f", &container])
        .output()
        .unwrap();
    assert!(output.status.success());
    harness.restart_supervisor().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let (status, restarts, _) = support::session_state(&harness.home, &lead);
        if restarts > 0 {
            assert_ne!(status, "failed");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "missing container was not restarted transiently"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
