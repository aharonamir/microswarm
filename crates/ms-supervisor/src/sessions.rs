use ms_proto::ExitReason;
pub use ms_runtime::lifecycle::{
    SessionEvent, SessionStatus, classify_exit, classify_exit_with_agent_reason,
};
use ms_store::{SessionLifecycleUpdate, StoreError, SupervisorStore};

pub struct SessionLifecycle(ms_runtime::lifecycle::SessionLifecycle);

impl SessionLifecycle {
    pub fn new(restart_limit: u8) -> Self {
        Self(ms_runtime::lifecycle::SessionLifecycle::new(restart_limit))
    }

    pub fn restore(status: &str, restart_count: u8, restart_limit: u8) -> Result<Self, String> {
        ms_runtime::lifecycle::SessionLifecycle::restore(status, restart_count, restart_limit)
            .map(Self)
    }

    pub fn status(&self) -> SessionStatus {
        self.0.status()
    }

    pub fn restart_count(&self) -> u8 {
        self.0.restart_count()
    }

    pub fn failure_reason(&self) -> Option<ExitReason> {
        self.0.failure_reason()
    }

    pub fn apply(&mut self, event: SessionEvent) -> Result<SessionStatus, String> {
        self.0.apply(event)
    }

    pub fn persist(
        &self,
        store: &mut SupervisorStore,
        session_id: &str,
        container_id: Option<&str>,
        timestamp: &str,
    ) -> Result<(), StoreError> {
        let status = self.status();
        let terminal = matches!(status, SessionStatus::Closed | SessionStatus::Failed);
        let close_reason = match (status, self.failure_reason()) {
            (SessionStatus::Closed, _) => Some("session_end"),
            (SessionStatus::Failed, Some(ExitReason::Safety)) => Some("safety"),
            (SessionStatus::Failed, _) => Some("failed"),
            _ => None,
        };
        store.update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: session_id.to_owned(),
            status: status.as_str().to_owned(),
            container_id: container_id.map(str::to_owned),
            restart_count: self.restart_count(),
            last_active_at: timestamp.to_owned(),
            closed_at: terminal.then(|| timestamp.to_owned()),
            close_reason: close_reason.map(str::to_owned),
        })
    }
}
