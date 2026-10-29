#[path = "us3_support.rs"]
mod support;

use serde_json::Value;
use std::{thread, time::Instant};

#[test]
fn expired_send_deadline_is_persisted_and_member_is_torn_down() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn researcher and send hang").unwrap();
    let lead = support::wait_for_lead_id(&harness.home, std::time::Duration::from_secs(5));
    let deadline = Instant::now() + std::time::Duration::from_secs(6);
    loop {
        let store =
            ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
        let waiting: bool = store.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM delegations WHERE lead_id=?1 AND status='waiting_reply' AND deadline_at IS NOT NULL)",
            [&lead],|row|row.get(0),
        ).unwrap();
        if waiting {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "delegation never armed a reply deadline"
        );
        thread::sleep(std::time::Duration::from_millis(25));
    }
    let store = ms_store::SupervisorStore::open(harness.home.join("supervisor.db")).unwrap();
    store.connection().execute(
        "UPDATE delegations SET deadline_at='2000-01-01T00:00:00.000Z' WHERE lead_id=?1 AND status='waiting_reply'",
        [&lead],
    ).unwrap();
    let failure: Value = support::wait_for_inbound(
        &harness.home,
        &lead,
        "member_failed",
        std::time::Duration::from_secs(2),
    );
    assert_eq!(failure["reason"], "timeout");
    let delegation: (String, String, Option<String>, Option<String>) = store.connection().query_row(
        "SELECT d.member_id,d.status,d.deadline_at,d.failure_reason FROM delegations d WHERE d.lead_id=?1 ORDER BY d.created_at DESC LIMIT 1",
        [&lead],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).unwrap();
    assert_eq!(delegation.1, "failed");
    assert_eq!(delegation.2.as_deref(), Some("2000-01-01T00:00:00.000Z"));
    assert_eq!(delegation.3.as_deref(), Some("timeout"));
    let _ = chat.wait_for("agent ›", std::time::Duration::from_secs(3));
    chat.close_input();
    let member = delegation.0;
    support::wait_for_status(
        &harness.home,
        &member,
        "failed",
        std::time::Duration::from_secs(2),
    );
    let container = harness.containers_for(&member).unwrap().remove(0);
    assert!(!harness.container_running(&container).unwrap());
}
