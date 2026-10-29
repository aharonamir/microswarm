use ms_proto::ExitReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Pending,
    Starting,
    Ready,
    Busy,
    Stopping,
    Crashed,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    Start,
    AgentReady,
    TurnStarted,
    TurnFinished,
    Shutdown,
    Restart,
    ContainerExited { code: i32, oom_killed: bool },
    ContainerExit { reason: ExitReason },
}

pub struct SessionLifecycle {
    status: SessionStatus,
    restart_count: u8,
    restart_limit: u8,
    failure_reason: Option<ExitReason>,
}

impl SessionLifecycle {
    pub fn new(restart_limit: u8) -> Self {
        Self {
            status: SessionStatus::Pending,
            restart_count: 0,
            restart_limit,
            failure_reason: None,
        }
    }

    pub fn restore(status: &str, restart_count: u8, restart_limit: u8) -> Result<Self, String> {
        let status = SessionStatus::parse(status)
            .ok_or_else(|| format!("unknown persisted session status {status:?}"))?;
        Ok(Self {
            status,
            restart_count,
            restart_limit,
            failure_reason: None,
        })
    }

    pub fn status(&self) -> SessionStatus {
        self.status
    }

    pub fn restart_count(&self) -> u8 {
        self.restart_count
    }

    pub fn failure_reason(&self) -> Option<ExitReason> {
        self.failure_reason
    }

    pub fn apply(&mut self, event: SessionEvent) -> Result<SessionStatus, String> {
        let next = match event {
            SessionEvent::Start if self.status == SessionStatus::Pending => SessionStatus::Starting,
            SessionEvent::AgentReady if self.status == SessionStatus::Starting => {
                SessionStatus::Ready
            }
            SessionEvent::TurnStarted if self.status == SessionStatus::Ready => SessionStatus::Busy,
            SessionEvent::TurnFinished if self.status == SessionStatus::Busy => {
                SessionStatus::Ready
            }
            SessionEvent::Shutdown if self.status == SessionStatus::Pending => {
                SessionStatus::Closed
            }
            SessionEvent::Shutdown
                if matches!(
                    self.status,
                    SessionStatus::Starting | SessionStatus::Ready | SessionStatus::Busy
                ) =>
            {
                SessionStatus::Stopping
            }
            SessionEvent::Restart if self.status == SessionStatus::Crashed => {
                SessionStatus::Starting
            }
            SessionEvent::ContainerExited { code: 0, .. }
                if !matches!(self.status, SessionStatus::Closed | SessionStatus::Failed) =>
            {
                SessionStatus::Closed
            }
            SessionEvent::ContainerExited { code, oom_killed }
                if !matches!(self.status, SessionStatus::Closed | SessionStatus::Failed) =>
            {
                self.record_failure(classify_exit(code, oom_killed))
            }
            SessionEvent::ContainerExit { reason }
                if !matches!(self.status, SessionStatus::Closed | SessionStatus::Failed) =>
            {
                if reason == ExitReason::Ok {
                    SessionStatus::Closed
                } else {
                    self.record_failure(reason)
                }
            }
            _ => {
                return Err(format!(
                    "invalid session transition: {:?} + {:?}",
                    self.status, event
                ));
            }
        };
        self.status = next;
        Ok(next)
    }

    fn record_failure(&mut self, reason: ExitReason) -> SessionStatus {
        self.failure_reason = Some(reason);
        if reason == ExitReason::Transient && self.restart_count < self.restart_limit {
            self.restart_count += 1;
            SessionStatus::Crashed
        } else {
            SessionStatus::Failed
        }
    }
}

impl SessionStatus {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "starting" => Self::Starting,
            "ready" => Self::Ready,
            "busy" => Self::Busy,
            "stopping" => Self::Stopping,
            "crashed" => Self::Crashed,
            "closed" => Self::Closed,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Busy => "busy",
            Self::Stopping => "stopping",
            Self::Crashed => "crashed",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }
}

pub const fn classify_exit(code: i32, oom_killed: bool) -> ExitReason {
    match code {
        0 => ExitReason::Ok,
        75 => ExitReason::Transient,
        137 if oom_killed => ExitReason::Transient,
        78 => ExitReason::Config,
        77 => ExitReason::Safety,
        _ => ExitReason::Safety,
    }
}

pub fn terminal_notice(
    close_reason: Option<&str>,
    agent_reason: Option<&str>,
    restarts: u8,
) -> &'static str {
    if close_reason == Some("safety") {
        "safety"
    } else if agent_reason == Some("config") {
        "config"
    } else if restarts > 0 {
        "retry_exhausted"
    } else {
        "safety"
    }
}

pub fn classify_exit_with_agent_reason(
    code: i32,
    oom_killed: bool,
    agent_reason: Option<&str>,
) -> ExitReason {
    let runtime_reason = classify_exit(code, oom_killed);
    let Some(agent_reason) = agent_reason else {
        return runtime_reason;
    };
    let agent_reason = match agent_reason {
        "ok" => ExitReason::Ok,
        "transient" => ExitReason::Transient,
        "config" => ExitReason::Config,
        "safety" => ExitReason::Safety,
        _ => ExitReason::Safety,
    };
    if exit_reason_rank(agent_reason) > exit_reason_rank(runtime_reason) {
        agent_reason
    } else {
        runtime_reason
    }
}

const fn exit_reason_rank(reason: ExitReason) -> u8 {
    match reason {
        ExitReason::Ok => 0,
        ExitReason::Transient => 1,
        ExitReason::Config => 2,
        ExitReason::Safety => 3,
    }
}
