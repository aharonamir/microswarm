#[path = "us3_support.rs"]
mod support;

use serde_json::Value;
use std::time::Duration;

#[test]
#[ignore = "red: T070 — runtime spawn errors must be routed as member failures"]
fn unavailable_runtime_records_spawn_error_for_the_lead() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    harness.chat(None, &["hello"]).unwrap();
    let config_path = harness.home.join("config/microswarm.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let config = config
        .lines()
        .map(|line| {
            if line.starts_with("image = ") {
                "image = \"microswarm-missing-agent:never\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, config).unwrap();
    let output = harness.chat(None, &["spawn researcher"]).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("delegation denied"));
    let lead = support::lead_id(&harness.home);
    let failure: Value = support::wait_for_inbound(
        &harness.home,
        &lead,
        "member_failed",
        Duration::from_secs(3),
    );
    assert_eq!(failure["reason"], "spawn_error");
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let reason: Option<String> = store.connection().query_row(
        "SELECT failure_reason FROM delegations WHERE lead_id=?1 ORDER BY created_at DESC LIMIT 1",
        [&lead],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(reason.as_deref(), Some("spawn_error"));
}
