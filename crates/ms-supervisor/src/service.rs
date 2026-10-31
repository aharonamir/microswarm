use crate::{
    config::{ConfigSet, SpawnPolicy, ToolTransport, load_config_dir},
    proxy::{ProxyRoutes, SessionProxy},
    runtime::{ContainerProfile, ContainerRuntime, FileCredentials},
    scope::{build_lead_scope, install_lead_scope},
};
use ms_proto::{DerivedFrom, InboundKind, OutboundKind, Scope, ScopeCeiling, SupervisorEventType};
use ms_runtime::claim_session_busy;
pub(crate) use ms_runtime::events::emit_supervisor_event;
pub(crate) use ms_runtime::{config_dir, new_id, utc_now, utc_now_precise};
use ms_store::{
    DelegationRecord, SessionLifecycleUpdate, SessionRecord, SupervisorStore, open_session_dir,
};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
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
    let config = load_config_dir(config_dir(&home)).map_err(|error| error.to_string())?;
    crate::recover::boot(&home, &config).await?;
    let retention_days = config.microswarm.home.retention_days;
    if let Err(error) =
        ms_runtime::retention::purge_expired_sessions(&home, retention_days, &utc_now())
    {
        eprintln!("session retention cleanup failed: {error}");
    }
    let retention_home = home.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            if let Err(error) = ms_runtime::retention::purge_expired_sessions(
                &retention_home,
                retention_days,
                &ms_runtime::utc_now(),
            ) {
                eprintln!("session retention cleanup failed: {error}");
            }
        }
    });
    let channels =
        serde_json::to_value(&config.microswarm.channels).map_err(|error| error.to_string())?;
    ms_channels::spawn_from_config(home.clone(), socket_path.clone(), channels);
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
            Ok(request) if request.get("op").and_then(Value::as_str) == Some("event_stream") => {
                crate::events::stream(&home, &request, &mut write).await?;
                return Ok(());
            }
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
        Some("creds_set") => ms_runtime::credentials::store_credential(home, request),
        Some("creds_list") => ms_runtime::credentials::list_credentials(home),
        Some("approve") | Some("deny") => resolve_approval(home, request).await,
        Some("chat") => chat_message(home, request).await,
        Some("channel_message") => chat_message(home, request).await,
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
        return Ok(json!({
            "text":format!("Approval {}.", resolved["decision"].as_str().unwrap_or("resolved")),
            "pending":resolved["pending"], "prompt":resolved["prompt"]
        }));
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
    let channel = request
        .get("channel")
        .and_then(Value::as_str)
        .unwrap_or("cli");
    let channel_thread = request
        .get("thread_id")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let session_id = match request.get("session_id").and_then(Value::as_str) {
        Some(id) => id.to_owned(),
        None => match store
            .active_lead_for_thread(channel, channel_thread)
            .map_err(|error| error.to_string())?
        {
            Some(id) => id,
            None => create_lead(home, &config, &mut store, channel, channel_thread).await?,
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
    let (stored_outbound, events_last_seq) =
        crate::router::tail_cursor(&store, &session_id).map_err(|error| error.to_string())?;
    let mut last_id = if stored_outbound == 0 {
        session
            .outbound()
            .query_row("SELECT COALESCE(MAX(id),0) FROM outbound", [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())?
    } else {
        stored_outbound
    };
    crate::router::persist_tail_cursor(&mut store, &session_id, last_id, events_last_seq)
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
            last_id = message.id.unwrap_or(last_id);
            crate::router::persist_tail_cursor(&mut store, &session_id, last_id, events_last_seq)
                .map_err(|error| error.to_string())?;
            if let Some(result) = process_outbound_message(
                home,
                &config,
                &mut store,
                &session_dir,
                &session_id,
                &message,
                Some(inbound_id),
            )
            .await?
            {
                return Ok(result);
            }
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for agent reply".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn create_lead(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    channel: &str,
    channel_thread: &str,
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
        channel: Some(channel.into()),
        channel_thread: Some(channel_thread.into()),
        status: "pending".into(),
        scope_hash: None,
        scope: None,
        created_at: utc_now(),
        last_active_at: utc_now(),
    };
    install_lead_scope(store, record, &session_dir, &scope)?;
    emit_supervisor_event(
        store,
        &id,
        &id,
        "lead",
        SupervisorEventType::SessionCreated,
        json!({"channel":channel,"thread_id":channel_thread}),
    )?;
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
    let credentials = Arc::new(FileCredentials {
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
    emit_supervisor_event(
        store,
        &id,
        &id,
        "lead",
        SupervisorEventType::ContainerCreated,
        json!({"container_id":container_id}),
    )?;
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
    emit_supervisor_event(
        store,
        &id,
        &id,
        "lead",
        SupervisorEventType::ContainerStarted,
        json!({"container_id":container_id}),
    )?;
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
    if let Err(error) = ms_runtime::wait_agent_ready(&mut session).await {
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
            store
                .update_delegation_status(
                    &delegation_id,
                    if result.prompt.is_some() {
                        "waiting_reply"
                    } else {
                        "active"
                    },
                    Some(&member_id),
                    None,
                    &utc_now(),
                )
                .map_err(|error| error.to_string())?;
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
            if startup.prompt.is_none() {
                open_session_dir(lead_session_dir).map_err(|error| error.to_string())?.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"spawned","member_id":member_id,"granted":scope,"narrowed":scope.narrowed,"initial_reply":startup.reply})).map_err(|error| error.to_string())?;
            }
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
    let credentials = Arc::new(FileCredentials {
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
    ms_runtime::wait_agent_ready(&mut session).await?;
    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    let (mut after_id, events_last_seq) =
        crate::router::tail_cursor(store, member_id).map_err(|error| error.to_string())?;
    crate::router::persist_tail_cursor(store, member_id, after_id, events_last_seq)
        .map_err(|error| error.to_string())?;
    loop {
        for reply in session
            .outbound_after(after_id)
            .map_err(|error| error.to_string())?
        {
            if let Some(id) = reply.id {
                after_id = id;
                crate::router::persist_tail_cursor(store, member_id, after_id, events_last_seq)
                    .map_err(|error| error.to_string())?;
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
    let status: String = store
        .connection()
        .query_row(
            "SELECT status FROM delegations WHERE id=?1",
            [&delegation_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if status == "waiting_reply" {
        open_session_dir(lead_session_dir).map_err(|error| error.to_string())?
            .append_inbound(&InboundKind::DelegationResult,
                &json!({"status":"member_busy","member_id":member_id,"delegation_id":delegation_id}))
            .map_err(|error| error.to_string())?;
        return Ok(None);
    }
    let member_session = open_session_dir(home.join("sessions").join(member_id))
        .map_err(|error| error.to_string())?;
    let current_outbound: i64 = member_session
        .outbound()
        .query_row("SELECT COALESCE(MAX(id),0) FROM outbound", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    let (_, events_last_seq) =
        crate::router::tail_cursor(store, member_id).map_err(|error| error.to_string())?;
    let cursor = current_outbound;
    crate::router::persist_tail_cursor(store, member_id, cursor, events_last_seq)
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
                crate::router::persist_tail_cursor(store, member_id, after_id, events_last_seq)
                    .map_err(|error| error.to_string())?;
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

pub(super) fn runtime_for(config: &ConfigSet) -> Result<ContainerRuntime, String> {
    ContainerRuntime::connect(&config.microswarm.runtime.socket)
}

pub(super) fn proxy_routes(config: &ConfigSet) -> ProxyRoutes {
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

pub(super) fn mcp_descriptors(config: &ConfigSet) -> Vec<Value> {
    config.tools.servers.iter().map(|server| json!({
        "id":server.id,
        "transport":match server.transport { ToolTransport::Stdio => "stdio", ToolTransport::StreamableHttp => "streamable_http" },
        "command":server.command,
        "args":server.args,
        "url":server.url,
        "credential":server.credential,
    })).collect()
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
                    store
                        .update_delegation_status(
                            delegation_id,
                            if startup.prompt.is_some() {
                                "waiting_reply"
                            } else {
                                "active"
                            },
                            Some(member_id),
                            None,
                            &utc_now(),
                        )
                        .map_err(|error| error.to_string())?;
                    if startup.prompt.is_none() {
                        session.append_inbound(&InboundKind::DelegationResult, &json!({"delegation_id":delegation_id,"status":"spawned","member_id":member_id,"granted":scope,"narrowed":scope.narrowed,"initial_reply":startup.reply})).map_err(|error| error.to_string())?;
                    }
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
            json!({"ok":true,"approval_id":approval.id,"decision":decision,"pending":prompt_for_lead.is_some(),"prompt":prompt_for_lead}),
        );
    }
    crate::approvals::resolve_by_code(&mut store, &session, code, approve)
        .map_err(|error| error.to_string())?;
    let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
    if let Some(container_id) = container_id {
        runtime_for(&config)?
            .signal(&container_id, "SIGUSR1")
            .await?;
    }
    let mut prompt = None;
    if approval.requesting_session_id != approval.root_session_id {
        let deadline =
            Instant::now() + Duration::from_secs(config.microswarm.limits.approval_timeout_s + 90);
        loop {
            prompt =
                route_waiting_member_replies(home, &config, &mut store, &approval.root_session_id)
                    .await?;
            if prompt.is_some() {
                break;
            }
            let waiting: bool = store.connection().query_row(
                "SELECT EXISTS(SELECT 1 FROM delegations WHERE member_id=?1 AND status='waiting_reply')",
                [&approval.requesting_session_id], |row| row.get(0)
            ).map_err(|error| error.to_string())?;
            if !waiting || Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    Ok(
        json!({"ok":true, "approval_id":approval.id, "decision":if approve {"approved"} else {"denied"}, "pending":prompt.is_some(), "prompt":prompt}),
    )
}

async fn wait_for_reply(home: &Path, request: &Value) -> Result<Value, String> {
    let session_id = request
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or("wait session_id is missing")?;
    let after_id = request.get("after_id").and_then(Value::as_i64).unwrap_or(0);
    let session_dir = home.join("sessions").join(session_id);
    let session = open_session_dir(&session_dir).map_err(|error| error.to_string())?;
    let mut store =
        SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
    let container_id = store
        .session_container_id(session_id)
        .map_err(|error| error.to_string())?;
    let config = load_config_dir(config_dir(home)).map_err(|error| error.to_string())?;
    let runtime = runtime_for(&config)?;
    let deadline =
        Instant::now() + Duration::from_secs(config.microswarm.limits.approval_timeout_s + 90);
    let events_last_seq = crate::router::tail_cursor(&store, session_id)
        .map_err(|e| e.to_string())?
        .1;
    let mut cursor = after_id;
    loop {
        if let Some(prompt) =
            route_waiting_member_replies(home, &config, &mut store, session_id).await?
        {
            return Ok(
                json!({"session_id":session_id,"pending":true,"prompt":prompt,"after_id":cursor}),
            );
        }
        if crate::approvals::expire_due(&mut store, session_id, &session)
            .map_err(|error| error.to_string())?
            > 0
            && let Some(container_id) = &container_id
        {
            runtime.signal(container_id, "SIGUSR1").await?;
        }
        for message in session
            .outbound_after(cursor)
            .map_err(|error| error.to_string())?
        {
            let Some(id) = message.id else { continue };
            cursor = id;
            crate::router::persist_tail_cursor(&mut store, session_id, cursor, events_last_seq)
                .map_err(|error| error.to_string())?;
            if let Some(response) = process_outbound_message(
                home,
                &config,
                &mut store,
                &session_dir,
                session_id,
                &message,
                None,
            )
            .await?
            {
                return Ok(response);
            }
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for approval result".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn process_outbound_message(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    session_dir: &Path,
    session_id: &str,
    message: &ms_proto::OutboundMessage,
    inbound_id: Option<i64>,
) -> Result<Option<Value>, String> {
    let pending = |prompt: String| {
        message
            .id
            .map(|id| json!({"session_id":session_id,"pending":true,"prompt":prompt,"after_id":id}))
    };
    Ok(match message.kind {
        OutboundKind::Reply if inbound_id.is_none_or(|id| message.in_reply_to == Some(id)) => {
            let active_at = utc_now();
            store
                .connection()
                .execute(
                    "UPDATE sessions SET status='ready', last_active_at=?2 WHERE id=?1",
                    [session_id, active_at.as_str()],
                )
                .map_err(|error| error.to_string())?;
            Some(
                json!({"session_id":session_id,"text":message.body.get("text").and_then(Value::as_str).unwrap_or("")}),
            )
        }
        OutboundKind::ApprovalRequest
            if inbound_id.is_none_or(|id| message.in_reply_to == Some(id)) =>
        {
            let binding = serde_json::from_value::<ms_proto::ApprovalBinding>(
                message.body["binding"].clone(),
            )
            .map_err(|e| e.to_string())?;
            let args = message.body.get("args").cloned().unwrap_or(Value::Null);
            let approval = crate::approvals::request_tool_approval(
                store,
                session_id,
                session_id,
                &binding,
                &args,
                Duration::from_secs(config.microswarm.limits.approval_timeout_s),
            )
            .map_err(|error| error.to_string())?;
            pending(crate::approvals::render_prompt(
                config.microswarm.home.lang,
                &approval.code,
                &binding.server,
                &binding.tool,
                &args,
            ))
        }
        OutboundKind::DelegationRequest => {
            handle_delegation_request(home, config, store, session_dir, session_id, &message.body)
                .await?
                .and_then(pending)
        }
        OutboundKind::SendToMember => {
            handle_send_to_member(home, config, store, session_dir, session_id, &message.body)
                .await?
                .and_then(pending)
        }
        OutboundKind::Dismiss => {
            handle_dismiss_member(home, config, store, session_dir, session_id, &message.body)
                .await?;
            None
        }
        _ => None,
    })
}

async fn route_waiting_member_replies(
    home: &Path,
    config: &ConfigSet,
    store: &mut SupervisorStore,
    lead_id: &str,
) -> Result<Option<String>, String> {
    let waiting = {
        let mut statement=store.connection().prepare("SELECT id,member_id FROM delegations WHERE lead_id=?1 AND status='waiting_reply' AND member_id IS NOT NULL").map_err(|e|e.to_string())?;
        statement
            .query_map([lead_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut routed = false;
    for (delegation_id, member_id) in waiting {
        let role: String = store
            .connection()
            .query_row(
                "SELECT role FROM sessions WHERE id=?1",
                [&member_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let member =
            open_session_dir(home.join("sessions").join(&member_id)).map_err(|e| e.to_string())?;
        let task_id: i64 = member
            .inbound()
            .query_row(
                "SELECT id FROM inbound WHERE kind='task' ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let (mut cursor, events_seq) =
            crate::router::tail_cursor(store, &member_id).map_err(|e| e.to_string())?;
        for reply in member.outbound_after(cursor).map_err(|e| e.to_string())? {
            let Some(id) = reply.id else { continue };
            cursor = id;
            crate::router::persist_tail_cursor(store, &member_id, cursor, events_seq)
                .map_err(|e| e.to_string())?;
            if reply.in_reply_to.is_none_or(|inbound| inbound < task_id) {
                continue;
            }
            if reply.kind == OutboundKind::ApprovalRequest {
                let binding: ms_proto::ApprovalBinding =
                    serde_json::from_value(reply.body["binding"].clone())
                        .map_err(|e| e.to_string())?;
                let args = reply.body.get("args").cloned().unwrap_or(Value::Null);
                let approval = crate::approvals::request_tool_approval(
                    store,
                    &member_id,
                    lead_id,
                    &binding,
                    &args,
                    Duration::from_secs(config.microswarm.limits.approval_timeout_s),
                )
                .map_err(|e| e.to_string())?;
                let prompt = crate::approvals::render_prompt(
                    config.microswarm.home.lang,
                    &approval.code,
                    &binding.server,
                    &binding.tool,
                    &args,
                );
                return Ok(Some(format!("[{role} {member_id}] {prompt}")));
            }
            if reply.kind != OutboundKind::Reply {
                continue;
            }
            let text = reply.body.get("text").and_then(Value::as_str).unwrap_or("");
            let lead =
                open_session_dir(home.join("sessions").join(lead_id)).map_err(|e| e.to_string())?;
            let spawned: bool = lead.inbound().query_row(
                "SELECT EXISTS(SELECT 1 FROM inbound WHERE kind='delegation_result' AND json_extract(body,'$.delegation_id')=?1 AND json_extract(body,'$.status')='spawned')",
                [&delegation_id], |row| row.get(0),
            ).map_err(|e| e.to_string())?;
            if spawned {
                lead.append_inbound(
                    &InboundKind::MemberReply,
                    &json!({"member_id":member_id,"delegation_id":delegation_id,"text":text}),
                )
                .map_err(|e| e.to_string())?;
            } else {
                let scope: String = store
                    .connection()
                    .query_row(
                        "SELECT scope_json FROM sessions WHERE id=?1",
                        [&member_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let granted: Value = serde_json::from_str(&scope).map_err(|e| e.to_string())?;
                lead.append_inbound(&InboundKind::DelegationResult,
                    &json!({"member_id":member_id,"delegation_id":delegation_id,"status":"spawned","initial_reply":text,"narrowed":granted["narrowed"],"granted":granted}))
                    .map_err(|e| e.to_string())?;
            }
            store.connection().execute("UPDATE delegations SET status='active',waiting_since=NULL,deadline_at=NULL,updated_at=?2 WHERE id=?1 AND status='waiting_reply'",[&delegation_id,utc_now().as_str()]).map_err(|e|e.to_string())?;
            store.connection().execute("UPDATE sessions SET status='ready',last_active_at=?2 WHERE id=?1 AND status='busy'",[&member_id,utc_now().as_str()]).map_err(|e|e.to_string())?;
            emit_supervisor_event(
                store,
                &member_id,
                lead_id,
                "member",
                SupervisorEventType::DelegationReply,
                json!({"member_id":member_id,"delegation_id":delegation_id}),
            )?;
            routed = true;
            break;
        }
    }
    if routed {
        let container = store
            .session_container_id(lead_id)
            .map_err(|e| e.to_string())?;
        if let Some(container) = container {
            runtime_for(config)?.signal(&container, "SIGUSR1").await?;
        }
    }
    Ok(None)
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
