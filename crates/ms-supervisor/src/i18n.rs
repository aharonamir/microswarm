use crate::config::Language;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKey {
    SupervisorStarted,
    SessionStarting,
    SessionReady,
    SessionStopped,
    SessionFailed,
    RuntimeUnavailable,
    ConfigInvalid,
    PermissionDenied,
    ApprovalRequested,
    ApprovalExpired,
    ApprovalDenied,
    ScopeNarrowed,
    MemberStarted,
    MemberFailed,
    MemberTimedOut,
    RetryExhausted,
    SessionRecovered,
    OrphanStopped,
    NoSessions,
}

pub const fn all_message_keys() -> &'static [MessageKey] {
    use MessageKey::*;
    &[
        SupervisorStarted,
        SessionStarting,
        SessionReady,
        SessionStopped,
        SessionFailed,
        RuntimeUnavailable,
        ConfigInvalid,
        PermissionDenied,
        ApprovalRequested,
        ApprovalExpired,
        ApprovalDenied,
        ScopeNarrowed,
        MemberStarted,
        MemberFailed,
        MemberTimedOut,
        RetryExhausted,
        SessionRecovered,
        OrphanStopped,
        NoSessions,
    ]
}

pub const fn translate(language: Language, key: MessageKey) -> &'static str {
    use Language::{En, Zh};
    use MessageKey::*;
    match (language, key) {
        (En, SupervisorStarted) => "MicroSwarm supervisor started.",
        (Zh, SupervisorStarted) => "MicroSwarm 监督进程已启动。",
        (En, SessionStarting) => "Starting the isolated agent session.",
        (Zh, SessionStarting) => "正在启动隔离的智能体会话。",
        (En, SessionReady) => "The agent session is ready.",
        (Zh, SessionReady) => "智能体会话已就绪。",
        (En, SessionStopped) => "The session has stopped.",
        (Zh, SessionStopped) => "会话已停止。",
        (En, SessionFailed) => "The session failed safely and will not be restarted.",
        (Zh, SessionFailed) => "会话已安全失败，不会自动重启。",
        (En, RuntimeUnavailable) => "The container runtime is unavailable.",
        (Zh, RuntimeUnavailable) => "容器运行时不可用。",
        (En, ConfigInvalid) => "Configuration is invalid; no agent was started.",
        (Zh, ConfigInvalid) => "配置无效；未启动智能体。",
        (En, PermissionDenied) => "The requested action was denied by policy.",
        (Zh, PermissionDenied) => "策略拒绝了此操作。",
        (En, ApprovalRequested) => "This action needs your approval.",
        (Zh, ApprovalRequested) => "此操作需要你批准。",
        (En, ApprovalExpired) => "The approval request expired and was denied.",
        (Zh, ApprovalExpired) => "批准请求已过期并被拒绝。",
        (En, ApprovalDenied) => "The action was denied.",
        (Zh, ApprovalDenied) => "此操作已被拒绝。",
        (En, ScopeNarrowed) => "The requested scope was narrowed by policy.",
        (Zh, ScopeNarrowed) => "请求的权限范围已按策略缩小。",
        (En, MemberStarted) => "The team member session started.",
        (Zh, MemberStarted) => "团队成员会话已启动。",
        (En, MemberFailed) => "The team member failed.",
        (Zh, MemberFailed) => "团队成员执行失败。",
        (En, MemberTimedOut) => "The team member exceeded its reply deadline.",
        (Zh, MemberTimedOut) => "团队成员已超过回复期限。",
        (En, RetryExhausted) => "The session exhausted its restart attempts.",
        (Zh, RetryExhausted) => "会话已用尽重启次数。",
        (En, SessionRecovered) => "The supervisor recovered the session after restart.",
        (Zh, SessionRecovered) => "监督进程重启后已恢复会话。",
        (En, OrphanStopped) => "An unmanaged container was stopped.",
        (Zh, OrphanStopped) => "未关联会话的容器已停止。",
        (En, NoSessions) => "There are no active sessions.",
        (Zh, NoSessions) => "当前没有活动会话。",
    }
}
