use crate::config::Language;
use ms_proto::{ApprovalBinding, InboundKind};
use ms_store::{ApprovalRecord, SessionConnections, StoreError, SupervisorStore};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn request_tool_approval(
    store: &mut SupervisorStore,
    requesting_session_id: &str,
    root_session_id: &str,
    binding: &ApprovalBinding,
    arguments: &Value,
    timeout: Duration,
) -> Result<ApprovalRecord, StoreError> {
    let mut random = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let id = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let code = random[..4]
        .iter()
        .map(|byte| APPROVAL_ALPHABET[usize::from(byte & 31)] as char)
        .collect::<String>();
    let created_at = utc_timestamp();
    let expires_at = utc_after(timeout);
    let approval = ApprovalRecord {
        id,
        code,
        root_session_id: root_session_id.into(),
        requesting_session_id: requesting_session_id.into(),
        kind: "tool_call".into(),
        payload: json!({ "binding": binding, "arguments": arguments }),
        status: "pending".into(),
        expires_at,
        created_at,
    };
    store.insert_approval(&approval)?;
    Ok(approval)
}

pub fn resolve_by_code(
    store: &mut SupervisorStore,
    session: &SessionConnections,
    code: &str,
    approve: bool,
) -> Result<(), StoreError> {
    let approval = store
        .pending_approval(code)?
        .ok_or(StoreError::InvalidContract(
            "pending approval code was not found",
        ))?;
    let timed_out = approval.expires_at <= utc_timestamp();
    let decision = if timed_out {
        "timed_out"
    } else if approve {
        "approved"
    } else {
        "denied"
    };
    if !store.resolve_approval(&approval.id, decision, &utc_timestamp())? {
        return Err(StoreError::InvalidContract("approval was already resolved"));
    }
    let body = json!({
        "approval_id": approval.id,
        "decision": decision,
        "binding": approval.payload.get("binding").cloned().unwrap_or(Value::Null),
    });
    session.append_inbound(&InboundKind::ApprovalResult, &body)?;
    Ok(())
}

pub fn expire_due(
    _store: &mut SupervisorStore,
    _session_id: &str,
    _session: &SessionConnections,
) -> Result<usize, StoreError> {
    todo!("T042: automatically deny and acknowledge expired approvals")
}

pub fn render_prompt(
    language: Language,
    code: &str,
    server: &str,
    tool: &str,
    arguments: &Value,
) -> String {
    match language {
        Language::En => format!(
            "Approve {server}/{tool} with arguments {arguments}? Reply: microswarm approve {code} or microswarm deny {code}."
        ),
        Language::Zh => format!(
            "批准 {server}/{tool}，参数为 {arguments}？回复：microswarm approve {code} 或 microswarm deny {code}。"
        ),
    }
}

const APPROVAL_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

fn utc_after(duration: Duration) -> String {
    let time = SystemTime::now() + duration;
    utc_timestamp_at(time)
}

fn utc_timestamp() -> String {
    utc_timestamp_at(SystemTime::now())
}

fn utc_timestamp_at(time: SystemTime) -> String {
    let elapsed = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let days = (elapsed.as_secs() / 86_400) as i64;
    let second_of_day = elapsed.as_secs() % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        elapsed.subsec_millis()
    )
}
