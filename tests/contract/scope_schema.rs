use jsonschema::validator_for;
use ms_proto::Scope;
use serde_json::{Value, json};

const SCHEMA: &str = include_str!("../../specs/001-microswarm/contracts/scope.schema.json");

fn valid_scope_value() -> Value {
    json!({
        "version": 1,
        "session_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "derived_from": "config",
        "source_ref": "config",
        "mounts": [],
        "credentials": [],
        "tools": [],
        "limits": { "memory_mb": 128, "pids": 32, "tmp_mb": 64, "max_turns": 8, "max_tokens": 4096 },
        "narrowed": [],
        "hash": format!("sha256:{}", "0".repeat(64))
    })
}

#[test]
fn scope_serializes_to_the_contract_and_hash_is_stable() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = validator_for(&schema).unwrap();
    let scope: Scope = serde_json::from_value(valid_scope_value()).unwrap();
    let same_scope: Scope = serde_json::from_value(valid_scope_value()).unwrap();
    let encoded = serde_json::to_value(&scope).unwrap();

    assert!(validator.is_valid(&encoded));
    assert_eq!(scope.canonical_hash(), same_scope.canonical_hash());
}
