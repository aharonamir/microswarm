use ms_proto::ExitReason;
use ms_proto::{InboundKind, SupervisorEventType};
use ms_store::{SessionLifecycleUpdate, StoreError, SupervisorStore};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub async fn close_idle_leads(
    home: &Path,
    runtime_socket: &str,
    idle_seconds: u64,
    grace_ms: u64,
) -> Result<(), String> {
    let store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let due = {
        let mut statement = store.connection().prepare("SELECT id,container_id,last_active_at FROM sessions WHERE kind='lead' AND status='ready' AND container_id IS NOT NULL AND (julianday('now')-julianday(last_active_at))*86400.0>=?1").map_err(|e| e.to_string())?;
        statement
            .query_map([idle_seconds as f64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (id, container, last_active) in due {
        if store.connection().execute("UPDATE sessions SET status='stopping' WHERE id=?1 AND status='ready' AND last_active_at=?2", [&id,&last_active]).map_err(|e|e.to_string())? == 0 { continue; }
        stop_idle_lead(home, runtime_socket, grace_ms, &id, &container).await?;
    }
    Ok(())
}

pub async fn close_idle_members(
    home: &Path,
    runtime_socket: &str,
    idle_seconds: u64,
    grace_ms: u64,
) -> Result<(), String> {
    let store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let due = store.connection().prepare("UPDATE sessions SET status='stopping' WHERE id IN (SELECT id FROM sessions WHERE kind='member' AND status='ready' AND container_id IS NOT NULL AND (julianday('now')-julianday(last_active_at))*86400.0>=?1) RETURNING id,parent_id,container_id").map_err(|e|e.to_string())?.query_map([idle_seconds as f64],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    for (member, lead, container) in due {
        stop_member_idle(home, runtime_socket, grace_ms, &member, &lead, &container).await?;
    }
    Ok(())
}

pub async fn graceful_stop(
    home: &Path,
    runtime_socket: &str,
    grace_ms: u64,
    session: &str,
    container: &str,
) -> Result<(), String> {
    ms_store::open_session_dir(home.join("sessions").join(session))
        .map_err(|e| e.to_string())?
        .append_inbound(
            &InboundKind::Control,
            &json!({"op":"shutdown","grace_ms":grace_ms}),
        )
        .map_err(|e| e.to_string())?;
    let runtime = crate::ContainerRuntime::connect(runtime_socket)?;
    runtime.signal(container, "SIGUSR1").await?;
    let grace = Duration::from_millis(grace_ms);
    let deadline = Instant::now() + grace;
    while runtime.is_running(container).await? && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if runtime.is_running(container).await? {
        runtime.stop(container, grace).await?;
    }
    Ok(())
}

async fn stop_members(home: &Path, socket: &str, grace_ms: u64, lead: &str) -> Result<(), String> {
    let mut store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let members = {
        let mut stmt=store.connection().prepare("SELECT id,container_id,role FROM sessions WHERE parent_id=?1 AND kind='member' AND status NOT IN ('closed','failed') AND container_id IS NOT NULL ORDER BY created_at").map_err(|e|e.to_string())?;
        stmt.query_map([lead], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    };
    for (id, container, role) in members {
        graceful_stop(home, socket, grace_ms, &id, &container).await?;
        let now = crate::utc_now();
        store.connection().execute("UPDATE sessions SET status='closed',close_reason='session_end',closed_at=?2,last_active_at=?2 WHERE id=?1",[&id,&now]).map_err(|e|e.to_string())?;
        store.connection().execute("UPDATE delegations SET status='dismissed',updated_at=?2 WHERE lead_id=?1 AND member_id=?3",[lead,&now,&id]).map_err(|e|e.to_string())?;
        crate::events::emit_supervisor_event(
            &mut store,
            &id,
            lead,
            &role,
            SupervisorEventType::SessionClosed,
            json!({"reason":"session_end"}),
        )?;
    }
    Ok(())
}
async fn stop_member_idle(
    home: &Path,
    socket: &str,
    grace_ms: u64,
    member: &str,
    lead: &str,
    container: &str,
) -> Result<(), String> {
    graceful_stop(home, socket, grace_ms, member, container).await?;
    let now = crate::utc_now();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    store.connection().execute("UPDATE sessions SET status='closed',close_reason='idle',closed_at=?2,last_active_at=?2 WHERE id=?1",[member,&now]).map_err(|e|e.to_string())?;
    store.connection().execute("UPDATE delegations SET status='failed',failure_reason='timeout',updated_at=?2 WHERE lead_id=?1 AND member_id=?3",[lead,&now,member]).map_err(|e|e.to_string())?;
    crate::events::emit_supervisor_event(
        &mut store,
        member,
        lead,
        "member",
        SupervisorEventType::DelegationTimeout,
        json!({"reason":"idle"}),
    )?;
    ms_store::open_session_dir(home.join("sessions").join(lead))
        .map_err(|e| e.to_string())?
        .append_inbound(
            &InboundKind::MemberFailed,
            &json!({"member_id":member,"reason":"timeout"}),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
async fn stop_idle_lead(
    home: &Path,
    socket: &str,
    grace_ms: u64,
    lead: &str,
    container: &str,
) -> Result<(), String> {
    stop_members(home, socket, grace_ms, lead).await?;
    graceful_stop(home, socket, grace_ms, lead, container).await?;
    let now = crate::utc_now();
    SupervisorStore::open(home.join("supervisor.db")).map_err(|e|e.to_string())?.connection().execute("UPDATE sessions SET status='closed',close_reason='idle',closed_at=?2,last_active_at=?2 WHERE id=?1 AND status='stopping'",[lead,&now]).map_err(|e|e.to_string())?;
    Ok(())
}

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

    pub fn persist(
        &self,
        store: &mut SupervisorStore,
        session_id: &str,
        container_id: Option<&str>,
        timestamp: &str,
    ) -> Result<(), StoreError> {
        let terminal = matches!(self.status, SessionStatus::Closed | SessionStatus::Failed);
        let close_reason = match (self.status, self.failure_reason) {
            (SessionStatus::Closed, _) => Some("session_end"),
            (SessionStatus::Failed, Some(ExitReason::Safety)) => Some("safety"),
            (SessionStatus::Failed, _) => Some("failed"),
            _ => None,
        };
        store.update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: session_id.to_owned(),
            status: self.status.as_str().to_owned(),
            container_id: container_id.map(str::to_owned),
            restart_count: self.restart_count,
            last_active_at: timestamp.to_owned(),
            closed_at: terminal.then(|| timestamp.to_owned()),
            close_reason: close_reason.map(str::to_owned),
        })
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
