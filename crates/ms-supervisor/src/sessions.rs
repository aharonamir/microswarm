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

pub struct SessionLifecycle;

impl SessionLifecycle {
    pub fn new(_restart_limit: u8) -> Self {
        todo!("T039: initialize a bounded persisted session lifecycle")
    }

    pub fn status(&self) -> SessionStatus {
        todo!("T039: expose current lifecycle status")
    }

    pub fn apply(&mut self, _event: SessionEvent) -> Result<SessionStatus, String> {
        todo!("T039: validate state transitions and fail closed")
    }
}
