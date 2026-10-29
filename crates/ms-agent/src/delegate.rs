use ms_proto::{AgentEventType, InboundKind, InboundMessage, OutboundKind, OutboundMessage};
use ms_store::SessionStore;
use serde_json::Value;
use std::time::{Duration, Instant};

const TOOL_CONTRACT: &str =
    include_str!("../../../specs/001-microswarm/contracts/delegation-tools.json");

pub fn tool_definitions(kind: &str, enabled: bool, _anthropic: bool) -> Vec<Value> {
    if kind != "lead" || !enabled {
        return Vec::new();
    }
    let contract: Value = serde_json::from_str(TOOL_CONTRACT)
        .expect("delegation-tools.json is valid JSON at compile time");
    contract["tools"]
        .as_array()
        .expect("delegation-tools.json has tools")
        .iter()
        .filter_map(|tool| {
            Some(serde_json::json!({
                "type":"function",
                "function": {
                    "name":format!("microswarm-core/{}", tool["name"].as_str()?),
                    "description":tool["description"],
                    "parameters":tool["input_schema"]
                }
            }))
        })
        .collect()
}

pub fn prepare_outbound(
    tool: &str,
    arguments: Value,
    in_reply_to: Option<i64>,
) -> Result<OutboundMessage, String> {
    let args = arguments
        .as_object()
        .ok_or_else(|| "delegation tool arguments must be an object".to_owned())?;
    let (kind, body) = match tool {
        "spawn_member" => {
            only_keys(
                args,
                &["role", "task", "timeout_s", "scope"],
                &["role", "task", "scope"],
            )?;
            nonempty_string(&arguments, "role")?;
            let task = nonempty_string(&arguments, "task")?;
            if task.len() > 8_000 {
                return Err("delegation task exceeds 8000 characters".into());
            }
            validate_timeout(&arguments)?;
            validate_scope_request(&arguments["scope"])?;
            (OutboundKind::DelegationRequest, arguments)
        }
        "send_to_member" => {
            only_keys(
                args,
                &["member_id", "text", "timeout_s"],
                &["member_id", "text"],
            )?;
            nonempty_string(&arguments, "member_id")?;
            nonempty_string(&arguments, "text")?;
            validate_timeout(&arguments)?;
            (OutboundKind::SendToMember, arguments)
        }
        "dismiss_member" => {
            only_keys(args, &["member_id"], &["member_id"])?;
            nonempty_string(&arguments, "member_id")?;
            (OutboundKind::Dismiss, arguments)
        }
        "list_members" => {
            only_keys(args, &[], &[])?;
            (
                OutboundKind::DelegationRequest,
                serde_json::json!({"op":"list_members"}),
            )
        }
        _ => return Err(format!("unknown delegation tool {tool:?}")),
    };
    Ok(OutboundMessage {
        id: None,
        kind,
        body,
        in_reply_to,
        created_at: None,
    })
}

pub fn write_outbound(store: &mut SessionStore, message: &OutboundMessage) -> Result<(), String> {
    store
        .append_outbound(message)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn event_for(message: &OutboundMessage) -> (AgentEventType, Value) {
    let event_type = match message.kind {
        OutboundKind::DelegationRequest => AgentEventType::DelegationRequest,
        OutboundKind::SendToMember => AgentEventType::DelegationSend,
        OutboundKind::Dismiss => AgentEventType::DelegationDismiss,
        OutboundKind::Reply | OutboundKind::ApprovalRequest => AgentEventType::DelegationRequest,
    };
    (event_type, message.body.clone())
}

pub fn expected_result_kind(tool: &str) -> Result<Vec<InboundKind>, String> {
    match tool {
        "spawn_member" | "dismiss_member" | "list_members" => {
            Ok(vec![InboundKind::DelegationResult])
        }
        "send_to_member" => Ok(vec![InboundKind::MemberReply, InboundKind::MemberFailed]),
        _ => Err(format!("unknown delegation tool {tool:?}")),
    }
}

pub async fn wait_for_result(
    store: &mut SessionStore,
    after_id: i64,
    expected: Vec<InboundKind>,
    timeout: Duration,
) -> Result<InboundMessage, String> {
    let deadline = Instant::now() + timeout;
    let cursor = after_id;
    loop {
        if let Some(message) = store
            .next_inbound(cursor)
            .map_err(|error| error.to_string())?
        {
            let id = message
                .id
                .ok_or_else(|| "supervisor result has no inbound id".to_owned())?;
            if expected.contains(&message.kind) {
                store
                    .acknowledge_inbound(id)
                    .map_err(|error| error.to_string())?;
                return Ok(message);
            }
            return Err(format!(
                "unexpected inbound {:?} while waiting for delegation result",
                message.kind
            ));
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for delegation result".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn only_keys(
    object: &serde_json::Map<String, Value>,
    allowed: &[&str],
    required: &[&str],
) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unexpected delegation argument {key:?}"));
    }
    if let Some(key) = required.iter().find(|key| !object.contains_key(**key)) {
        return Err(format!("missing delegation argument {key:?}"));
    }
    Ok(())
}

fn nonempty_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("delegation argument {key:?} must be a nonempty string"))
}

fn validate_timeout(value: &Value) -> Result<(), String> {
    if let Some(timeout) = value.get("timeout_s") {
        let timeout = timeout
            .as_u64()
            .ok_or_else(|| "timeout_s must be an integer".to_owned())?;
        if !(10..=3_600).contains(&timeout) {
            return Err("timeout_s must be between 10 and 3600".into());
        }
    }
    Ok(())
}

fn validate_scope_request(scope: &Value) -> Result<(), String> {
    let object = scope
        .as_object()
        .ok_or_else(|| "delegation scope must be an object".to_owned())?;
    only_keys(object, &["mounts", "credentials", "tools"], &[])?;
    for field in ["mounts", "credentials", "tools"] {
        if let Some(values) = scope.get(field) {
            let values = values
                .as_array()
                .ok_or_else(|| format!("delegation scope {field} must be an array"))?;
            for value in values {
                match field {
                    "mounts" => {
                        let item = value.as_object().ok_or("mount request must be an object")?;
                        only_keys(item, &["path", "mode"], &["path", "mode"])?;
                        nonempty_string(value, "path")?;
                        if !matches!(value["mode"].as_str(), Some("ro" | "rw")) {
                            return Err("mount mode must be ro or rw".into());
                        }
                    }
                    "credentials" => {
                        if !value.as_str().is_some_and(|name| !name.trim().is_empty()) {
                            return Err("credential requests must be nonempty strings".into());
                        }
                    }
                    "tools" => {
                        let item = value.as_object().ok_or("tool request must be an object")?;
                        only_keys(item, &["server", "tools"], &["server"])?;
                        nonempty_string(value, "server")?;
                        if let Some(tools) = value.get("tools")
                            && !tools.as_array().is_some_and(|tools| {
                                tools
                                    .iter()
                                    .all(|tool| tool.as_str().is_some_and(|s| !s.is_empty()))
                            })
                        {
                            return Err("requested tool names must be strings".into());
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(())
}
