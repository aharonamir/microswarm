#[path = "../../crates/ms-cli/tests/common/mod.rs"]
mod common;

use jsonschema::validator_for;
use serde_json::Value;

#[test]
fn trace_json_output_validates_against_event_contract() {
    let home = common::fixture();
    let output = common::trace(&home, &["--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let schema: Value = serde_json::from_str(include_str!(
        "../../specs/001-microswarm/contracts/events.schema.json"
    ))
    .unwrap();
    let validator = validator_for(&schema).unwrap();
    assert_eq!(events.len(), 3);
    for event in events {
        assert!(validator.is_valid(&event), "invalid trace event: {event}");
    }
    common::cleanup(&home);
}
