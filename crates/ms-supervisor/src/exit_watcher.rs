//! Container exit handling and transient restart policy.
use crate::{
    config::load_config_dir,
    runtime::ContainerRuntime,
    service::{config_dir, emit_supervisor_event, utc_now},
    sessions::{SessionEvent, SessionLifecycle, SessionStatus},
};
use ms_proto::{ExitReason, InboundKind, SupervisorEventType};
use ms_store::{SupervisorStore, open_session_dir};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
static EXIT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn reconcile_loop(home: PathBuf) {
    loop {
        if let Err(error) = reconcile(&home).await {
            eprintln!("container reconciliation: {error}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn reconcile(home: &Path) -> Result<(), String> {
    let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
    let runtime = ContainerRuntime::connect(&config.microswarm.runtime.socket)?;
    for event in runtime
        .stopped_sessions(&home.join("supervisor.db"))
        .await?
    {
        handle_exit(
            home,
            &runtime,
            &event.session_id,
            &event.container_id,
            event.exit_code,
        )
        .await?;
    }
    Ok(())
}

pub async fn watch(home: PathBuf) {
    loop {
        let runtime = match load_config_dir(config_dir(&home))
            .map_err(|error| error.to_string())
            .and_then(|config| ContainerRuntime::connect(&config.microswarm.runtime.socket))
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("container exit watcher: {error}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let root = home.clone();
        let watched_runtime = runtime.clone();
        let result = runtime
            .watch_exits(move |event| {
                let home = root.clone();
                let runtime = watched_runtime.clone();
                async move {
                    handle_exit(
                        &home,
                        &runtime,
                        &event.session_id,
                        &event.container_id,
                        event.exit_code,
                    )
                    .await
                }
            })
            .await;
        if let Err(error) = result {
            eprintln!("container exit watcher: {error}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn handle_exit(
    home: &Path,
    runtime: &ContainerRuntime,
    id: &str,
    container: &str,
    code: Option<i32>,
) -> Result<(), String> {
    let _guard = EXIT_LOCK.lock().await;
    handle_exit_locked(home, runtime, id, container, code).await
}

async fn handle_exit_locked(
    home: &Path,
    runtime: &ContainerRuntime,
    id: &str,
    container: &str,
    code: Option<i32>,
) -> Result<(), String> {
    let config = load_config_dir(config_dir(home)).map_err(|e| e.to_string())?;
    let mut store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let row: Option<(String, String, String, String, Option<String>, u8)> = store
        .connection()
        .query_row(
            "SELECT root_id,kind,role,status,container_id,restart_count FROM sessions WHERE id=?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((root, kind, role, status, stored_container, restarts)) = row else {
        return Ok(());
    };
    if stored_container.as_deref() != Some(container)
        || matches!(status.as_str(), "closed" | "failed" | "stopping")
        || runtime.is_running(container).await?
    {
        return Ok(());
    }
    let dir = home.join("sessions").join(id);
    let session = open_session_dir(&dir).map_err(|e| e.to_string())?;
    let agent_reason: Option<String> = session
        .events()
        .query_row(
            "SELECT data FROM events WHERE type='agent.exit' ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .and_then(|v| serde_json::from_str::<Value>(&v).ok())
        .and_then(|v| v.get("reason").and_then(Value::as_str).map(str::to_owned));
    let oom = runtime.oom_killed(container).await.unwrap_or(false);
    let reason = crate::sessions::classify_exit_with_agent_reason(
        code.unwrap_or(-1),
        oom,
        agent_reason.as_deref(),
    );
    let mut lifecycle =
        SessionLifecycle::restore(&status, restarts, config.microswarm.limits.restart_max)?;
    lifecycle.apply(SessionEvent::ContainerExit { reason })?;
    persist(&mut store, id, container, &lifecycle)?;
    emit_supervisor_event(
        &mut store,
        id,
        &root,
        &role,
        SupervisorEventType::ContainerExited,
        json!({"reason":reason,"exit_code":code,"oom_killed":oom,"status":lifecycle.status().as_str()}),
    )?;
    if lifecycle.status() == SessionStatus::Crashed {
        let count = lifecycle.restart_count();
        let wait = Duration::from_secs(1u64 << u32::from(count.saturating_sub(1).min(2)));
        tokio::time::sleep(wait).await;
        let still_current: bool = store.connection().query_row("SELECT status='crashed' AND container_id=?2 AND restart_count=?3 FROM sessions WHERE id=?1",rusqlite::params![id,container,i64::from(count)],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !still_current {
            return Ok(());
        }
        lifecycle.apply(SessionEvent::Restart)?;
        persist(&mut store, id, container, &lifecycle)?;
        let old_seq: i64 = session
            .events()
            .query_row("SELECT COALESCE(MAX(seq),0) FROM events", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if let Err(error) = runtime.start(container).await {
            lifecycle.apply(SessionEvent::ContainerExit {
                reason: ExitReason::Config,
            })?;
            persist(&mut store, id, container, &lifecycle)?;
            emit_supervisor_event(
                &mut store,
                id,
                &root,
                &role,
                SupervisorEventType::ContainerExited,
                json!({"reason":"config","detail":error,"status":"failed"}),
            )?;
            if kind == "member" {
                notify_member_failure(home, &mut store, id, &root, ExitReason::Config)?;
            }
            return Ok(());
        }
        emit_supervisor_event(
            &mut store,
            id,
            &root,
            &role,
            SupervisorEventType::ContainerRestart,
            json!({"restart_count":count,"backoff_s":wait.as_secs()}),
        )?;
        if wait_ready(&dir, old_seq, Duration::from_secs(30)).await? {
            let mut current =
                SessionLifecycle::restore("starting", count, config.microswarm.limits.restart_max)?;
            current.apply(SessionEvent::AgentReady)?;
            persist(&mut store, id, container, &current)?;
        }
    } else if lifecycle.status() == SessionStatus::Failed && kind == "member" {
        notify_member_failure(home, &mut store, id, &root, reason)?;
    }
    Ok(())
}

fn persist(
    store: &mut SupervisorStore,
    id: &str,
    container: &str,
    lifecycle: &SessionLifecycle,
) -> Result<(), String> {
    lifecycle
        .persist(store, id, Some(container), &utc_now())
        .map_err(|e| e.to_string())
}
fn notify_member_failure(
    home: &Path,
    store: &mut SupervisorStore,
    id: &str,
    lead: &str,
    reason: ExitReason,
) -> Result<(), String> {
    let session = open_session_dir(home.join("sessions").join(lead)).map_err(|e| e.to_string())?;
    let kind = if reason == ExitReason::Safety {
        "safety"
    } else if reason == ExitReason::Config {
        "config"
    } else {
        "crashed"
    };
    session
        .append_inbound(
            &InboundKind::MemberFailed,
            &json!({"member_id":id,"reason":kind}),
        )
        .map_err(|e| e.to_string())?;
    let delegation:Option<String>=store.connection().query_row("SELECT id FROM delegations WHERE member_id=?1 AND status NOT IN ('failed','completed','dismissed') ORDER BY created_at DESC LIMIT 1",[id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    if let Some(d) = delegation {
        let failure_reason = match reason {
            ExitReason::Safety => "safety",
            ExitReason::Config => "config",
            _ => "crashed",
        };
        store.connection().execute(
            "UPDATE delegations SET status='failed',failure_reason=?2,updated_at=?3 WHERE id=?1",
            rusqlite::params![d,failure_reason,utc_now()],
        ).map_err(|e|e.to_string())?;
        crate::service::emit_supervisor_event(
            store,
            id,
            lead,
            "member",
            SupervisorEventType::DelegationFailed,
            json!({"delegation_id":d,"member_id":id,"reason":failure_reason}),
        )?;
    }
    Ok(())
}
async fn wait_ready(dir: &Path, after: i64, timeout: Duration) -> Result<bool, String> {
    let until = tokio::time::Instant::now() + timeout;
    loop {
        let events = open_session_dir(dir).map_err(|e| e.to_string())?;
        let seq:Option<i64>=events.events().query_row("SELECT seq FROM events WHERE type='agent.ready' AND seq>?1 ORDER BY seq DESC LIMIT 1",[after],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        if seq.is_some() {
            return Ok(true);
        }
        if tokio::time::Instant::now() >= until {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
