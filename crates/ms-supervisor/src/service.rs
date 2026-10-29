use crate::{
    config::{ConfigSet, ToolTransport, load_config_dir},
    proxy::{CredentialProvider, ProxyRoutes, SessionProxy},
    runtime::{ContainerProfile, ContainerRuntime},
    scope::{build_lead_scope, install_lead_scope},
};
use ms_proto::{InboundKind, OutboundKind};
use ms_store::{
    SessionConnections, SessionLifecycleUpdate, SessionRecord, SupervisorStore, open_session_dir,
};
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

async fn stop_idle_lead(
    home: &Path,
    config: &ConfigSet,
    session_id: &str,
    container_id: &str,
) -> Result<(), String> {
    let runtime = runtime_for(config)?;
    let session = open_session_dir(home.join("sessions").join(session_id))
        .map_err(|error| error.to_string())?;
    session
        .append_inbound(&InboundKind::Control, &json!({"op":"shutdown"}))
        .map_err(|error| error.to_string())?;
    runtime.signal(container_id, "SIGUSR1").await?;

    let grace = Duration::from_millis(config.microswarm.limits.grace_ms);
    let deadline = Instant::now() + grace;
    while runtime.is_running(container_id).await? && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if runtime.is_running(container_id).await? {
        runtime.stop(container_id, grace).await?;
    }

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

async fn dispatch(home: &Path, request: &Value) -> Value {
    let result = match request.get("op").and_then(Value::as_str) {
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
    let last_id: i64 = session
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
        for message in session
            .outbound_after(last_id)
            .map_err(|error| error.to_string())?
        {
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

fn utc_now() -> String {
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

fn config_dir(home: &Path) -> PathBuf {
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
