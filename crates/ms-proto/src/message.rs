use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InboundKind {
    UserMessage,
    ApprovalResult,
    DelegationResult,
    MemberReply,
    MemberFailed,
    Task,
    Control,
    Unknown(String),
}

impl Serialize for InboundKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let kind = match self {
            Self::UserMessage => "user_message",
            Self::ApprovalResult => "approval_result",
            Self::DelegationResult => "delegation_result",
            Self::MemberReply => "member_reply",
            Self::MemberFailed => "member_failed",
            Self::Task => "task",
            Self::Control => "control",
            Self::Unknown(kind) => kind,
        };
        serializer.serialize_str(kind)
    }
}

impl<'de> Deserialize<'de> for InboundKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let kind = String::deserialize(deserializer)?;
        Ok(match kind.as_str() {
            "user_message" => Self::UserMessage,
            "approval_result" => Self::ApprovalResult,
            "delegation_result" => Self::DelegationResult,
            "member_reply" => Self::MemberReply,
            "member_failed" => Self::MemberFailed,
            "task" => Self::Task,
            "control" => Self::Control,
            _ => Self::Unknown(kind),
        })
    }
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
