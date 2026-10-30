#[path = "us3_support.rs"]
mod support;

use std::{process::Command, thread, time::{Duration, Instant}};

#[test]
#[ignore = "red: T080 — boot treats missing managed container as safety exit"]
fn restart_treats_missing_active_container_as_transient_crash() {
    let Some(mut harness) = support::start() else { return };
    harness.chat(None, &["hello"]).unwrap();
    let lead = support::lead_id(&harness.home);
    let container = harness.containers_for(&lead).unwrap().remove(0);
    harness.stop_supervisor_for_test();
    let removed = Command::new(support::runtime_name(&harness)).args(["rm", "-f", &container]).output().unwrap();
    assert!(removed.status.success());
    harness.restart_supervisor().unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let (status, restarts, _) = support::session_state(&harness.home, &lead);
        if restarts > 0 { assert_ne!(status, "failed", "missing container should use transient policy"); break }
        assert!(Instant::now() < until, "missing container was not processed as transient");
        thread::sleep(Duration::from_millis(25));
    }
}
