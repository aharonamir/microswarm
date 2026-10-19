use ms_proto::{InboundKind, OutboundMessage};
use ms_store::{SessionConnections, SessionRecord, StoreError, SupervisorStore};

pub fn resolve_or_create<F>(
    _store: &mut SupervisorStore,
    _channel: &str,
    _thread: &str,
    _create: F,
) -> Result<String, StoreError>
where
    F: FnOnce() -> SessionRecord,
{
    todo!("T040: resolve an active lead by channel thread or create one")
}

pub fn append_inbound(
    _session: &SessionConnections,
    _kind: InboundKind,
    _body: serde_json::Value,
) -> Result<i64, StoreError> {
    todo!("T040: append a channel message to the supervisor-owned inbound database")
}

pub fn outbound_after(
    _session: &SessionConnections,
    _after_id: i64,
) -> Result<Vec<OutboundMessage>, StoreError> {
    todo!("T040: tail outbound rows after the stored cursor")
}
