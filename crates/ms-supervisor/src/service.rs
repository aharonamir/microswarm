use crate::{
    config::{ConfigSet, SpawnPolicy, ToolTransport, load_config_dir},
    proxy::{CredentialProvider, ProxyRoutes, SessionProxy},
    runtime::{ContainerProfile, ContainerRuntime},
    scope::{build_lead_scope, install_lead_scope},
};
use ms_proto::{
    DerivedFrom, InboundKind, OutboundKind, Scope, ScopeCeiling, SupervisorEvent,
    SupervisorEventType,
};
use ms_store::{
    DelegationRecord, SessionConnections, SessionLifecycleUpdate, SessionRecord, SupervisorStore,
    open_session_dir,
};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UnixListener, UnixStream},
};

pub async fn serve(home: impl AsRef<Path>) -> Result<(), String> {
    let home = home.as_ref().to_path_buf();
    fs::create_dir_all(&home).map_err(|error| error.to_string())?;
    let socket_path = home.join("supervisor.sock");
    if socket_path.exists() {
        if UnixStream::connect(&socket_path).await.is_ok() {
            return Err("another microswarmd is already listening".into());
        }
        fs::remove_file(&socket_path).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    let idle_home = home.clone();
    tokio::spawn(async move { idle_session_watcher(idle_home).await });
    tokio::spawn(crate::exit_watcher::watch(home.clone()));
    tokio::spawn(crate::exit_watcher::reconcile_loop(home.clone()));
    loop {
        let (stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let home = home.clone();
        tokio::spawn(async move {
            let _ = handle_connection(home, stream).await;
        });
    }
}

async fn handle_connection(home: PathBuf, stream: UnixStream) -> Result<(), String> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .await
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(());
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => dispatch(&home, &request).await,
            Err(error) => json!({"error":format!("invalid request: {error}")}),
        };
        let mut bytes = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        write
            .write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
    }
}

async fn idle_session_watcher(home: PathBuf) {
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Err(error) = close_idle_leads(&home).await {
            eprintln!("idle session watcher failed: {error}");
        }
        if let Err(error) = close_idle_members(&home).await {
            eprintln!("member idle watcher failed: {error}");
        }
        if let Err(error) = crate::deadlines::process_due(&home).await {
            eprintln!("delegation deadline watcher failed: {error}");
        }
    }
}

async fn close_idle_leads(home: &Path) -> Result<(), String> {
    let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
    let idle_seconds = config.microswarm.limits.lead_idle_s;
    let store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let due = {
        let mut statement = store
            .connection()
            .prepare(
                "SELECT id, container_id, last_active_at FROM sessions
                 WHERE kind='lead' AND status='ready' AND container_id IS NOT NULL
                   AND (julianday('now') - julianday(last_active_at)) * 86400.0 >= ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([idle_seconds as f64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for (session_id, container_id, last_active_at) in due {
        let claimed = store
            .connection()
            .execute(
                "UPDATE sessions SET status='stopping'
                 WHERE id=?1 AND status='ready' AND last_active_at=?2",
                [&session_id, &last_active_at],
            )
            .map_err(|error| error.to_string())?;
        if claimed == 0 {
            continue;
        }
        stop_idle_lead(home, &config, &session_id, &container_id).await?;
    }
    Ok(())
}

async fn close_idle_members(home: &Path) -> Result<(), String> {
    let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
    let store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let due = store.connection().prepare(
        "UPDATE sessions SET status='stopping' WHERE id IN (
           SELECT id FROM sessions WHERE kind='member' AND status='ready' AND container_id IS NOT NULL
           AND (julianday('now') - julianday(last_active_at)) * 86400.0 >= ?1
         ) RETURNING id,parent_id,container_id",
    ).map_err(|e|e.to_string())?.query_map([config.microswarm.limits.member_idle_s as f64],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    for (member_id, lead_id, container_id) in due {
        stop_member_idle(home, &config, &member_id, &lead_id, &container_id).await?;
    }
    Ok(())
}

async fn stop_member_idle(
    home: &Path,
    config: &ConfigSet,
    member_id: &str,
    lead_id: &str,
    container_id: &str,
) -> Result<(), String> {
    graceful_stop_session(home, config, member_id, container_id).await?;
    let now = utc_now();
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    store.connection().execute("UPDATE sessions SET status='closed',close_reason='idle',closed_at=?2,last_active_at=?2 WHERE id=?1",[member_id,now.as_str()]).map_err(|error|error.to_string())?;
    store.connection().execute("UPDATE delegations SET status='failed',failure_reason='timeout',updated_at=?2 WHERE lead_id=?1 AND member_id=?3",[lead_id,now.as_str(),member_id]).map_err(|error|error.to_string())?;
    emit_supervisor_event(
        &mut store,
        member_id,
        lead_id,
        "member",
        SupervisorEventType::DelegationTimeout,
        json!({"reason":"idle"}),
    )?;
    open_session_dir(home.join("sessions").join(lead_id))
        .map_err(|error| error.to_string())?
        .append_inbound(
            &InboundKind::MemberFailed,
            &json!({"member_id":member_id,"reason":"timeout"}),
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn stop_idle_lead(
    home: &Path,
    config: &ConfigSet,
    session_id: &str,
    container_id: &str,
) -> Result<(), String> {
    stop_members_for_lead(home, config, session_id).await?;
    graceful_stop_session(home, config, session_id, container_id).await?;

    let closed_at = utc_now();
    let store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    store
        .connection()
        .execute(
            "UPDATE sessions SET status='closed', close_reason='idle', closed_at=?2, last_active_at=?2
             WHERE id=?1 AND status='stopping'",
            [session_id, closed_at.as_str()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn stop_members_for_lead(
    home: &Path,
    config: &ConfigSet,
    lead_id: &str,
) -> Result<(), String> {
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let members = {
        let mut statement = store.connection().prepare(
            "SELECT id, container_id, role FROM sessions WHERE parent_id=?1 AND kind='member'
             AND status NOT IN ('closed','failed') AND container_id IS NOT NULL ORDER BY created_at",
        ).map_err(|error| error.to_string())?;
        statement
            .query_map([lead_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for (member_id, container_id, role) in members {
        graceful_stop_session(home, config, &member_id, &container_id).await?;
        let now = utc_now();
        store.connection().execute(
            "UPDATE sessions SET status='closed', close_reason='session_end', closed_at=?2, last_active_at=?2 WHERE id=?1",
            [member_id.as_str(),now.as_str()],
        ).map_err(|error| error.to_string())?;
        store.connection().execute(
            "UPDATE delegations SET status='dismissed', updated_at=?2 WHERE lead_id=?1 AND member_id=?3",
            [lead_id,now.as_str(),member_id.as_str()],
        ).map_err(|error| error.to_string())?;
        emit_supervisor_event(
            &mut store,
            &member_id,
            lead_id,
            &role,
            SupervisorEventType::SessionClosed,
            json!({"reason":"session_end"}),
        )?;
    }
    Ok(())
}

async fn graceful_stop_session(
    home: &Path,
    config: &ConfigSet,
    session_id: &str,
    container_id: &str,
) -> Result<(), String> {
    open_session_dir(home.join("sessions").join(session_id))
        .map_err(|error| error.to_string())?
        .append_inbound(
            &InboundKind::Control,
            &json!({"op":"shutdown","grace_ms":config.microswarm.limits.grace_ms}),
        )
        .map_err(|error| error.to_string())?;
    let runtime = runtime_for(config)?;
    runtime.signal(container_id, "SIGUSR1").await?;
    let grace = Duration::from_millis(config.microswarm.limits.grace_ms);
    let deadline = Instant::now() + grace;
    while runtime.is_running(container_id).await? && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if runtime.is_running(container_id).await? {
        runtime.stop(container_id, grace).await?;
    }
    Ok(())
}

async fn dispatch(home: &Path, request: &Value) -> Value {
    let result = match request.get("op").and_then(Value::as_str) {
        Some("skills_install") => install_skill(home, request),
        Some("creds_set") => store_credential(home, request),
        Some("creds_list") => list_credentials(home),
        Some("approve") | Some("deny") => resolve_approval(home, request).await,
        Some("chat") => chat_message(home, request).await,
        Some("wait") => wait_for_reply(home, request).await,
        _ => Err("unknown supervisor request".to_owned()),
    };
    match result {
        Ok(value) => value,
        Err(error) => json!({"error":error}),
    }
}

fn install_skill(home: &Path, request: &Value) -> Result<Value, String> {
    let source = request
        .get("path")
        .and_then(Value::as_str)
        .ok_or("skill directory path is missing")?;
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let installed =
        crate::skills::install_team_skill(&mut store, &home.join("skills"), Path::new(source))?;
    Ok(json!({
        "name":installed.name,
        "version":installed.version,
        "roles":installed.roles,
    }))
}

async fn chat_message(home: &Path, request: &Value) -> Result<Value, String> {
    let text = request
        .get("text")
        .and_then(Value::as_str)
        .ok_or("chat text is missing")?;
    if let Some(code) = text
        .strip_prefix("/approve ")
        .or_else(|| text.strip_prefix("/deny "))
    {
        let approve = text.starts_with("/approve ");
        let resolved = resolve_approval(
            home,
            &json!({"op":if approve {"approve"} else {"deny"},"code":code.trim()}),
        )
        .await?;
        return Ok(
            json!({"text":format!("Approval {}.", resolved["decision"].as_str().unwrap_or("resolved"))}),
        );
    }
    let config_dir = if home.join("config/microswarm.toml").is_file() {
        home.join("config")
    } else {
        home.to_path_buf()
    };
    let config = load_config_dir(&config_dir).map_err(|error| error.to_string())?;
    config
        .microswarm
        .model
        .as_ref()
        .ok_or("[model] must be configured before chat")?;
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let session_id = match request.get("session_id").and_then(Value::as_str) {
        Some(id) => id.to_owned(),
        None => match store
            .active_lead_for_thread("cli", "default")
            .map_err(|error| error.to_string())?
        {
            Some(id) => id,
            None => create_lead(home, &config, &mut store).await?,
        },
    };
    let container_id = store
        .session_container_id(&session_id)
        .map_err(|error| error.to_string())?
        .ok_or("session has no running container")?;
    let session_dir = home.join("sessions").join(&session_id);
    let session = open_session_dir(&session_dir).map_err(|error| error.to_string())?;
    let active_at = utc_now();
    claim_session_busy(&store, &session_id, &active_at)?;
    let mut last_id: i64 = session
        .outbound()
        .query_row("SELECT COALESCE(MAX(id),0) FROM outbound", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    let inbound_id = session
        .append_inbound(&InboundKind::UserMessage, &json!({"text":text}))
        .map_err(|error| error.to_string())?;
    let runtime = runtime_for(&config)?;
    runtime.signal(&container_id, "SIGUSR1").await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let (status, close_reason, restart_count): (String, Option<String>, u8) = store
            .connection()
            .query_row(
                "SELECT status,close_reason,restart_count FROM sessions WHERE id=?1",
                [&session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| error.to_string())?;
        if status == "failed" {
            let exit_data: Option<String> = store.connection().query_row(
                "SELECT data FROM supervisor_events WHERE session_id=?1 AND type='container.exited' ORDER BY seq DESC LIMIT 1",[&session_id],|row|row.get(0)
            ).optional().map_err(|error|error.to_string())?;
            let exit_reason = exit_data
                .and_then(|data| serde_json::from_str::<Value>(&data).ok())
                .and_then(|data| {
                    data.get("reason")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            let key = match ms_runtime::lifecycle::terminal_notice(
                close_reason.as_deref(),
                exit_reason.as_deref(),
                restart_count,
            ) {
                "config" => crate::i18n::MessageKey::ConfigInvalid,
                "retry_exhausted" => crate::i18n::MessageKey::RetryExhausted,
                _ => crate::i18n::MessageKey::SessionFailed,
            };
            let notice = crate::i18n::translate(config.microswarm.home.lang, key);
            return Ok(
                json!({"session_id":session_id,"text":format!("{notice} (session {session_id})")}),
            );
        }
        for message in session
            .outbound_after(last_id)
            .map_err(|error| error.to_string())?
        {
            if let Some(id) = message.id {
                last_id = id;
            }
            if message.in_reply_to == Some(inbound_id) && message.kind == OutboundKind::Reply {
                let active_at = utc_now();
                store
                    .connection()
                    .execute(
                        "UPDATE sessions SET status='ready', last_active_at=?2 WHERE id=?1",
                        [session_id.as_str(), active_at.as_str()],
                    )
                    .map_err(|error| error.to_string())?;
                return Ok(
                    json!({"session_id":session_id,"text":message.body.get("text").and_then(Value::as_str).unwrap_or("")}),
                );
            }
            if message.in_reply_to == Some(inbound_id)
                && message.kind == OutboundKind::ApprovalRequest
            {
                let binding: ms_proto::ApprovalBinding = serde_json::from_value(
                    message
                        .body
                        .get("binding")
                        .cloned()
                        .ok_or("approval request has no binding")?,
                )
                .map_err(|error| error.to_string())?;
                let arguments = message.body.get("args").cloned().unwrap_or(Value::Null);
                let approval = crate::approvals::request_tool_approval(
                    &mut store,
                    &session_id,
                    &session_id,
                    &binding,
                    &arguments,
                    Duration::from_secs(config.microswarm.limits.approval_timeout_s),
                )
                .map_err(|error| error.to_string())?;
                let prompt = crate::approvals::render_prompt(
                    config.microswarm.home.lang,
                    &approval.code,
                    &binding.server,
                    &binding.tool,
                    &arguments,
                );
                return Ok(
                    json!({"session_id":session_id,"pending":true,"prompt":prompt,"after_id":message.id.unwrap_or(last_id)}),
                );
            }
            if message.kind == OutboundKind::DelegationRequest
                && let Some(prompt) = handle_delegation_request(
                    home,
                    &config,
                    &mut store,
                    &session_dir,
                    &session_id,
                    &message.body,
                )
                .await?
            {
                return Ok(
                    json!({"session_id":session_id,"pending":true,"prompt":prompt,"after_id":message.id.unwrap_or(last_id)}),
                );
            }
            if message.kind == OutboundKind::SendToMember
                && let Some(prompt) = handle_send_to_member(
                    home,
                    &config,
                    &mut store,
                    &session_dir,
                    &session_id,
                    &message.body,
                )
                .await?
            {
                return Ok(
                    json!({"session_id":session_id,"pending":true,"prompt":prompt,"after_id":message.id.unwrap_or(last_id)}),
                );
            }
            if message.kind == OutboundKind::Dismiss {
                handle_dismiss_member(
                    home,
                    &config,
                    &mut store,
                    &session_dir,
                    &session_id,
                    &message.body,
                )
                .await?;
            }
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for agent reply".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn claim_session_busy(
    store: &SupervisorStore,
    session_id: &str,
    active_at: &str,
) -> Result<(), String> {
    let claimed = store
        .connection()
        .execute(
            "UPDATE sessions SET status='busy', last_active_at=?2 WHERE id=?1 AND status IN ('ready','busy')",
            [session_id, active_at],
        )
        .map_err(|error| error.to_string())?;
    if claimed == 0 {
        return Err("session is no longer ready to accept a message".into());
    }
    Ok(())
}

async fn create_lead(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
) -> Result<String, String> {
    let id = new_id()?;
    let session_dir = home.join("sessions").join(&id);
    fs::create_dir_all(&session_dir).map_err(|error| error.to_string())?;
    let mut session = open_session_dir(&session_dir).map_err(|error| error.to_string())?;
    let scope = build_lead_scope(config, &id)?;
    let record = SessionRecord {
        id: id.clone(),
        root_id: id.clone(),
        parent_id: None,
        role: "lead".into(),
        kind: "lead".into(),
        channel: Some("cli".into()),
        channel_thread: Some("default".into()),
        status: "pending".into(),
        scope_hash: None,
        scope: None,
        created_at: utc_now(),
        last_active_at: utc_now(),
    };
    install_lead_scope(store, record, &session_dir, &scope)?;
    let runtime = runtime_for(config)?;
    let network = format!("ms-net-{id}");
    let network_id = runtime.create_network(&network).await?;
    let host_gateway = runtime.network_gateway(&network_id).await?;
    let container_user = runtime.container_user(&session_dir)?;
    let listener = TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|error| error.to_string())?;
    let proxy_addr = listener.local_addr().map_err(|error| error.to_string())?;
    let proxy = SessionProxy::from_store(store, &id).map_err(|error| error.to_string())?;
    let routes = proxy_routes(config);
    let credentials = Arc::new(HomeCredentials {
        home: home.to_path_buf(),
    });
    tokio::spawn(async move {
        if let Err(error) = proxy.serve(listener, routes, credentials).await {
            eprintln!("session proxy listener stopped: {error}");
        }
    });
    let proxy_url = format!("http://host.docker.internal:{}", proxy_addr.port());
    let mut env = vec![
        format!("MS_SESSION_ID={id}"),
        format!("MS_ROOT_SESSION_ID={id}"),
        "MS_KIND=lead".into(),
        "MS_ROLE=lead".into(),
        "MS_SESSION_DIR=/session".into(),
        format!("MS_PROXY_URL={proxy_url}"),
        format!(
            "MS_WORK_DIR={}",
            scope.mounts.first().map_or("/work".into(), |mount| mount
                .container_path
                .to_string_lossy()
                .into_owned())
        ),
    ];
    if let Some(model) = &config.microswarm.model {
        env.extend([
            format!("MS_MODEL_PROVIDER={}", model.provider),
            format!("MS_MODEL_NAME={}", model.model),
            format!("MS_MODEL_CREDENTIAL={}", model.credential),
        ]);
    }
    env.push(format!(
        "MS_MCP_SERVERS={}",
        serde_json::to_string(&mcp_descriptors(config)).map_err(|error| error.to_string())?
    ));
    let profile = ContainerProfile::for_session(
        &scope,
        &config.microswarm.runtime.image,
        &session_dir,
        &network_id,
        &host_gateway,
        &container_user,
    )?;
    let container_id = runtime.create_container(&profile, &env).await?;
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: id.clone(),
            status: "starting".into(),
            container_id: Some(container_id.clone()),
            restart_count: 0,
            last_active_at: utc_now(),
            closed_at: None,
            close_reason: None,
        })
        .map_err(|error| error.to_string())?;
    runtime.start(&container_id).await?;
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: id.clone(),
            status: "ready".into(),
            container_id: Some(container_id),
            restart_count: 0,
            last_active_at: utc_now(),
            closed_at: None,
            close_reason: None,
        })
        .map_err(|error| error.to_string())?;
    if let Err(error) = wait_agent_ready(&mut session).await {
        let container_id = store
            .session_container_id(&id)
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        let state = runtime
            .container_state(&container_id)
            .await
            .unwrap_or_default();
        let logs = runtime
            .container_logs(&container_id)
            .await
            .unwrap_or_default();
        return Err(format!(
            "{error}; {state}; agent container log: {}",
            logs.chars().take(1000).collect::<String>()
        ));
    }
    drop(session);
    Ok(id)
}

async fn handle_delegation_request(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    lead_session_dir: &Path,
    lead_id: &str,
    body: &Value,
) -> Result<Option<String>, String> {
    if body.get("op").and_then(Value::as_str) == Some("list_members") {
        let mut statement = store
            .connection()
            .prepare(
                "SELECT id, role, status, scope_json FROM sessions
             WHERE parent_id=?1 AND kind='member' ORDER BY created_at",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([lead_id], |row| {
                let scope: Option<String> = row.get(3)?;
                Ok(json!({
                    "member_id":row.get::<_, String>(0)?,
                    "role":row.get::<_, String>(1)?,
                    "status":row.get::<_, String>(2)?,
                    "scope":scope.and_then(|scope| serde_json::from_str::<Value>(&scope).ok()),
                }))
            })
            .map_err(|error| error.to_string())?;
        let members = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        open_session_dir(lead_session_dir)
            .map_err(|error| error.to_string())?
            .append_inbound(
                &InboundKind::DelegationResult,
                &json!({"status":"listed","members":members}),
            )
            .map_err(|error| error.to_string())?;
        return Ok(None);
    }
    let role_name = body
        .get("role")
        .and_then(Value::as_str)
        .ok_or("spawn request role is missing")?;
    let task = body
        .get("task")
        .and_then(Value::as_str)
        .ok_or("spawn request task is missing")?;
    let request = body.get("scope").cloned().unwrap_or_else(|| json!({}));
    let timeout_s = body.get("timeout_s").and_then(Value::as_u64).unwrap_or(300);
    let (lead_hash, lead_scope) = store
        .session_scope(lead_id)
        .map_err(|error| error.to_string())?
        .ok_or("lead has no authoritative scope")?;
    let lead_scope: Scope =
        serde_json::from_value(lead_scope).map_err(|error| error.to_string())?;
    if lead_scope.hash != lead_hash || lead_scope.derived_from != DerivedFrom::Config {
        return Err("lead authoritative scope failed validation".into());
    }
    let delegation_id = new_id()?;
    let member_id = new_id()?;
    let roles = store
        .team_roles_named(role_name)
        .map_err(|error| error.to_string())?;
    let role = if roles.len() == 1 {
        Some(roles[0].clone())
    } else {
        None
    };
    if role.is_none() && role_name != "adhoc" {
        return reject_delegation(
            store,
            lead_session_dir,
            &delegation_id,
            lead_id,
            role_name,
            task,
            &request,
            "spawn_error",
            timeout_s,
        );
    }
    let skill_policy = role
        .as_ref()
        .map(|role| parse_spawn_policy(&role.spawn_policy))
        .transpose()?;
    let explicit_policy = config.policy.spawn.roles.get(role_name).copied();
    let policy = crate::policy::resolve_spawn_policy(
        skill_policy,
        explicit_policy,
        config.policy.spawn.default,
    );
    let max_members = lead_scope
        .delegation
        .as_ref()
        .filter(|delegation| delegation.enabled)
        .map(|delegation| u64::from(delegation.max_members))
        .unwrap_or(0);
    let active = store
        .active_member_count(lead_id)
        .map_err(|error| error.to_string())?;
    if active >= max_members {
        return reject_delegation(
            store,
            lead_session_dir,
            &delegation_id,
            lead_id,
            role_name,
            task,
            &request,
            "limit",
            timeout_s,
        );
    }
    if policy == SpawnPolicy::Deny {
        return reject_delegation(
            store,
            lead_session_dir,
            &delegation_id,
            lead_id,
            role_name,
            task,
            &request,
            "policy",
            timeout_s,
        );
    }
    let ceiling = ScopeCeiling {
        paths: config.ceiling.paths.clone(),
        credentials: config.ceiling.credentials.clone(),
        tools: config.ceiling.tools.clone(),
        limits: None,
    };
    let role_scope = role
        .as_ref()
        .map(|role| role.scope.clone())
        .unwrap_or_else(|| request.clone());
    let scope = match crate::scope::build_member_scope(
        &lead_scope,
        &role_scope,
        &request,
        &member_id,
        &delegation_id,
        &ceiling,
    ) {
        Ok(scope) => scope,
        Err(error) if error.contains("scope_empty") => {
            return reject_delegation(
                store,
                lead_session_dir,
                &delegation_id,
                lead_id,
                role_name,
                task,
                &request,
                "scope_empty",
                timeout_s,
            );
        }
        Err(error) => return Err(error),
    };
    let timestamp = utc_now();
    let mut delegation = DelegationRecord {
        id: delegation_id.clone(),
        lead_id: lead_id.into(),
        member_id: None,
        role: role_name.into(),
        task: task.into(),
        scope_request: request.clone(),
        policy: match policy {
            SpawnPolicy::Allow => "allow",
            SpawnPolicy::Ask => "ask",
            SpawnPolicy::Deny => "deny",
        }
        .into(),
        approval_id: None,
        timeout_s,
        status: "requested".into(),
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
    };
    if policy == SpawnPolicy::Ask {
        let approval_payload = json!({
            "delegation_id":delegation_id,"member_id":member_id,"lead_id":lead_id,
            "role":role_name,"task":task,"scope_request":request,"scope":scope,
            "skill":role.as_ref().map(|role| &role.skill),
            "instructions":role.as_ref().map(|role| role.instructions.as_str()).unwrap_or(""),
            "timeout_s":timeout_s,
        });
        let approval = crate::approvals::request_spawn_approval(
            store,
            lead_id,
            approval_payload,
            Duration::from_secs(config.microswarm.limits.approval_timeout_s),
        )
        .map_err(|error| error.to_string())?;
        delegation.approval_id = Some(approval.id.clone());
        delegation.status = "awaiting_approval".into();
        delegation.updated_at = utc_now();
        store
            .insert_delegation(&delegation)
            .map_err(|error| error.to_string())?;
        return Ok(Some(format!(
            "[lead] wants to start a member '{role_name}'\n  task:   {task}\n  mounts: {}\n  tools:  {}\n  credentials: {}\n  /approve {}   /deny {}",
            scope
                .mounts
                .iter()
                .map(|mount| format!("{} {:?}", mount.container_path.display(), mount.mode))
                .collect::<Vec<_>>()
                .join(" · "),
            scope
                .tools
                .iter()
                .map(|grant| format!("{} {:?}", grant.server, grant.tools))
                .collect::<Vec<_>>()
                .join(", "),
            scope.credentials.join(", "),
            approval.code,
            approval.code,
        )));
    }
    store
        .insert_delegation(&delegation)
        .map_err(|error| error.to_string())?;
    store
        .update_delegation_status(&delegation_id, "spawning", None, None, &utc_now())
        .map_err(|error| error.to_string())?;
    let mut startup = MemberStartup::default();
    match create_member(
        home,
        config,
        store,
        lead_id,
        &delegation_id,
        &member_id,
        role_name,
        role.as_ref().map(|role| role.skill.as_str()),
        task,
        timeout_s,
        role.as_ref()
            .map(|role| role.instructions.as_str())
            .unwrap_or(""),
        scope.clone(),
    )
    .await
    {
        Ok(result) => {
            if result.prompt.is_none() {
                store
                    .update_delegation_status(
                        &delegation_id,
                        "active",
                        Some(&member_id),
                        None,
                        &utc_now(),
                    )
                    .map_err(|error| error.to_string())?;
            }
            startup = result;
            emit_supervisor_event(
                store,
                lead_id,
                lead_id,
                role_name,
                SupervisorEventType::DelegationSpawned,
                json!({"delegation_id":delegation_id,"member_id":member_id,"role":role_name}),
            )?;
            if !scope.narrowed.is_empty() {
                emit_supervisor_event(
                    store,
                    lead_id,
                    lead_id,
                    role_name,
                    SupervisorEventType::ScopeNarrowed,
                    json!({"delegation_id":delegation_id,"member_id":member_id,"narrowed":scope.narrowed}),
                )?;
            }
            open_session_dir(lead_session_dir).map_err(|error| error.to_string())?.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"spawned","member_id":member_id,"granted":scope,"narrowed":scope.narrowed,"initial_reply":startup.reply})).map_err(|error| error.to_string())?;
        }
        Err(error) => {
            store
                .update_delegation_status(&delegation_id, "failed", None, None, &utc_now())
                .map_err(|store_error| store_error.to_string())?;
            store
                .connection()
                .execute(
                    "UPDATE delegations SET failure_reason='spawn_error' WHERE id=?1",
                    [&delegation_id],
                )
                .map_err(|store_error| store_error.to_string())?;
            store.connection().execute("UPDATE sessions SET status='failed',close_reason='failed',closed_at=?2 WHERE id=?1 AND status IN ('pending','starting')", rusqlite::params![member_id,utc_now()]).map_err(|store_error|store_error.to_string())?;
            open_session_dir(lead_session_dir).map_err(|store_error| store_error.to_string())?.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"denied","reason":"spawn_error","detail":error})).map_err(|store_error| store_error.to_string())?;
            open_session_dir(lead_session_dir).map_err(|store_error|store_error.to_string())?.append_inbound(&InboundKind::MemberFailed,&json!({"member_id":member_id,"delegation_id":delegation_id,"reason":"spawn_error"})).map_err(|store_error|store_error.to_string())?;
        }
    }
    Ok(startup.prompt)
}

fn parse_spawn_policy(value: &str) -> Result<SpawnPolicy, String> {
    match value {
        "allow" => Ok(SpawnPolicy::Allow),
        "ask" => Ok(SpawnPolicy::Ask),
        "deny" => Ok(SpawnPolicy::Deny),
        _ => Err(format!("invalid stored spawn policy {value:?}")),
    }
}

#[allow(clippy::too_many_arguments)]
fn reject_delegation(
    store: &mut SupervisorStore,
    lead_session_dir: &Path,
    delegation_id: &str,
    lead_id: &str,
    role: &str,
    task: &str,
    request: &Value,
    reason: &str,
    timeout_s: u64,
) -> Result<Option<String>, String> {
    let now = utc_now();
    store
        .insert_delegation(&DelegationRecord {
            id: delegation_id.into(),
            lead_id: lead_id.into(),
            member_id: None,
            role: role.into(),
            task: task.into(),
            scope_request: request.clone(),
            policy: "deny".into(),
            approval_id: None,
            timeout_s,
            status: if reason == "policy" {
                "denied"
            } else {
                "failed"
            }
            .into(),
            created_at: now.clone(),
            updated_at: now,
        })
        .map_err(|error| error.to_string())?;
    emit_supervisor_event(
        store,
        lead_id,
        lead_id,
        role,
        if reason == "policy" {
            SupervisorEventType::DelegationDenied
        } else {
            SupervisorEventType::DelegationFailed
        },
        json!({"delegation_id":delegation_id,"role":role,"reason":reason}),
    )?;
    open_session_dir(lead_session_dir)
        .map_err(|error| error.to_string())?
        .append_inbound(
            &InboundKind::DelegationResult,
            &json!({"delegation_id":delegation_id,"status":"denied","reason":reason}),
        )
        .map_err(|error| error.to_string())?;
    Ok(None)
}

pub(crate) fn emit_supervisor_event(
    store: &mut SupervisorStore,
    session_id: &str,
    root_session_id: &str,
    role: &str,
    event_type: SupervisorEventType,
    data: Value,
) -> Result<(), String> {
    let ts_mono = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX);
    store
        .append_supervisor_event(&SupervisorEvent {
            seq: 0,
            ts: utc_now(),
            ts_mono,
            session_id: session_id.into(),
            root_session_id: root_session_id.into(),
            agent_role: role.into(),
            source: Default::default(),
            event_type,
            data,
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Default)]
struct MemberStartup {
    prompt: Option<String>,
    reply: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn create_member(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    lead_id: &str,
    delegation_id: &str,
    member_id: &str,
    role: &str,
    skill: Option<&str>,
    task: &str,
    timeout_s: u64,
    instructions: &str,
    scope: Scope,
) -> Result<MemberStartup, String> {
    let session_dir = home.join("sessions").join(member_id);
    fs::create_dir_all(&session_dir).map_err(|error| error.to_string())?;
    let mut session = open_session_dir(&session_dir).map_err(|error| error.to_string())?;
    let record = SessionRecord {
        id: member_id.into(),
        root_id: lead_id.into(),
        parent_id: Some(lead_id.into()),
        role: role.into(),
        kind: "member".into(),
        channel: None,
        channel_thread: None,
        status: "pending".into(),
        scope_hash: None,
        scope: None,
        created_at: utc_now(),
        last_active_at: utc_now(),
    };
    crate::scope::install_member_scope(store, record, &session_dir, &scope)?;
    let task_id = session
        .append_inbound(
            &InboundKind::Task,
            &json!({"delegation_id":delegation_id,"from_lead":lead_id,"text":task}),
        )
        .map_err(|error| error.to_string())?;
    let runtime = runtime_for(config)?;
    let network = format!("ms-net-{member_id}");
    let network_id = runtime.create_network(&network).await?;
    let host_gateway = runtime.network_gateway(&network_id).await?;
    let container_user = runtime.container_user(&session_dir)?;
    let listener = TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|error| error.to_string())?;
    let proxy_addr = listener.local_addr().map_err(|error| error.to_string())?;
    let proxy = SessionProxy::from_store(store, member_id).map_err(|error| error.to_string())?;
    let routes = proxy_routes(config);
    let credentials = Arc::new(HomeCredentials {
        home: home.to_path_buf(),
    });
    tokio::spawn(async move {
        if let Err(error) = proxy.serve(listener, routes, credentials).await {
            eprintln!("member session proxy listener stopped: {error}");
        }
    });
    let mut env = vec![
        format!("MS_SESSION_ID={member_id}"),
        format!("MS_ROOT_SESSION_ID={lead_id}"),
        "MS_KIND=member".into(),
        format!("MS_ROLE={role}"),
        "MS_SESSION_DIR=/session".into(),
        format!(
            "MS_PROXY_URL=http://host.docker.internal:{}",
            proxy_addr.port()
        ),
        format!(
            "MS_WORK_DIR={}",
            scope.mounts.first().map_or("/work".into(), |mount| mount
                .container_path
                .to_string_lossy()
                .into_owned())
        ),
    ];
    if let Some(model) = &config.microswarm.model {
        env.extend([
            format!("MS_MODEL_PROVIDER={}", model.provider),
            format!("MS_MODEL_NAME={}", model.model),
        ]);
        if scope.credentials.contains(&model.credential) {
            env.push(format!("MS_MODEL_CREDENTIAL={}", model.credential));
        }
    }
    env.push(format!(
        "MS_MCP_SERVERS={}",
        serde_json::to_string(&mcp_descriptors(config)).map_err(|error| error.to_string())?
    ));
    let mut profile = ContainerProfile::for_session(
        &scope,
        &config.microswarm.runtime.image,
        &session_dir,
        &network_id,
        &host_gateway,
        &container_user,
    )?;
    if let Some(skill) = skill {
        let instruction_path = fs::canonicalize(instructions)
            .map_err(|error| format!("cannot resolve role instructions: {error}"))?;
        if !instruction_path.is_file() {
            return Err("role instructions are not a file".into());
        }
        profile.mounts.push(crate::runtime::MountSpec {
            source: instruction_path.display().to_string(),
            target: format!("/skills/{skill}/role.md"),
            read_only: true,
        });
        env.push(format!("MS_ROLE_INSTRUCTIONS=/skills/{skill}/role.md"));
    }
    let container_id = runtime.create_container(&profile, &env).await?;
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: member_id.into(),
            status: "starting".into(),
            container_id: Some(container_id.clone()),
            restart_count: 0,
            last_active_at: utc_now(),
            closed_at: None,
            close_reason: None,
        })
        .map_err(|error| error.to_string())?;
    runtime.start(&container_id).await?;
    store
        .update_session_lifecycle(&SessionLifecycleUpdate {
            session_id: member_id.into(),
            status: "ready".into(),
            container_id: Some(container_id),
            restart_count: 0,
            last_active_at: utc_now(),
            closed_at: None,
            close_reason: None,
        })
        .map_err(|error| error.to_string())?;
    wait_agent_ready(&mut session).await?;
    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    let mut after_id = 0;
    loop {
        for reply in session
            .outbound_after(after_id)
            .map_err(|error| error.to_string())?
        {
            if let Some(id) = reply.id {
                after_id = id;
            }
            if reply.kind == OutboundKind::ApprovalRequest {
                let binding: ms_proto::ApprovalBinding = serde_json::from_value(
                    reply
                        .body
                        .get("binding")
                        .cloned()
                        .ok_or("member approval has no binding")?,
                )
                .map_err(|error| error.to_string())?;
                let arguments = reply.body.get("args").cloned().unwrap_or(Value::Null);
                let approval = crate::approvals::request_tool_approval(
                    store,
                    member_id,
                    lead_id,
                    &binding,
                    &arguments,
                    Duration::from_secs(config.microswarm.limits.approval_timeout_s),
                )
                .map_err(|error| error.to_string())?;
                store.connection().execute(
                    "UPDATE delegations SET status='waiting_reply',waiting_since=?2,updated_at=?2 WHERE id=?1",
                    [delegation_id,utc_now_precise().as_str()],
                ).map_err(|error| error.to_string())?;
                let rendered = crate::approvals::render_prompt(
                    config.microswarm.home.lang,
                    &approval.code,
                    &binding.server,
                    &binding.tool,
                    &arguments,
                );
                return Ok(MemberStartup {
                    prompt: Some(format!("[{role} {member_id}] {rendered}")),
                    reply: None,
                });
            }
            if reply.kind == OutboundKind::Reply && reply.in_reply_to == Some(task_id) {
                let response = reply.body.get("text").and_then(Value::as_str).unwrap_or("");
                emit_supervisor_event(
                    store,
                    member_id,
                    lead_id,
                    role,
                    SupervisorEventType::DelegationReply,
                    json!({"member_id":member_id,"delegation_id":delegation_id}),
                )?;
                return Ok(MemberStartup {
                    prompt: None,
                    reply: Some(response.to_owned()),
                });
            }
        }
        if Instant::now() >= deadline {
            return Err("member did not finish its initial task before timeout".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn handle_send_to_member(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    lead_session_dir: &Path,
    lead_id: &str,
    body: &Value,
) -> Result<Option<String>, String> {
    let member_id = body
        .get("member_id")
        .and_then(Value::as_str)
        .ok_or("send_to_member has no member_id")?;
    let text = body
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or("send_to_member has no text")?;
    let timeout_s = body.get("timeout_s").and_then(Value::as_u64).unwrap_or(300);
    if !(10..=3600).contains(&timeout_s) {
        return Err("send_to_member timeout_s must be between 10 and 3600".into());
    }
    let member = store
        .member_route(lead_id, member_id)
        .map_err(|error| error.to_string())?
        .ok_or("unknown_member")?;
    if member.parent_id != lead_id || member.kind != "member" {
        return Err("member does not belong to this lead".into());
    }
    if !matches!(member.status.as_str(), "ready" | "busy") {
        open_session_dir(lead_session_dir)
            .map_err(|error| error.to_string())?
            .append_inbound(
                &InboundKind::MemberFailed,
                &json!({"member_id":member_id,"reason":"crashed"}),
            )
            .map_err(|error| error.to_string())?;
        return Ok(None);
    }
    let container_id = member
        .container_id
        .clone()
        .ok_or("member has no container")?;
    let delegation_id = member
        .delegation_id
        .clone()
        .ok_or("member has no delegation record")?;
    let member_session = open_session_dir(home.join("sessions").join(member_id))
        .map_err(|error| error.to_string())?;
    let cursor: i64 = member_session
        .outbound()
        .query_row("SELECT COALESCE(MAX(id),0) FROM outbound", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    let task_id = member_session
        .append_inbound(
            &InboundKind::Task,
            &json!({"delegation_id":delegation_id,"from_lead":lead_id,"text":text}),
        )
        .map_err(|error| error.to_string())?;
    store.connection().execute(
        "UPDATE delegations SET status='waiting_reply',waiting_since=strftime('%Y-%m-%dT%H:%M:%fZ','now'),deadline_at=strftime('%Y-%m-%dT%H:%M:%fZ','now','+' || ?2 || ' seconds'),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND status IN ('active','spawning')",
        rusqlite::params![delegation_id,i64::try_from(timeout_s).map_err(|error|error.to_string())?],
    ).map_err(|error|error.to_string())?;
    store.connection().execute(
        "UPDATE sessions SET status='busy',last_active_at=?2 WHERE id=?1 AND status IN ('ready','busy')",
        [member_id,utc_now().as_str()],
    ).map_err(|error|error.to_string())?;
    let runtime = runtime_for(config)?;
    runtime.signal(&container_id, "SIGUSR1").await?;
    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    let mut after_id = cursor;
    loop {
        if Instant::now() >= deadline {
            store.connection().execute("UPDATE delegations SET deadline_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND status='waiting_reply'",[&delegation_id]).map_err(|error|error.to_string())?;
            crate::deadlines::process_due(home).await?;
            return Ok(None);
        }
        for reply in member_session
            .outbound_after(after_id)
            .map_err(|error| error.to_string())?
        {
            if Instant::now() >= deadline {
                break;
            }
            if let Some(id) = reply.id {
                after_id = id;
            }
            if reply.kind == OutboundKind::ApprovalRequest {
                let binding: ms_proto::ApprovalBinding = serde_json::from_value(
                    reply
                        .body
                        .get("binding")
                        .cloned()
                        .ok_or("member approval has no binding")?,
                )
                .map_err(|error| error.to_string())?;
                let arguments = reply.body.get("args").cloned().unwrap_or(Value::Null);
                let approval = crate::approvals::request_tool_approval(
                    store,
                    member_id,
                    lead_id,
                    &binding,
                    &arguments,
                    Duration::from_secs(config.microswarm.limits.approval_timeout_s),
                )
                .map_err(|error| error.to_string())?;
                store.connection().execute(
                    "UPDATE delegations SET status='waiting_reply',waiting_since=?2,updated_at=?2 WHERE id=?1",
                    [&delegation_id,utc_now_precise().as_str()],
                ).map_err(|error|error.to_string())?;
                let rendered = crate::approvals::render_prompt(
                    config.microswarm.home.lang,
                    &approval.code,
                    &binding.server,
                    &binding.tool,
                    &arguments,
                );
                return Ok(Some(format!("[{} {}] {rendered}", member.role, member_id)));
            }
            if reply.kind != OutboundKind::Reply || reply.in_reply_to != Some(task_id) {
                continue;
            }
            let response = reply.body.get("text").and_then(Value::as_str).unwrap_or("");
            open_session_dir(lead_session_dir)
                .map_err(|error| error.to_string())?
                .append_inbound(
                    &InboundKind::MemberReply,
                    &json!({"member_id":member_id,"delegation_id":delegation_id,"text":response}),
                )
                .map_err(|error| error.to_string())?;
            store.connection().execute(
                "UPDATE delegations SET status='active',waiting_since=NULL,deadline_at=NULL,updated_at=?2 WHERE id=?1",
                [&delegation_id,utc_now().as_str()],
            ).map_err(|error|error.to_string())?;
            store.connection().execute(
                "UPDATE sessions SET status='ready',last_active_at=?2 WHERE id=?1 AND status='busy'",
                [member_id,utc_now().as_str()],
            ).map_err(|error|error.to_string())?;
            emit_supervisor_event(
                store,
                member_id,
                lead_id,
                &member.kind,
                SupervisorEventType::DelegationReply,
                json!({"member_id":member_id,"delegation_id":delegation_id}),
            )?;
            return Ok(None);
        }
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM sessions WHERE id=?1",
                [member_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if matches!(status.as_str(), "closed" | "failed") {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn handle_dismiss_member(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    lead_session_dir: &Path,
    lead_id: &str,
    body: &Value,
) -> Result<(), String> {
    let member_id = body
        .get("member_id")
        .and_then(Value::as_str)
        .ok_or("dismiss has no member_id")?;
    let lead = open_session_dir(lead_session_dir).map_err(|error| error.to_string())?;
    let Some(member) = store
        .member_route(lead_id, member_id)
        .map_err(|error| error.to_string())?
    else {
        lead.append_inbound(
            &InboundKind::DelegationResult,
            &json!({"status":"unknown_member","member_id":member_id}),
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    };
    if member.kind != "member" {
        return Err("dismiss target is not a member".into());
    }
    if matches!(member.status.as_str(), "closed" | "failed") {
        lead.append_inbound(
            &InboundKind::DelegationResult,
            &json!({"status":"already_closed","member_id":member_id}),
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let container_id = member.container_id.ok_or("member has no container")?;
    graceful_stop_session(home, config, member_id, &container_id).await?;
    let now = utc_now();
    store
        .connection()
        .execute(
            "UPDATE sessions SET status='closed', close_reason='dismissed', closed_at=?2,
             last_active_at=?2 WHERE id=?1 AND parent_id=?3 AND kind='member'",
            [member_id, now.as_str(), lead_id],
        )
        .map_err(|error| error.to_string())?;
    store
        .connection()
        .execute(
            "UPDATE delegations SET status='dismissed', updated_at=?2 WHERE member_id=?1 AND lead_id=?3",
            [member_id, now.as_str(), lead_id],
        )
        .map_err(|error| error.to_string())?;
    emit_supervisor_event(
        store,
        member_id,
        lead_id,
        &member.kind,
        SupervisorEventType::DelegationDismissed,
        json!({"member_id":member_id}),
    )?;
    lead.append_inbound(
        &InboundKind::DelegationResult,
        &json!({"status":"dismissed","member_id":member_id}),
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn wait_agent_ready(session: &mut SessionConnections) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let ready: bool = session
            .events()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE type='agent.ready')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if ready {
            return Ok(());
        }
        let inbound: i64 = session
            .inbound()
            .query_row("SELECT COUNT(*) FROM inbound", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        let events: String = session
            .events()
            .query_row(
                "SELECT COALESCE(group_concat(type, ','),'') FROM events",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if events.contains("agent.exit") {
            return Err(format!(
                "agent exited before ready (inbound rows={inbound}, events={events})"
            ));
        }
        if Instant::now() >= deadline {
            return Err("agent did not become ready".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn runtime_for(config: &ConfigSet) -> Result<ContainerRuntime, String> {
    ContainerRuntime::connect(&config.microswarm.runtime.socket)
}

fn proxy_routes(config: &ConfigSet) -> ProxyRoutes {
    let model = config.microswarm.model.as_ref();
    ProxyRoutes {
        model_upstream: model.map(|model| model.base_url.clone()),
        model_credential: model.map(|model| model.credential.clone()),
        anthropic_model: model.is_some_and(|model| model.provider == "anthropic"),
        tools: config
            .tools
            .servers
            .iter()
            .filter_map(|server| {
                server
                    .url
                    .as_ref()
                    .map(|url| (server.id.clone(), url.clone()))
            })
            .collect(),
    }
}

fn mcp_descriptors(config: &ConfigSet) -> Vec<Value> {
    config.tools.servers.iter().map(|server| json!({
        "id":server.id,
        "transport":match server.transport { ToolTransport::Stdio => "stdio", ToolTransport::StreamableHttp => "streamable_http" },
        "command":server.command,
        "args":server.args,
        "url":server.url,
        "credential":server.credential,
    })).collect()
}

struct HomeCredentials {
    home: PathBuf,
}

impl CredentialProvider for HomeCredentials {
    fn resolve(&self, name: &str) -> Option<String> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return None;
        }
        fs::read_to_string(self.home.join("credentials").join(name)).ok()
    }
}

fn new_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| error.to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn utc_now() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (elapsed.as_secs() / 86_400) as i64;
    let sec = elapsed.as_secs() % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        sec / 3600,
        (sec % 3600) / 60,
        sec % 60
    )
}

fn utc_now_precise() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let second = utc_now();
    format!(
        "{}.{:03}Z",
        second.trim_end_matches('Z'),
        elapsed.subsec_millis()
    )
}

fn store_credential(home: &Path, request: &Value) -> Result<Value, String> {
    let name = request
        .get("name")
        .and_then(Value::as_str)
        .ok_or("credential name is missing")?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        return Err("invalid credential name".into());
    }
    let value = request
        .get("value")
        .and_then(Value::as_str)
        .ok_or("credential value is missing")?;
    let directory = home.join("credentials");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let path = directory.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    use std::io::Write;
    file.write_all(value.as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(json!({"ok":true}))
}

fn list_credentials(home: &Path) -> Result<Value, String> {
    let directory = home.join("credentials");
    if !directory.exists() {
        return Ok(json!({"names":[]}));
    }
    let mut names = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    Ok(json!({"names":names}))
}

async fn resolve_approval(home: &Path, request: &Value) -> Result<Value, String> {
    let code = request
        .get("code")
        .and_then(Value::as_str)
        .ok_or("approval code is missing")?;
    let approve = request.get("op").and_then(Value::as_str) == Some("approve");
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let approval = store
        .pending_approval(code)
        .map_err(|error| error.to_string())?
        .ok_or("pending approval code was not found")?;
    let container_id = store
        .session_container_id(&approval.requesting_session_id)
        .map_err(|error| error.to_string())?;
    let session_dir = home.join("sessions").join(&approval.requesting_session_id);
    let session = open_session_dir(session_dir).map_err(|error| error.to_string())?;
    if approval.kind == "spawn" {
        let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
        let now = utc_now();
        let timed_out = approval.expires_at <= now;
        let decision = if timed_out {
            "timed_out"
        } else if approve {
            "approved"
        } else {
            "denied"
        };
        if !store
            .resolve_approval(&approval.id, decision, &now)
            .map_err(|error| error.to_string())?
        {
            return Err("approval was already resolved".into());
        }
        let delegation_id = approval
            .payload
            .get("delegation_id")
            .and_then(Value::as_str)
            .ok_or("spawn approval has no delegation id")?;
        let member_id = approval
            .payload
            .get("member_id")
            .and_then(Value::as_str)
            .ok_or("spawn approval has no member id")?;
        let role = approval
            .payload
            .get("role")
            .and_then(Value::as_str)
            .ok_or("spawn approval has no role")?;
        let task = approval
            .payload
            .get("task")
            .and_then(Value::as_str)
            .ok_or("spawn approval has no task")?;
        let scope: Scope = serde_json::from_value(
            approval
                .payload
                .get("scope")
                .cloned()
                .ok_or("spawn approval has no scope")?,
        )
        .map_err(|error| error.to_string())?;
        let timeout_s = approval
            .payload
            .get("timeout_s")
            .and_then(Value::as_u64)
            .unwrap_or(300);
        let lead_id = approval.requesting_session_id.as_str();
        let mut prompt_for_lead = None;
        if decision != "approved" {
            store
                .update_delegation_status(delegation_id, "denied", None, None, &now)
                .map_err(|error| error.to_string())?;
            session.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"denied","reason":if decision == "timed_out" {"approval_timeout"} else {"user_denied"}})).map_err(|error| error.to_string())?;
        } else {
            store
                .update_delegation_status(delegation_id, "spawning", None, None, &now)
                .map_err(|error| error.to_string())?;
            match create_member(
                home,
                &config,
                &mut store,
                lead_id,
                delegation_id,
                member_id,
                role,
                approval.payload.get("skill").and_then(Value::as_str),
                task,
                timeout_s,
                approval
                    .payload
                    .get("instructions")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                scope.clone(),
            )
            .await
            {
                Ok(startup) => {
                    if startup.prompt.is_none() {
                        store
                            .update_delegation_status(
                                delegation_id,
                                "active",
                                Some(member_id),
                                None,
                                &utc_now(),
                            )
                            .map_err(|error| error.to_string())?;
                    }
                    session.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"spawned","member_id":member_id,"granted":scope,"narrowed":scope.narrowed,"initial_reply":startup.reply})).map_err(|error| error.to_string())?;
                    prompt_for_lead = startup.prompt;
                }
                Err(error) => {
                    store
                        .update_delegation_status(delegation_id, "failed", None, None, &utc_now())
                        .map_err(|error| error.to_string())?;
                    session.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"denied","reason":"spawn_error","detail":error})).map_err(|error| error.to_string())?;
                }
            }
        }
        if let Some(container_id) = container_id {
            runtime_for(&config)?
                .signal(&container_id, "SIGUSR1")
                .await?;
        }
        return Ok(
            json!({"ok":true,"approval_id":approval.id,"decision":decision,"prompt":prompt_for_lead}),
        );
    }
    crate::approvals::resolve_by_code(&mut store, &session, code, approve)
        .map_err(|error| error.to_string())?;
    if let Some(container_id) = container_id {
        let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
        runtime_for(&config)?
            .signal(&container_id, "SIGUSR1")
            .await?;
    }
    Ok(
        json!({"ok":true, "approval_id":approval.id, "decision":if approve {"approved"} else {"denied"}}),
    )
}

async fn wait_for_reply(home: &Path, request: &Value) -> Result<Value, String> {
    let session_id = request
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or("wait session_id is missing")?;
    let after_id = request.get("after_id").and_then(Value::as_i64).unwrap_or(0);
    let session = open_session_dir(home.join("sessions").join(session_id))
        .map_err(|error| error.to_string())?;
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let container_id = store
        .session_container_id(session_id)
        .map_err(|error| error.to_string())?;
    let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
    let runtime = runtime_for(&config)?;
    let deadline =
        Instant::now() + Duration::from_secs(config.microswarm.limits.approval_timeout_s + 90);
    loop {
        route_waiting_member_replies(home, &config, &mut store, session_id).await?;
        if crate::approvals::expire_due(&mut store, session_id, &session)
            .map_err(|error| error.to_string())?
            > 0
            && let Some(container_id) = &container_id
        {
            runtime.signal(container_id, "SIGUSR1").await?;
        }
        if let Some(reply) = session
            .outbound_after(after_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|message| message.kind == OutboundKind::Reply)
        {
            let active_at = utc_now();
            store
                .connection()
                .execute(
                    "UPDATE sessions SET status='ready', last_active_at=?2 WHERE id=?1",
                    [session_id, active_at.as_str()],
                )
                .map_err(|error| error.to_string())?;
            return Ok(
                json!({"session_id":session_id,"text":reply.body.get("text").and_then(Value::as_str).unwrap_or("")}),
            );
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for approval result".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn route_waiting_member_replies(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    lead_id: &str,
) -> Result<(), String> {
    let waiting = {
        let mut statement=store.connection().prepare("SELECT id,member_id,waiting_since FROM delegations WHERE lead_id=?1 AND status='waiting_reply' AND member_id IS NOT NULL").map_err(|e|e.to_string())?;
        statement
            .query_map([lead_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut routed = false;
    for (delegation_id, member_id, since) in waiting {
        let Some(since) = since else { continue };
        let member =
            open_session_dir(home.join("sessions").join(&member_id)).map_err(|e| e.to_string())?;
        let reply=member.outbound().query_row(
            "SELECT body FROM outbound WHERE kind='reply' AND created_at>?1 ORDER BY id DESC LIMIT 1",[since],|row|row.get::<_,String>(0)
        );
        let Ok(body) = reply else { continue };
        let body: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let text = body.get("text").and_then(Value::as_str).unwrap_or("");
        open_session_dir(home.join("sessions").join(lead_id))
            .map_err(|e| e.to_string())?
            .append_inbound(
                &InboundKind::MemberReply,
                &json!({"member_id":member_id,"delegation_id":delegation_id,"text":text}),
            )
            .map_err(|e| e.to_string())?;
        store.connection().execute("UPDATE delegations SET status='active',waiting_since=NULL,deadline_at=NULL,updated_at=?2 WHERE id=?1 AND status='waiting_reply'",[&delegation_id,utc_now().as_str()]).map_err(|e|e.to_string())?;
        emit_supervisor_event(
            store,
            &member_id,
            lead_id,
            "member",
            SupervisorEventType::DelegationReply,
            json!({"member_id":member_id,"delegation_id":delegation_id}),
        )?;
        routed = true;
    }
    if routed {
        let container = store
            .session_container_id(lead_id)
            .map_err(|e| e.to_string())?;
        if let Some(container) = container {
            runtime_for(config)?.signal(&container, "SIGUSR1").await?;
        }
    }
    Ok(())
}

pub(crate) fn config_dir(home: &Path) -> PathBuf {
    let nested = home.join("config");
    if nested.join("microswarm.toml").is_file() {
        nested
    } else {
        home.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::claim_session_busy;
    use ms_store::{SessionRecord, SupervisorStore};
    use serde_json::json;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn chat_claim_rejects_a_session_that_idle_shutdown_already_claimed() {
        let dir = std::env::temp_dir().join(format!(
            "ms-chat-claim-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
        store
            .insert_session(&SessionRecord {
                id: "01J00000000000000000000000".into(),
                root_id: "01J00000000000000000000000".into(),
                parent_id: None,
                role: "lead".into(),
                kind: "lead".into(),
                channel: Some("cli".into()),
                channel_thread: Some("default".into()),
                status: "stopping".into(),
                scope_hash: None,
                scope: Some(json!({})),
                created_at: "2026-10-29T00:00:00Z".into(),
                last_active_at: "2026-10-29T00:00:00Z".into(),
            })
            .unwrap();

        let result =
            claim_session_busy(&store, "01J00000000000000000000000", "2026-10-29T00:00:01Z");
        assert!(
            result.is_err(),
            "stopping session must not accept a chat turn"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
