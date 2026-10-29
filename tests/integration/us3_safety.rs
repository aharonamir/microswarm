#[path = "us3_support.rs"]
mod support;

use ms_proto::ExitReason;
use ms_supervisor::sessions::classify_exit;
use std::{process::Command, time::Duration};

#[test]
#[ignore = "red: T068 — safety failures and non-OOM signals must never restart"]
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
}
