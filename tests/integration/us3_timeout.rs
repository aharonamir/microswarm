#[path = "us3_support.rs"]
mod support;

use serde_json::Value;

#[test]
#[ignore = "red: T069 — delegation reply deadlines must persist and tear down members"]
fn expired_send_deadline_is_persisted_and_member_is_torn_down() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher and send hang"])
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lead = support::lead_id(&harness.home);
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let delegation: (String, String, Option<String>, Option<String>) = store.connection().query_row(
        "SELECT d.member_id,d.status,d.deadline_at,d.failure_reason FROM delegations d WHERE d.lead_id=?1 ORDER BY d.created_at DESC LIMIT 1",
        [&lead],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).unwrap();
    assert_eq!(delegation.1, "failed");
    assert!(delegation.2.is_some(), "deadline_at was not persisted");
    assert_eq!(delegation.3.as_deref(), Some("timeout"));
    let failure: Value = support::wait_for_inbound(
        &harness.home,
        &lead,
        "member_failed",
        std::time::Duration::from_secs(2),
    );
    assert_eq!(failure["reason"], "timeout");
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
