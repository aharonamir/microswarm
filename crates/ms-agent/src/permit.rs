use ms_proto::{ApprovalBinding, Permission, Scope};
pub use ms_shell::{
    ApprovalPrompt, ParsedCommand, PermitAction, PermitDecision, ToolError, evaluate_shell,
};
use rmcp::model::JsonObject;
use serde_json::Value;

/// Opaque authority token. Its fields and constructor are private to this module.
#[derive(Debug)]
pub struct PermittedToolCall {
    server: String,
    tool: String,
    arguments: Option<JsonObject>,
}

impl PermittedToolCall {
    pub(crate) fn server(&self) -> &str {
        &self.server
    }
    pub(crate) fn tool(&self) -> &str {
        &self.tool
    }
    pub(crate) fn arguments(&mut self) -> Option<JsonObject> {
        self.arguments.take()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermitError {
    NotGranted,
    Denied,
    ApprovalRequired,
    ApprovalMismatch,
    UnparseableShell(ToolError),
}

impl std::fmt::Display for PermitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotGranted => f.write_str("tool is outside the session scope"),
            Self::Denied => f.write_str("tool policy denies this call"),
            Self::ApprovalRequired => f.write_str("tool call requires approval"),
            Self::ApprovalMismatch => f.write_str("approval does not match this tool call"),
            Self::UnparseableShell(error) => write!(f, "{}: {}", error.code, error.message),
        }
    }
}

pub fn authorize_approved_tool(
    scope: &Scope,
    server: &str,
    tool: &str,
    arguments: Option<JsonObject>,
    approved: &ApprovalBinding,
) -> Result<PermittedToolCall, PermitError> {
    let grant = scope
        .tools
        .iter()
        .find(|grant| grant.server == server && grant.tools.contains(tool))
        .ok_or(PermitError::NotGranted)?;
    let policy = grant.overrides.get(tool).copied().unwrap_or(grant.policy);
    match policy {
        Permission::Deny => return Err(PermitError::Denied),
        Permission::Allow => return authorize_tool(scope, server, tool, arguments),
        Permission::Ask => {}
    }
    let arguments_value = arguments.clone().map(Value::Object);
    if !approved.matches_call(server, tool, arguments_value.as_ref()) {
        return Err(PermitError::ApprovalMismatch);
    }
    if tool == "shell.run" {
        let command = arguments
            .as_ref()
            .and_then(|args| args.get("command"))
            .and_then(Value::as_str)
            .ok_or(PermitError::Denied)?;
        if evaluate_shell(command).action == PermitAction::Deny {
            return Err(PermitError::Denied);
        }
    }
    Ok(PermittedToolCall {
        server: server.into(),
        tool: tool.into(),
        arguments,
    })
}

impl std::error::Error for PermitError {}

pub fn authorize_tool(
    scope: &Scope,
    server: &str,
    tool: &str,
    arguments: Option<JsonObject>,
) -> Result<PermittedToolCall, PermitError> {
    let grant = scope
        .tools
        .iter()
        .find(|grant| grant.server == server && grant.tools.contains(tool))
        .ok_or(PermitError::NotGranted)?;
    let policy = grant.overrides.get(tool).copied().unwrap_or(grant.policy);
    match policy {
        Permission::Deny => return Err(PermitError::Denied),
        Permission::Ask => return Err(PermitError::ApprovalRequired),
        Permission::Allow => {}
    }
    if tool == "shell.run" {
        let command = arguments
            .as_ref()
            .and_then(|args| args.get("command"))
            .and_then(Value::as_str)
            .ok_or(PermitError::Denied)?;
        match evaluate_shell(command).action {
            PermitAction::Allow => {}
            PermitAction::Ask => return Err(PermitError::ApprovalRequired),
            PermitAction::Deny => return Err(PermitError::Denied),
        }
    }
    Ok(PermittedToolCall {
        server: server.into(),
        tool: tool.into(),
        arguments,
    })
}
