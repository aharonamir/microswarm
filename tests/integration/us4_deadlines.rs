#[path = "us3_support.rs"]
mod support;

use std::{thread, time::{Duration, Instant}};

#[test]
#[ignore = "red: T079 — boot does not explicitly rearm persisted deadlines"]
fn restart_enforces_original_delegation_deadline() {
    let Some(mut harness) = support::start_with_skill() else { return };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn researcher and send hang").unwrap();
    let lead = support::wait_for_lead_id(&harness.home, Duration::from_secs(5));
    let until = Instant::now() + Duration::from_secs(6);
    let store = ms_store::SupervisorStore::open(harness.home.join("supervisor.db")).unwrap();
    let (member, original): (String, String) = loop {
        if let Ok(found) = store.connection().query_row("SELECT member_id,deadline_at FROM delegations WHERE lead_id=?1 AND status='waiting_reply'", [&lead], |row| Ok((row.get(0)?,row.get(1)?))) { break found }
        assert!(Instant::now() < until, "deadline not armed");
        thread::sleep(Duration::from_millis(25));
    };
    harness.stop_supervisor_for_test();
    store.connection().execute("UPDATE delegations SET deadline_at='2000-01-01T00:00:00.000Z' WHERE lead_id=?1", [&lead]).unwrap();
    harness.restart_supervisor().unwrap();
    let notice = support::wait_for_inbound(&harness.home, &lead, "member_failed", Duration::from_secs(5));
    assert_eq!(notice["reason"], "timeout");
    assert_eq!(support::session_state(&harness.home, &member).0, "failed");
    let recorded: String = store.connection().query_row("SELECT deadline_at FROM delegations WHERE lead_id=?1", [&lead], |row| row.get(0)).unwrap();
    assert_ne!(recorded, original);
    assert_eq!(recorded, "2000-01-01T00:00:00.000Z");
    chat.close_input();
}
