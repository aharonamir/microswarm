use crate::ContainerRuntime;
use ms_proto::{InboundKind, OutboundMessage};
use ms_store::{SessionConnections, SessionRecord, StoreError, SupervisorStore};

pub fn resolve_or_create<F>(
    store: &mut SupervisorStore,
    channel: &str,
    thread: &str,
    create: F,
) -> Result<String, StoreError>
where
    F: FnOnce() -> SessionRecord,
{
    if let Some(id) = store.active_lead_for_thread(channel, thread)? {
        return Ok(id);
    }
    let mut session = create();
    if session.kind != "lead" {
        return Err(StoreError::InvalidContract(
            "channel router can create only lead sessions",
        ));
    }
    session.channel = Some(channel.to_owned());
    session.channel_thread = Some(thread.to_owned());
    store.insert_session(&session)?;
    Ok(session.id)
}

pub fn append_inbound(
    session: &SessionConnections,
    kind: InboundKind,
    body: serde_json::Value,
) -> Result<i64, StoreError> {
    session.append_inbound(&kind, &body)
}

pub fn outbound_after(
    session: &SessionConnections,
    after_id: i64,
) -> Result<Vec<OutboundMessage>, StoreError> {
    session.outbound_after(after_id)
}

pub fn outbound_data_version(session: &SessionConnections) -> Result<i64, StoreError> {
    session.outbound_data_version()
}

pub fn tail_cursor(store: &SupervisorStore, session_id: &str) -> Result<(i64, i64), StoreError> {
    Ok(store.tail_cursor(session_id)?.unwrap_or_default())
}

pub fn persist_tail_cursor(
    store: &mut SupervisorStore,
    session_id: &str,
    outbound_last_id: i64,
    events_last_seq: i64,
) -> Result<(), StoreError> {
    store.upsert_tail_cursor(session_id, outbound_last_id, events_last_seq)
}

pub async fn signal_agent(runtime: &ContainerRuntime, container_id: &str) -> Result<(), String> {
    runtime.signal(container_id, "SIGUSR1").await
}
