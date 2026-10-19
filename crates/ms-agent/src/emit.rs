use ms_proto::{AgentEvent, AgentEventType};
use ms_store::{SessionStore, StoreError};
use serde_json::Value;

pub fn build_event(
    session_id: &str,
    root_session_id: &str,
    role: &str,
    event_type: AgentEventType,
    data: Value,
) -> AgentEvent {
    AgentEvent {
        seq: 0,
        ts: crate::boot::utc_timestamp(),
        ts_mono: crate::boot::monotonic_nanos(),
        session_id: session_id.to_owned(),
        root_session_id: root_session_id.to_owned(),
        agent_role: role.to_owned(),
        source: Default::default(),
        event_type,
        data,
    }
}

pub fn emit(
    store: &mut SessionStore,
    session_id: &str,
    root_session_id: &str,
    role: &str,
    event_type: AgentEventType,
    data: Value,
) -> Result<(i64, AgentEvent), StoreError> {
    let mut event = build_event(session_id, root_session_id, role, event_type, data);
    event.seq = store.append_agent_event(&event)?;
    Ok((event.seq, event))
}
