use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboundKind {
    UserMessage,
    ApprovalResult,
    DelegationResult,
    MemberReply,
    MemberFailed,
    Task,
    Control,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InboundMessage {
    pub id: Option<i64>,
    pub kind: InboundKind,
    pub body: Value,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundKind {
    Reply,
    ApprovalRequest,
    DelegationRequest,
    SendToMember,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub id: Option<i64>,
    pub kind: OutboundKind,
    pub body: Value,
    pub in_reply_to: Option<i64>,
    pub created_at: Option<String>,
}
