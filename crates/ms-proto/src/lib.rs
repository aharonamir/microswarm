//! Shared MicroSwarm crate.
mod approval;
mod event;
mod exit;
mod message;
mod scope;

pub use approval::ApprovalBinding;
pub use event::{AgentEvent, AgentEventType, SupervisorEvent, SupervisorEventType};
pub use exit::{ExitReason, exit_code};
pub use message::{InboundKind, InboundMessage, OutboundKind, OutboundMessage};
pub use scope::{
    DelegationScope, DerivedFrom, Limits, Mount, MountMode, NarrowedDimension, NarrowedItem,
    NarrowedReason, Permission, Scope, ScopeCeiling, ScopeError, ToolGrant, ToolNames,
    intersect_scope,
};
