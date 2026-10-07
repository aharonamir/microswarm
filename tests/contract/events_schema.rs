use jsonschema::validator_for;
use ms_proto::{AgentEvent, SupervisorEvent};
use serde_json::{Value, json};

const SCHEMA: &str = include_str!("../../specs/001-microswarm/contracts/events.schema.json");
const SESSION: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn agent_event(kind: &str, data: Value) -> Result<Value, serde_json::Error> {
    let event: AgentEvent = serde_json::from_value(json!({
        "seq": 1, "ts": "2026-10-07T00:00:00Z", "ts_mono": 0,
        "session_id": SESSION, "root_session_id": SESSION, "agent_role": "lead",
        "type": kind, "data": data
    }))?;
    serde_json::to_value(event)
}

fn supervisor_event(kind: &str, data: Value) -> Result<Value, serde_json::Error> {
    let event: SupervisorEvent = serde_json::from_value(json!({
        "seq": 1, "ts": "2026-10-07T00:00:00Z", "ts_mono": 0,
        "session_id": SESSION, "root_session_id": SESSION, "agent_role": "lead",
        "type": kind, "data": data
    }))?;
    serde_json::to_value(event)
}

#[test]
fn every_agent_event_round_trips_and_validates_only_as_an_agent_event() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = validator_for(&schema).unwrap();
    let cases = [
        ("agent.ready", json!({})),
        ("agent.exit", json!({"reason":"ok","code":0})),
        ("turn.start", json!({})),
        ("turn.end", json!({})),
        (
            "model.call",
            json!({"model":"fake","input_tokens":1,"output_tokens":1,"duration_ms":1}),
        ),
        (
            "tool.call",
            json!({"call_id":"c1","server":"s","tool":"t","args":{}}),
        ),
        (
            "tool.result",
            json!({"call_id":"c1","ok":true,"duration_ms":1}),
        ),
        (
            "permit.decision",
            json!({"call_id":"c1","decision":"allow","rule":"default"}),
        ),
        ("delegation.request", json!({})),
        ("delegation.send", json!({})),
        ("delegation.dismiss", json!({})),
    ];
    for (kind, data) in cases {
        let event = agent_event(kind, data).unwrap();
        assert!(
            validator.is_valid(&event),
            "agent event {kind} must satisfy schema: {event}"
        );
        assert!(
            supervisor_event(kind, json!({})).is_err(),
            "agent type {kind} must not deserialize as supervisor event"
        );
    }
}

#[test]
fn every_supervisor_event_round_trips_and_validates_only_as_a_supervisor_event() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = validator_for(&schema).unwrap();
    let cases = [
        ("session.created", json!({})),
        ("session.status", json!({})),
        ("session.closed", json!({})),
        ("container.created", json!({})),
        ("container.started", json!({})),
        (
            "container.exited",
            json!({"container_id":"c1","exit_code":0,"classified_as":"ok"}),
        ),
        ("container.restart", json!({})),
        ("container.adopted", json!({})),
        ("container.orphan_stopped", json!({})),
        ("scope.written", json!({})),
        (
            "scope.narrowed",
            json!({"delegation_id":"d1","narrowed":[]}),
        ),
        ("delegation.policy", json!({})),
        ("delegation.spawned", json!({})),
        ("delegation.denied", json!({})),
        ("delegation.reply", json!({})),
        ("delegation.timeout", json!({})),
        ("delegation.failed", json!({})),
        ("delegation.dismissed", json!({})),
        ("approval.requested", json!({})),
        ("approval.resolved", json!({})),
        ("approval.expired", json!({})),
        ("approval.reissued", json!({})),
        ("message.inbound", json!({})),
        ("message.delivered", json!({})),
        ("proxy.request", json!({})),
        ("proxy.denied", json!({})),
        ("supervisor.started", json!({})),
        ("supervisor.recovered", json!({})),
    ];
    for (kind, data) in cases {
        let event = supervisor_event(kind, data).unwrap();
        assert!(
            validator.is_valid(&event),
            "supervisor event {kind} must satisfy schema: {event}"
        );
        assert!(
            agent_event(kind, json!({})).is_err(),
            "supervisor type {kind} must not deserialize as agent event"
        );
    }
}
