use ms_proto::ApprovalBinding;
use ms_store::{ApprovalRecord, SessionConnections, StoreError, SupervisorStore};
use std::time::Duration;

pub fn request_tool_approval(
    _store: &mut SupervisorStore,
    _requesting_session_id: &str,
    _root_session_id: &str,
    _binding: &ApprovalBinding,
    _timeout: Duration,
) -> Result<ApprovalRecord, StoreError> {
    todo!("T042: persist a short-code approval bound to one exact call")
}

pub fn resolve_by_code(
    _store: &mut SupervisorStore,
    _session: &SessionConnections,
    _code: &str,
    _approve: bool,
) -> Result<(), StoreError> {
    todo!("T042: resolve pending approvals and append approval_result")
}

pub fn render_prompt(
    _language: crate::config::Language,
    _code: &str,
    _server: &str,
    _tool: &str,
    _arguments: &serde_json::Value,
) -> String {
    todo!("T042: render a localized approval prompt")
}
