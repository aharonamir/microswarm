use ms_proto::{AgentEvent, AgentEventType};
use ms_store::{SessionStore, StoreError};
use serde_json::Value;

pub fn build_event(
    _session_id: &str,
    _root_session_id: &str,
    _role: &str,
    _event_type: AgentEventType,
    _data: Value,
) -> AgentEvent {
    todo!("T033: construct and persist schema events")
}

pub fn emit(
    _store: &mut SessionStore,
    _session_id: &str,
    _root_session_id: &str,
    _role: &str,
    _event_type: AgentEventType,
    _data: Value,
) -> Result<(i64, AgentEvent), StoreError> {
    todo!("T033: append event and return its assigned sequence")
}
