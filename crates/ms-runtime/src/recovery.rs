//! Boot-time reconciliation of managed containers and persisted supervisor state.
use crate::{
    ContainerProfile, ContainerRuntime, ManagedContainer, MountSpec,
    lifecycle::{SessionEvent, SessionLifecycle, SessionStatus},
};
use ms_proto::{ExitReason, InboundKind, Scope, SupervisorEvent, SupervisorEventType};
use ms_store::{ApprovalRecord, SessionLifecycleUpdate, SupervisorStore, open_session_dir};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub id: String,
    pub root: String,
    pub role: String,
    pub kind: String,
    pub status: String,
    pub container_id: Option<String>,
    pub restart_count: u8,
}

pub struct BootScan {
    pub adopted: Vec<(SessionInfo, ManagedContainer)>,
    pub missing: Vec<SessionInfo>,
}

pub struct RecoveryConfig {
    pub image: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub model_credential: Option<String>,
    pub mcp_servers: String,
    pub restart_max: u8,
}

pub async fn scan(home: &Path, runtime: &ContainerRuntime) -> Result<BootScan, String> {
    let mut store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let all_sessions = {
        let mut stmt = store
            .connection()
            .prepare("SELECT id,root_id,role,kind,status,container_id,restart_count FROM sessions")
            .map_err(|e| e.to_string())?;
        stmt.query_map([], |row| {
            Ok(SessionInfo {
                id: row.get(0)?,
                root: row.get(1)?,
                role: row.get(2)?,
                kind: row.get(3)?,
                status: row.get(4)?,
                container_id: row.get(5)?,
                restart_count: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    };
    let known_ids: HashSet<_> = all_sessions
        .iter()
        .map(|session| session.id.clone())
        .collect();
    let sessions: Vec<_> = all_sessions
        .into_iter()
        .filter(|session| {
            matches!(
                session.status.as_str(),
                "starting" | "ready" | "busy" | "crashed" | "stopping"
            )
        })
        .collect();
    let by_id: HashMap<_, _> = sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut adopted = Vec::new();
    let mut matched_ids = HashSet::new();
    for container in runtime.managed_containers().await? {
        let matched = container
            .session_id
            .as_deref()
            .and_then(|id| by_id.get(id))
            .filter(|s| s.container_id.as_deref() == Some(container.id.as_str()))
            .filter(|s| s.status != "stopping" && home.join("sessions").join(&s.id).is_dir());
        if let Some(session) = matched {
            if container.running {
                matched_ids.insert(session.id.clone());
                adopted.push(((*session).clone(), container));
            }
            continue;
        }
        // A runtime daemon may serve several MicroSwarm homes. Reconcile only
        // containers named by this home's authoritative session database.
        let Some(id) = container
            .session_id
            .as_deref()
            .filter(|id| known_ids.contains(*id))
        else {
            continue;
        };
        if container.running {
            runtime.stop(&container.id, Duration::from_secs(2)).await?;
        }
        record_event(
            &mut store,
            id,
            id,
            "supervisor",
            SupervisorEventType::ContainerOrphanStopped,
            json!({"container_id":container.id,"reason":"unmatched_session"}),
        )?;
    }
    let missing = sessions
        .into_iter()
        .filter(|s| s.status != "stopping" && !matched_ids.contains(&s.id))
        .collect();
    Ok(BootScan { adopted, missing })
}

pub fn record_event(
    store: &mut SupervisorStore,
    id: &str,
    root: &str,
    role: &str,
    typ: SupervisorEventType,
    data: Value,
) -> Result<(), String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ts: String = store
        .connection()
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    store
        .append_supervisor_event(&SupervisorEvent {
            seq: 0,
            ts,
            ts_mono: elapsed.as_nanos().try_into().unwrap_or(u64::MAX),
            session_id: id.into(),
            root_session_id: root.into(),
            agent_role: role.into(),
            source: Default::default(),
            event_type: typ,
            data,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn reissue_pending(store: &mut SupervisorStore, timeout: Duration) -> Result<usize, String> {
    let pending = {
        let mut stmt=store.connection().prepare("SELECT id,code,root_session_id,requesting_session_id,kind,payload,status,expires_at,created_at FROM approvals WHERE status='pending' ORDER BY created_at").map_err(|e|e.to_string())?;
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    };
    for (id, old_code, root, requester, kind, payload, _, _, _) in &pending {
        let mut random = [0u8; 16];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut random))
            .map_err(|e| e.to_string())?;
        let new_id = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let alphabet = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
        let mut code = random[..4]
            .iter()
            .map(|b| alphabet[usize::from(b & 31)] as char)
            .collect::<String>();
        if code == *old_code {
            code.push('2');
        }
        let now: String = store
            .connection()
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        let replacement = ApprovalRecord {
            id: new_id.clone(),
            code: code.clone(),
            root_session_id: root.clone(),
            requesting_session_id: requester.clone(),
            kind: kind.clone(),
            payload: serde_json::from_str(payload).map_err(|e| e.to_string())?,
            status: "pending".into(),
            expires_at: utc_after(timeout),
            created_at: now.clone(),
        };
        let transaction = store
            .connection()
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        transaction.execute("INSERT INTO approvals(id,code,root_session_id,requesting_session_id,kind,payload,status,expires_at,created_at) VALUES (?1,?2,?3,?4,?5,?6,'pending',?7,?8)",params![replacement.id,replacement.code,root,requester,kind,payload,replacement.expires_at,now]).map_err(|e|e.to_string())?;
        transaction.execute("UPDATE approvals SET status='expired',resolved_at=?2,reissued_as=?3 WHERE id=?1 AND status='pending'",params![id,now,new_id]).map_err(|e|e.to_string())?;
        transaction
            .execute(
                "UPDATE delegations SET approval_id=?2 WHERE approval_id=?1",
                params![id, new_id],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        record_event(
            store,
            requester,
            root,
            "supervisor",
            SupervisorEventType::ApprovalExpired,
            json!({"approval_id":id,"code":old_code}),
        )?;
        record_event(
            store,
            requester,
            root,
            "supervisor",
            SupervisorEventType::ApprovalReissued,
            json!({"approval_id":new_id,"replaces":id,"code":code}),
        )?;
    }
    Ok(pending.len())
}

fn utc_after(duration: Duration) -> String {
    let target = SystemTime::now() + duration;
    let since = target.duration_since(UNIX_EPOCH).unwrap_or_default();
    let sec = since.as_secs();
    // SQLite's date conversion keeps the same UTC format used by supervisor.db.
    let db = rusqlite::Connection::open_in_memory().expect("in-memory SQLite");
    db.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1,'unixepoch')",
        [sec as i64],
        |row| row.get(0),
    )
    .expect("UTC timestamp")
}

pub async fn recreate_missing(
    home: &Path,
    runtime: &ContainerRuntime,
    session: &SessionInfo,
    config: &RecoveryConfig,
    proxy_port: u16,
) -> Result<(), String> {
    let mut store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let mut lifecycle =
        SessionLifecycle::restore(&session.status, session.restart_count, config.restart_max)?;
    lifecycle.apply(SessionEvent::ContainerExit {
        reason: ExitReason::Transient,
    })?;
    let count = lifecycle.restart_count();
    let status = lifecycle.status();
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: session.id.clone(),
            status: status.as_str().into(),
            container_id: None,
            restart_count: count,
            last_active_at: timestamp(&store)?,
            closed_at: (status == SessionStatus::Failed)
                .then(|| timestamp(&store).unwrap_or_default()),
            close_reason: (status == SessionStatus::Failed).then(|| "failed".into()),
        })
        .map_err(|e| e.to_string())?;
    record_event(
        &mut store,
        &session.id,
        &session.root,
        &session.role,
        SupervisorEventType::ContainerExited,
        json!({"reason":"transient","container_id":session.container_id,"status":status.as_str()}),
    )?;
    if status == SessionStatus::Failed {
        if session.kind == "member" {
            notify_member(home, &session.root, &session.id, "crashed")?;
        }
        return Ok(());
    }
    tokio::time::sleep(Duration::from_secs(
        1u64 << u32::from(count.saturating_sub(1).min(2)),
    ))
    .await;
    let (hash, value) = store
        .session_scope(&session.id)
        .map_err(|e| e.to_string())?
        .ok_or("session has no stored scope")?;
    let scope: Scope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if scope.session_id != session.id || scope.hash != hash || scope.canonical_hash() != hash {
        return Err("stored scope hash mismatch".into());
    }
    let dir = home.join("sessions").join(&session.id);
    let network = format!("ms-net-{}", session.id);
    let gateway = match runtime.network_gateway(&network).await {
        Ok(value) => value,
        Err(_) => {
            let id = runtime.create_network(&network).await?;
            runtime.network_gateway(&id).await?
        }
    };
    let user = runtime.container_user(&dir)?;
    let mut profile =
        ContainerProfile::for_session(&scope, &config.image, &dir, &network, &gateway, &user)?;
    let mut env = vec![
        format!("MS_SESSION_ID={}", session.id),
        format!("MS_ROOT_SESSION_ID={}", session.root),
        format!("MS_KIND={}", session.kind),
        format!("MS_ROLE={}", session.role),
        "MS_SESSION_DIR=/session".into(),
        format!("MS_PROXY_URL=http://host.docker.internal:{proxy_port}"),
        format!(
            "MS_WORK_DIR={}",
            scope
                .mounts
                .first()
                .map(|m| m.container_path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/work".into())
        ),
    ];
    if let (Some(provider), Some(name)) = (&config.model_provider, &config.model_name) {
        env.extend([
            format!("MS_MODEL_PROVIDER={provider}"),
            format!("MS_MODEL_NAME={name}"),
        ]);
    }
    if let Some(credential) = &config.model_credential
        && (session.kind == "lead" || scope.credentials.contains(credential))
    {
        env.push(format!("MS_MODEL_CREDENTIAL={credential}"));
    }
    env.push(format!("MS_MCP_SERVERS={}", config.mcp_servers));
    if session.kind == "member" {
        let role: Option<(String, String)> = store
            .connection()
            .query_row(
                "SELECT skill,instructions FROM team_roles WHERE role=?1 LIMIT 1",
                [&session.role],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((skill, instructions)) = role {
            let path = std::fs::canonicalize(instructions).map_err(|e| e.to_string())?;
            profile.mounts.push(MountSpec {
                source: path.display().to_string(),
                target: format!("/skills/{skill}/role.md"),
                read_only: true,
            });
            env.push(format!("MS_ROLE_INSTRUCTIONS=/skills/{skill}/role.md"));
        }
    }
    let container = runtime.create_container(&profile, &env).await?;
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: session.id.clone(),
            status: "starting".into(),
            container_id: Some(container.clone()),
            restart_count: count,
            last_active_at: timestamp(&store)?,
            closed_at: None,
            close_reason: None,
        })
        .map_err(|e| e.to_string())?;
    runtime.start(&container).await?;
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: session.id.clone(),
            status: "ready".into(),
            container_id: Some(container.clone()),
            restart_count: count,
            last_active_at: timestamp(&store)?,
            closed_at: None,
            close_reason: None,
        })
        .map_err(|e| e.to_string())?;
    record_event(
        &mut store,
        &session.id,
        &session.root,
        &session.role,
        SupervisorEventType::ContainerRestart,
        json!({"container_id":container,"restart_count":count,"recovered":true}),
    )?;
    Ok(())
}

fn timestamp(store: &SupervisorStore) -> Result<String, String> {
    store
        .connection()
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())
}

fn notify_member(home: &Path, lead: &str, member: &str, reason: &str) -> Result<(), String> {
    open_session_dir(home.join("sessions").join(lead))
        .map_err(|e| e.to_string())?
        .append_inbound(
            &InboundKind::MemberFailed,
            &json!({"member_id":member,"reason":reason}),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
