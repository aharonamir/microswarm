#[path = "harness.rs"]
mod harness;

use std::{thread, time::{Duration, Instant}};

#[test]
#[ignore = "red: T078 — restart does not expire and reissue approval"]
fn restart_reissues_pending_approval_and_rejects_old_code() {
    let Some(mut harness) = harness::IntegrationHarness::start_approval_test_or_skip() else { return };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("run approval tool").unwrap();
    let old_prompt = chat.wait_for("/approve ", Duration::from_secs(10)).unwrap();
    let old_code = old_prompt.split("/approve ").nth(1).unwrap().split_whitespace().next().unwrap().to_owned();
    let store = ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let old_id: String = store.connection().query_row("SELECT id FROM approvals WHERE code=?1", [&old_code], |row| row.get(0)).unwrap();
    harness.stop_supervisor_for_test();
    harness.restart_supervisor().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    let new_code: String = loop {
        let state = store.connection().query_row("SELECT status,reissued_as FROM approvals WHERE id=?1", [&old_id], |row| Ok((row.get::<_, String>(0)?,row.get::<_, Option<String>>(1)?))).unwrap();
        if state.0 == "expired" {
            let new_id = state.1.expect("expired approval must link replacement");
            break store.connection().query_row("SELECT code FROM approvals WHERE id=?1 AND status='pending'", [&new_id], |row| row.get(0)).unwrap();
        }
        assert!(Instant::now() < until, "approval was not reissued");
        thread::sleep(Duration::from_millis(25));
    };
    assert_ne!(old_code, new_code);
    let old = harness.chat(None, &[&format!("/approve {old_code}")]).unwrap();
    assert!(!old.status.success(), "expired code must be rejected");
    let events: i64 = store.connection().query_row("SELECT COUNT(*) FROM supervisor_events WHERE type IN ('approval.expired','approval.reissued')", [], |row| row.get(0)).unwrap();
    assert_eq!(events, 2);
    chat.close_input();
}
