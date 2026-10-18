use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentSource {
    #[default]
    #[serde(rename = "agent")]
    Agent,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum SupervisorSource {
    #[default]
    #[serde(rename = "supervisor")]
    Supervisor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentEventType {
    #[serde(rename = "agent.ready")]
    Ready,
    #[serde(rename = "agent.exit")]
    Exit,
    #[serde(rename = "turn.start")]
    TurnStart,
    #[serde(rename = "turn.end")]
    TurnEnd,
    #[serde(rename = "model.call")]
    ModelCall,
    #[serde(rename = "tool.call")]
    ToolCall,
    #[serde(rename = "tool.result")]
    ToolResult,
    #[serde(rename = "permit.decision")]
    PermitDecision,
    #[serde(rename = "delegation.request")]
    DelegationRequest,
    #[serde(rename = "delegation.send")]
    DelegationSend,
    #[serde(rename = "delegation.dismiss")]
    DelegationDismiss,
    #[serde(rename = "inbound.error")]
    InboundError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SupervisorEventType {
    #[serde(rename = "session.created")]
    SessionCreated,
    #[serde(rename = "session.status")]
    SessionStatus,
    #[serde(rename = "session.closed")]
    SessionClosed,
    #[serde(rename = "container.created")]
    ContainerCreated,
    #[serde(rename = "container.started")]
    ContainerStarted,
    #[serde(rename = "container.exited")]
    ContainerExited,
    #[serde(rename = "container.restart")]
    ContainerRestart,
    #[serde(rename = "container.adopted")]
    ContainerAdopted,
    #[serde(rename = "container.orphan_stopped")]
    ContainerOrphanStopped,
    #[serde(rename = "scope.written")]
    ScopeWritten,
    #[serde(rename = "scope.narrowed")]
    ScopeNarrowed,
    #[serde(rename = "delegation.policy")]
    DelegationPolicy,
    #[serde(rename = "delegation.spawned")]
    DelegationSpawned,
    #[serde(rename = "delegation.denied")]
    DelegationDenied,
    #[serde(rename = "delegation.reply")]
    DelegationReply,
    #[serde(rename = "delegation.timeout")]
    DelegationTimeout,
    #[serde(rename = "delegation.failed")]
    DelegationFailed,
    #[serde(rename = "delegation.dismissed")]
    DelegationDismissed,
    #[serde(rename = "approval.requested")]
    ApprovalRequested,
    #[serde(rename = "approval.resolved")]
    ApprovalResolved,
    #[serde(rename = "approval.expired")]
    ApprovalExpired,
    #[serde(rename = "approval.reissued")]
    ApprovalReissued,
    #[serde(rename = "message.inbound")]
    MessageInbound,
    #[serde(rename = "message.delivered")]
    MessageDelivered,
    #[serde(rename = "proxy.request")]
    ProxyRequest,
    #[serde(rename = "proxy.denied")]
    ProxyDenied,
    #[serde(rename = "supervisor.started")]
    SupervisorStarted,
    #[serde(rename = "supervisor.recovered")]
    SupervisorRecovered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEvent {
    pub seq: i64,
    pub ts: String,
    pub ts_mono: u64,
    pub session_id: String,
    pub root_session_id: String,
    pub agent_role: String,
    #[serde(default)]
    pub source: AgentSource,
    #[serde(rename = "type")]
    pub event_type: AgentEventType,
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorEvent {
    pub seq: i64,
    pub ts: String,
    pub ts_mono: u64,
    pub session_id: String,
    pub root_session_id: String,
    pub agent_role: String,
    #[serde(default)]
    pub source: SupervisorSource,
    #[serde(rename = "type")]
    pub event_type: SupervisorEventType,
    pub data: Value,
}
