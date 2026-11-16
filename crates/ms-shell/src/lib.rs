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

pub fn parse_shell_command(command: &str) -> Option<ParsedCommand> {
    let commands = split_pipeline(command)?;
    Some(ParsedCommand {
        kind: if commands.len() > 1 {
            "pipeline"
        } else if has_shell_operator_outside_quotes(command) {
            "compound"
        } else {
            "simple"
        },
        commands,
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
