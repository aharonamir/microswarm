use ms_proto::{ApprovalBinding, Permission, Scope};
use rmcp::model::JsonObject;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermitAction {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCommand {
    pub kind: &'static str,
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalPrompt {
    pub raw_command: String,
    pub parsed_structure: ParsedCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
    pub rewrite_guidance: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermitDecision {
    pub action: PermitAction,
    pub approval_prompt: Option<ApprovalPrompt>,
    pub tool_error: Option<ToolError>,
}

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

pub fn evaluate_shell(command: &str) -> PermitDecision {
    let Some(commands) = split_pipeline(command) else {
        return PermitDecision {
            action: PermitAction::Deny,
            approval_prompt: None,
            tool_error: Some(ToolError {
                code: "shell_parse_failed",
                message: "shell command could not be parsed".into(),
                rewrite_guidance:
                    "rewrite the command in a simpler form without unmatched quotes or escapes"
                        .into(),
            }),
        };
    };
    let complex = has_shell_operator_outside_quotes(command);
    let risky_program = commands.first().is_some_and(|first| {
        let program = first.split_whitespace().next().unwrap_or("");
        let basename = program.rsplit('/').next().unwrap_or(program);
        matches!(
            basename,
            "sh" | "bash"
                | "dash"
                | "zsh"
                | "ksh"
                | "fish"
                | "perl"
                | "ruby"
                | "node"
                | "php"
                | "lua"
                | "env"
                | "eval"
                | "exec"
                | "xargs"
                | "nohup"
                | "timeout"
                | "nice"
                | "sudo"
                | "su"
                | "doas"
                | "busybox"
        ) || basename.starts_with("python")
    });
    if complex || risky_program {
        PermitDecision {
            action: PermitAction::Ask,
            approval_prompt: Some(ApprovalPrompt {
                raw_command: command.into(),
                parsed_structure: ParsedCommand {
                    kind: if commands.len() > 1 {
                        "pipeline"
                    } else {
                        "compound"
                    },
                    commands,
                },
            }),
            tool_error: None,
        }
    } else {
        PermitDecision {
            action: PermitAction::Allow,
            approval_prompt: None,
            tool_error: None,
        }
    }
}

fn has_shell_operator_outside_quotes(command: &str) -> bool {
    let mut quote = None;
    let mut escaped = false;
    for ch in command.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '\n' | ';' | '&' | '|' | '<' | '>' | '`' | '$' | '(' | ')' | '{' | '}' => {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn split_pipeline(command: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in command.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '|' => {
                let part = command[start..index].trim();
                if part.is_empty() {
                    return None;
                }
                parts.push(part.to_owned());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if escaped || quote.is_some() {
        return None;
    }
    let last = command[start..].trim();
    if last.is_empty() {
        return None;
    }
    parts.push(last.to_owned());
    Some(parts)
}
