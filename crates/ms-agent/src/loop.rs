use crate::boot::AgentContext;
use ms_proto::{
    AgentEventType, ExitReason, InboundKind, InboundMessage, OutboundMessage, exit_code,
};
use ms_store::HistoryEntry;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::signal::unix::{SignalKind, signal};

#[derive(Debug)]
pub struct TurnOutput {
    pub outbound: Vec<OutboundMessage>,
    pub history: Vec<HistoryEntry>,
}

pub trait TurnProcessor {
    fn process<'a>(
        &'a mut self,
        context: &'a mut AgentContext,
        message: InboundMessage,
        history: Vec<HistoryEntry>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnOutput, TurnError>> + 'a>>;
}

#[derive(Debug)]
pub enum TurnError {
    Config(String),
    Transient(String),
}

impl TurnError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => 78,
            Self::Transient(_) => 75,
        }
    }
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(message) | Self::Transient(message) => f.write_str(message),
        }
    }
}

#[derive(Debug)]
pub enum LoopError {
    Signal(std::io::Error),
    Terminated,
    Store(ms_store::StoreError),
    Turn(TurnError),
}

impl LoopError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Turn(error) => error.exit_code(),
            Self::Terminated => exit_code(ExitReason::Safety),
            Self::Signal(_) => exit_code(ExitReason::Safety),
            Self::Store(error) if error.is_transient() => exit_code(ExitReason::Transient),
            Self::Store(_) => exit_code(ExitReason::Safety),
        }
    }
}

impl std::fmt::Display for LoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => write!(formatter, "cannot subscribe to SIGUSR1: {error}"),
            Self::Terminated => formatter.write_str("agent received SIGTERM"),
            Self::Store(error) => write!(formatter, "session store failed: {error}"),
            Self::Turn(error) => write!(formatter, "turn failed: {error}"),
        }
    }
}

impl std::error::Error for LoopError {}

pub async fn run<P: TurnProcessor>(
    mut context: AgentContext,
    mut processor: P,
) -> Result<(), LoopError> {
    let mut wake = signal(SignalKind::user_defined1()).map_err(LoopError::Signal)?;
    let mut terminate = signal(SignalKind::terminate()).map_err(LoopError::Signal)?;
    loop {
        let cursor = context.store.cursor().map_err(LoopError::Store)?;
        if let Some(message) = context
            .store
            .next_inbound(cursor)
            .map_err(LoopError::Store)?
        {
            let inbound_id = message
                .id
                .expect("stored inbound messages always have an id");
            let invalid_reason = match &message.kind {
                InboundKind::UserMessage
                    if !message.body.get("text").is_some_and(|v| v.is_string()) =>
                {
                    Some("user_message requires a string text field".to_owned())
                }
                InboundKind::Task if !message.body.get("text").is_some_and(|v| v.is_string()) => {
                    Some("task requires a string text field".to_owned())
                }
                InboundKind::UserMessage | InboundKind::Task | InboundKind::Control => None,
                InboundKind::ApprovalResult
                    if message
                        .body
                        .get("approval_id")
                        .is_some_and(|v| v.is_string())
                        && matches!(
                            message.body.get("decision").and_then(|v| v.as_str()),
                            Some("approved" | "denied" | "timed_out" | "expired")
                        )
                        && message.body.get("binding").is_some_and(|v| v.is_object()) =>
                {
                    None
                }
                InboundKind::ApprovalResult => Some("approval_result is malformed".to_owned()),
                InboundKind::Unknown(kind) => Some(format!("unknown inbound kind: {kind}")),
                kind => Some(format!(
                    "inbound kind is not handled by this agent: {kind:?}"
                )),
            };
            if let Some(reason) = invalid_reason {
                crate::emit::emit(
                    &mut context.store,
                    &context.session_id,
                    &context.root_session_id,
                    &context.role,
                    AgentEventType::InboundError,
                    json!({ "inbound_id": inbound_id, "kind": message.kind, "error": reason }),
                )
                .map_err(LoopError::Store)?;
                context
                    .store
                    .commit_turn(inbound_id, &[], &[])
                    .map_err(LoopError::Store)?;
                continue;
            }
            if message.kind == InboundKind::Control
                && message.body.get("op").and_then(|op| op.as_str()) == Some("shutdown")
            {
                context
                    .store
                    .commit_turn(inbound_id, &[], &[])
                    .map_err(LoopError::Store)?;
                return Ok(());
            }
            if message.kind == InboundKind::Control {
                crate::emit::emit(
                    &mut context.store,
                    &context.session_id,
                    &context.root_session_id,
                    &context.role,
                    AgentEventType::InboundError,
                    json!({ "inbound_id": inbound_id, "kind": "control", "error": "unsupported or malformed control message" }),
                ).map_err(LoopError::Store)?;
                context
                    .store
                    .commit_turn(inbound_id, &[], &[])
                    .map_err(LoopError::Store)?;
                continue;
            }

            let history = context.store.history().map_err(LoopError::Store)?;
            let output = tokio::select! {
                result = processor.process(&mut context, message, history) => result.map_err(LoopError::Turn)?,
                _ = terminate.recv() => return Err(LoopError::Terminated),
            };
            context
                .store
                .commit_turn(inbound_id, &output.outbound, &output.history)
                .map_err(LoopError::Store)?;
            continue;
        }

        tokio::select! {
            _ = wake.recv() => {},
            _ = terminate.recv() => return Err(LoopError::Terminated),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {},
        }
    }
}
