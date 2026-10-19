use ms_proto::ExitReason;
use ms_store::{SessionLifecycleUpdate, StoreError, SupervisorStore};

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
                let reason = classify_exit(code, oom_killed);
                self.failure_reason = Some(reason);
                if reason == ExitReason::Transient && self.restart_count < self.restart_limit {
                    self.restart_count += 1;
                    SessionStatus::Crashed
                } else {
                    SessionStatus::Failed
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

    pub fn persist(
        &self,
        store: &mut SupervisorStore,
        session_id: &str,
        container_id: Option<&str>,
        timestamp: &str,
    ) -> Result<(), StoreError> {
        let status = self.status.as_str();
        let terminal = matches!(self.status, SessionStatus::Closed | SessionStatus::Failed);
        let close_reason = match (self.status, self.failure_reason) {
            (SessionStatus::Closed, _) => Some("session_end"),
            (SessionStatus::Failed, Some(ExitReason::Safety)) => Some("safety"),
            (SessionStatus::Failed, _) => Some("failed"),
            _ => None,
        };
        store.update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: session_id.to_owned(),
            status: status.to_owned(),
            container_id: container_id.map(str::to_owned),
            restart_count: self.restart_count,
            last_active_at: timestamp.to_owned(),
            closed_at: terminal.then(|| timestamp.to_owned()),
            close_reason: close_reason.map(str::to_owned),
        })
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
