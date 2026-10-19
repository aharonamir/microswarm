use ms_agent::emit::emit;
use ms_proto::AgentEventType;
use ms_store::{SessionStore, open_session_dir};
use serde_json::json;
use std::fs;

#[test]
fn emitter_builds_schema_events_with_session_identity_and_payload() {
    let dir = std::env::temp_dir().join(format!("ms-emitter-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let supervisor = open_session_dir(&dir).unwrap();
    let mut store = SessionStore::open_dir(&dir).unwrap();
    let (seq, event) = emit(
        &mut store,
        "session-1",
        "root-1",
        "lead",
        AgentEventType::ToolCall,
        json!({"server":"weather","tool":"forecast"}),
    )
    .unwrap();
    assert_eq!(seq, 1);
    assert_eq!(event.event_type, AgentEventType::ToolCall);
    assert_eq!(event.session_id, "session-1");
    assert_eq!(event.root_session_id, "root-1");
    assert_eq!(event.agent_role, "lead");
    assert_eq!(event.data["server"], "weather");
    assert!(!event.ts.is_empty());
    drop(store);
    drop(supervisor);
    fs::remove_dir_all(dir).unwrap();
}
