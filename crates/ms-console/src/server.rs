use crate::{
    assets,
    auth::ConsoleAuth,
    chat::ChatManager,
    events::{ConsoleEvent, EventRing},
    rpc,
};
use futures_util::stream;
use http::{HeaderMap, HeaderValue, Request, Response, StatusCode, header};
use http_body_util::{BodyExt, Full, StreamBody, combinators::BoxBody};
use hyper::{
    body::{Bytes, Frame, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use std::{
    collections::VecDeque,
    convert::Infallible,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, broadcast},
    time::Instant,
};

pub fn security_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers
}

pub fn validate_listen(listen: SocketAddr) -> Result<(), String> {
    if listen.ip().is_loopback() {
        Ok(())
    } else {
        Err("The console only listens on loopback. Use an SSH tunnel (ssh -L 8790:127.0.0.1:8790 host) or a TLS proxy.".into())
    }
}

pub fn session_cookie(id: &str) -> String {
    format!("ms_console={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200")
}

type WebBody = BoxBody<Bytes, Infallible>;
type WebResponse = Response<WebBody>;

pub const SSE_PING_INTERVAL: Duration = Duration::from_secs(15);

pub async fn run(home: PathBuf, listen: SocketAddr) -> Result<(), String> {
    validate_listen(listen)?;
    let listener = TcpListener::bind(listen).await.map_err(|e| e.to_string())?;
    let listen = listener.local_addr().map_err(|e| e.to_string())?;
    let auth = Arc::new(Mutex::new(ConsoleAuth::new()?));
    let ring = Arc::new(Mutex::new(EventRing::new(2_000)));
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    tokio::spawn(rpc::follow(home.clone(), ring.clone()));
    let token = auth.lock().await.token().to_owned();
    eprintln!("MicroSwarm console: http://{}/login?token={token}", listen);
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let auth = auth.clone();
        let home = home.clone();
        let ring = ring.clone();
        let chats = chats.clone();
        let host = listen.to_string();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |request| {
                let auth = auth.clone();
                let home = home.clone();
                let host = host.clone();
                let ring = ring.clone();
                let chats = chats.clone();
                async move {
                    Ok::<_, Infallible>(
                        handle_request(request, auth, home, host, ring, chats).await,
                    )
                }
            });
            let _ = http1::Builder::new().serve_connection(io, service).await;
        });
    }
}

async fn handle_request(
    request: Request<Incoming>,
    auth: Arc<Mutex<ConsoleAuth>>,
    home: PathBuf,
    allowed_host: String,
    ring: Arc<Mutex<EventRing>>,
    chats: Arc<Mutex<ChatManager>>,
) -> WebResponse {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let api_path = path.starts_with("/api/");
    let query = request.uri().query().unwrap_or("").to_owned();
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let console_header = request
        .headers()
        .get("x-ms-console")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(cookie_id)
        .map(str::to_owned);
    let last_event_id = request
        .headers()
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let now = SystemTime::now();
    let (mut response, set_cookie) = if host != allowed_host
        && host
            != format!(
                "localhost:{}",
                allowed_host.rsplit(':').next().unwrap_or("8790")
            ) {
        (
            json_response(StatusCode::MISDIRECTED_REQUEST, "host_not_allowed"),
            None,
        )
    } else if method == http::Method::GET && path == "/login" {
        let candidate = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("token="));
        let candidate = candidate.map(url_decode);
        let consumed = match candidate {
            Some(value) => auth.lock().await.consume_token(&value, now),
            None => None,
        };
        match consumed {
            Some(id) => {
                let mut response = empty_response(StatusCode::SEE_OTHER);
                response
                    .headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_static("/"));
                (response, Some(session_cookie(&id)))
            }
            None => (
                text_response(
                    StatusCode::UNAUTHORIZED,
                    "Run microswarm console again for a new link.",
                ),
                None,
            ),
        }
    } else {
        let authenticated = match cookie.as_deref() {
            Some(id) => auth.lock().await.has_session(id, now),
            None => false,
        };
        if !authenticated {
            let response = if api_path {
                json_response(StatusCode::UNAUTHORIZED, "login_required")
            } else {
                text_response(
                    StatusCode::UNAUTHORIZED,
                    "Run microswarm console again for a new link.",
                )
            };
            (response, None)
        } else if method == http::Method::POST {
            if origin.as_deref() != Some(&format!("http://{host}")) {
                (
                    json_response(StatusCode::FORBIDDEN, "origin_mismatch"),
                    None,
                )
            } else if console_header.as_deref() != Some("1") {
                (
                    json_response(StatusCode::FORBIDDEN, "missing_console_header"),
                    None,
                )
            } else if !content_type.as_deref().is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .eq_ignore_ascii_case("application/json")
            }) {
                (
                    json_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type"),
                    None,
                )
            } else {
                route_authenticated(
                    method,
                    path,
                    request,
                    RequestContext {
                        auth,
                        cookie,
                        home,
                        ring,
                        last_event_id,
                        query,
                        chats,
                    },
                )
                .await
            }
        } else {
            route_authenticated(
                method,
                path,
                request,
                RequestContext {
                    auth,
                    cookie,
                    home,
                    ring,
                    last_event_id,
                    query,
                    chats,
                },
            )
            .await
        }
    };
    for (name, value) in security_headers().iter() {
        response.headers_mut().insert(name.clone(), value.clone());
    }
    if api_path {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    if let Some(value) = set_cookie {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, HeaderValue::from_str(&value).unwrap());
    }
    response
}

struct RequestContext {
    auth: Arc<Mutex<ConsoleAuth>>,
    cookie: Option<String>,
    home: PathBuf,
    ring: Arc<Mutex<EventRing>>,
    last_event_id: Option<u64>,
    query: String,
    chats: Arc<Mutex<ChatManager>>,
}

fn session_view_data(
    home: &Path,
    id: &str,
) -> Result<(ms_runtime::trace::TraceData, Vec<serde_json::Value>), (StatusCode, &'static str)> {
    if !crate::read::valid_session_id(id) {
        return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
    }
    let trace = ms_runtime::trace::build(home, id, None).map_err(|e| read_error(&e))?;
    let agents = crate::views::agents(&trace);
    Ok((trace, agents))
}

fn parse_query_i64(query: &str, key: &str) -> Result<Option<i64>, (StatusCode, &'static str)> {
    query_value(query, key)
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| (StatusCode::BAD_REQUEST, "invalid_query"))
        })
        .transpose()
}
fn parse_query_usize(query: &str, key: &str) -> Result<Option<usize>, (StatusCode, &'static str)> {
    query_value(query, key)
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| (StatusCode::BAD_REQUEST, "invalid_query"))
        })
        .transpose()
}

type ViewCache = std::collections::VecDeque<(String, String, i64, String, serde_json::Value)>;
static VIEW_CACHE: std::sync::OnceLock<std::sync::Mutex<ViewCache>> = std::sync::OnceLock::new();
fn memo_view(
    home: &Path,
    trace: &ms_runtime::trace::TraceData,
    name: String,
    build: impl FnOnce() -> serde_json::Value,
) -> serde_json::Value {
    let root = trace
        .sessions
        .first()
        .map(|session| session.root.as_str())
        .unwrap_or_default()
        .to_owned();
    let newest = trace.events.len() as i64;
    let home = home.to_string_lossy().into_owned();
    let cache = VIEW_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
    if let Ok(mut entries) = cache.lock()
        && let Some(index) = entries.iter().position(|entry| {
            entry.0 == home && entry.1 == root && entry.2 == newest && entry.3 == name
        })
    {
        let entry = entries.remove(index).unwrap();
        let value = entry.4.clone();
        entries.push_front(entry);
        return value;
    }
    let value = build();
    if let Ok(mut entries) = cache.lock() {
        entries.push_front((home, root, newest, name, value.clone()));
        // Bound response objects, including query variants and old generations.
        entries.truncate(32);
    }
    value
}

fn api_get(
    home: &Path,
    path: &str,
    query: &str,
) -> Result<serde_json::Value, (StatusCode, &'static str)> {
    if path == "/api/v1/home" {
        let mut sessions =
            crate::read::sessions(home, "active").map_err(|e| read_error(&e))?["sessions"]
                .as_array()
                .cloned()
                .unwrap_or_default();
        sessions.extend(
            crate::read::sessions(home, "ended").map_err(|e| read_error(&e))?["sessions"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        for session in &mut sessions {
            if let Some(id) = session["id"].as_str()
                && let Ok((trace, agents)) = session_view_data(home, id)
            {
                session["agents"] = serde_json::json!(agents);
                let steps = crate::views::steps::derive(
                    &trace.events,
                    &agents,
                    None,
                    None,
                    200,
                    "everything",
                )["steps"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                session["latest"]=steps.last().map(|step|serde_json::json!({"who":step["agent"]["role"],"text_or_sentence":step["sentence"]})).unwrap_or_else(||serde_json::json!({"who":"lead","text_or_sentence":null}));
                session["last_line"] = session["latest"]["text_or_sentence"].clone();
            }
        }
        let approvals =
            crate::read::approvals(home, "pending").map_err(|e| read_error(&e))?["approvals"]
                .as_array()
                .cloned()
                .unwrap_or_default();
        return Ok(crate::views::home::derive(&sessions, &approvals, None));
    }
    if path == "/api/v1/overview" {
        let pending = crate::read::pending_approvals(home).unwrap_or(0);
        return Ok(
            serde_json::json!({"supervisor":"running","runtime":"unknown","containers":0,"pending_approvals":pending,"lang":"en","console_version":"0.2.0"}),
        );
    }
    if path == "/api/v1/sessions" {
        let group = query_value(query, "group").unwrap_or_else(|| "active".into());
        return crate::read::sessions(home, &group).map_err(|e| read_error(&e));
    }
    if path == "/api/v1/approvals" {
        let state = query_value(query, "state").unwrap_or_else(|| "pending".into());
        return crate::read::approvals(home, &state).map_err(|e| read_error(&e));
    }
    if path == "/api/v1/host" {
        return Ok(crate::read::host(home));
    }
    let parts = path
        .trim_start_matches("/api/v1/")
        .split('/')
        .collect::<Vec<_>>();
    match parts.as_slice() {
        ["sessions", id, "team"] => {
            let (trace, agents) = session_view_data(home, id)?;
            Ok(memo_view(home, &trace, "team".into(), || {
                let steps = crate::views::steps::derive(
                    &trace.events,
                    &agents,
                    None,
                    None,
                    200,
                    "everything",
                )["steps"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut team = crate::views::team::derive(&trace.events, &agents, &steps);
                if let Some(groups) = team["groups"].as_array_mut() {
                    for member in groups
                        .iter_mut()
                        .flat_map(|group| group["members"].as_array_mut().into_iter().flatten())
                    {
                        if let Some(member_id) = member["agent"]["id"].as_str()
                            && let Ok(scope) = crate::read::scope(home, member_id, true)
                        {
                            member["reach_summary"] =
                                crate::views::reach::summary(&scope["granted"]);
                        }
                    }
                }
                team
            }))
        }
        ["sessions", id, "steps"] => {
            let (trace, agents) = session_view_data(home, id)?;
            let before = parse_query_i64(query, "before_seq")?;
            let after = parse_query_i64(query, "after_seq")?;
            if before.is_some() && after.is_some() {
                return Err((StatusCode::BAD_REQUEST, "invalid_query"));
            }
            let limit = parse_query_usize(query, "limit")?.unwrap_or(200);
            if !(1..=200).contains(&limit) {
                return Err((StatusCode::BAD_REQUEST, "invalid_query"));
            }
            let filter = query_value(query, "filter").unwrap_or_else(|| "everything".into());
            if !matches!(filter.as_str(), "everything" | "decisions" | "problems") {
                return Err((StatusCode::BAD_REQUEST, "invalid_query"));
            }
            let cache_key = format!("steps:{query}");
            let agent_id = query_value(query, "agent");
            if let Some(agent) = agent_id.as_deref() {
                if !crate::read::valid_session_id(agent) {
                    return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
                }
                if !agents.iter().any(|item| item["id"] == agent) {
                    return Err((StatusCode::NOT_FOUND, "not_found"));
                }
            }
            Ok(memo_view(home, &trace, cache_key, || {
                let mut result = crate::views::steps::derive_for_agent(
                    &trace.events,
                    &agents,
                    before,
                    after,
                    limit,
                    &filter,
                    agent_id.as_deref(),
                );
                let mut phases = crate::views::phases::derive(&trace.events, &agents);
                for phase in &mut phases {
                    phase["step_ids"] = serde_json::json!(
                        result["steps"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|step| step["phase_id"] == phase["phase_id"])
                            .map(|step| step["step_id"].clone())
                            .collect::<Vec<_>>()
                    );
                }
                result["phases"] = serde_json::json!(phases);
                result
            }))
        }
        ["sessions", id, "timeline"] => {
            let (trace, agents) = session_view_data(home, id)?;
            let since = query_value(query, "since");
            let cache_key = format!("timeline:{}", since.as_deref().unwrap_or(""));
            Ok(memo_view(home, &trace, cache_key, || {
                let steps = crate::views::steps::derive(
                    &trace.events,
                    &agents,
                    None,
                    None,
                    200,
                    "everything",
                )["steps"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                crate::views::timeline::derive(&trace.events, &agents, &steps, since.as_deref())
            }))
        }
        ["sessions", id, "reach"] => {
            if !crate::read::valid_session_id(id) {
                return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
            }
            let view = crate::read::scope(home, id, false).map_err(|e| read_error(&e))?;
            crate::views::reach::derive(&view, true)
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "read_failed"))
        }
        ["members", id, "reach"] => {
            if !crate::read::valid_session_id(id) {
                return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
            }
            let view = crate::read::scope(home, id, true).map_err(|e| read_error(&e))?;
            crate::views::reach::derive(&view, false)
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "read_failed"))
        }
        ["members", id, "report"] => {
            crate::read::member_report(home, id).map_err(|e| read_error(&e))
        }
        ["sessions", id, "steps", step_id] => {
            let (trace, agents) = session_view_data(home, id)?;
            let steps = crate::views::steps::derive(
                &trace.events,
                &agents,
                None,
                None,
                trace.events.len(),
                "everything",
            )["steps"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let step = steps
                .iter()
                .find(|step| step["step_id"] == *step_id)
                .cloned()
                .ok_or((StatusCode::NOT_FOUND, "not_found"))?;
            let seqs = step["event_seqs"].as_array().cloned().unwrap_or_default();
            let events = trace
                .events
                .iter()
                .filter(|event| seqs.iter().any(|seq| *seq == event["seq"]))
                .cloned()
                .collect::<Vec<_>>();
            Ok(
                serde_json::json!({"step":step,"explanation":{"decision":step["decision"],"facts":[]},"events":events}),
            )
        }
        ["sessions", id] => crate::read::session(home, id).map_err(|e| read_error(&e)),
        ["sessions", id, "scope"] => {
            crate::read::scope(home, id, false).map_err(|e| read_error(&e))
        }
        ["sessions", id, "activity"] => {
            crate::read::recent_activity(home, id).map_err(|e| read_error(&e))
        }
        ["sessions", id, "messages"] => {
            if !crate::read::valid_session_id(id) {
                return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
            }
            let limit = query_value(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(100);
            let before = query_value(query, "before");
            crate::read::messages(home, id, before.as_deref(), limit).map_err(|e| read_error(&e))
        }
        ["sessions", id, "trace"] => crate::read::trace(home, id).map_err(|e| read_error(&e)),
        ["approvals", id] => crate::read::approval(home, id).map_err(|e| read_error(&e)),
        ["members", id, "scope"] => crate::read::scope(home, id, true).map_err(|e| read_error(&e)),
        _ => Err((StatusCode::NOT_FOUND, "not_found")),
    }
}

pub async fn api_post(
    home: &Path,
    path: &str,
    payload: serde_json::Value,
    chats: Arc<Mutex<ChatManager>>,
    ring: Arc<Mutex<EventRing>>,
) -> Result<(StatusCode, Option<serde_json::Value>), (StatusCode, &'static str)> {
    if path == "/api/v1/sessions" {
        let text = message_text(&payload)?;
        let thread = ms_runtime::new_id()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "request_id_unavailable"))?;
        let response = crate::chat::submit(home.to_path_buf(), None, thread, text, chats, ring)
            .await
            .map_err(|e| {
                if e == "session_busy" {
                    (StatusCode::CONFLICT, "session_busy")
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, e)
                }
            })?;
        return Ok((StatusCode::ACCEPTED, Some(response)));
    }
    let parts = path
        .trim_start_matches("/api/v1/")
        .split('/')
        .collect::<Vec<_>>();
    match parts.as_slice() {
        ["sessions", id, "messages"] => {
            if !crate::read::valid_session_id(id) {
                return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
            }
            let (kind, status) = crate::read::chat_state(home, id).map_err(|e| read_error(&e))?;
            if kind != "lead" {
                return Err((StatusCode::BAD_REQUEST, "lead_session_required"));
            }
            if matches!(status.as_str(), "closed" | "failed" | "stopping") {
                return Err((StatusCode::CONFLICT, "session_closed"));
            }
            let text = message_text(&payload)?;
            let thread = ms_runtime::new_id()
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "request_id_unavailable"))?;
            let response = crate::chat::submit(
                home.to_path_buf(),
                Some((*id).into()),
                thread,
                text,
                chats,
                ring,
            )
            .await
            .map_err(|e| {
                if e == "session_busy" {
                    (StatusCode::CONFLICT, "session_busy")
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, e)
                }
            })?;
            Ok((
                StatusCode::ACCEPTED,
                Some(serde_json::json!({"request_id":response["request_id"]})),
            ))
        }
        ["sessions", id, "end"] => {
            if !crate::read::valid_session_id(id) {
                return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
            }
            let (kind, status) = crate::read::chat_state(home, id).map_err(|e| read_error(&e))?;
            if kind != "lead" {
                return Err((StatusCode::CONFLICT, "lead_session_required"));
            }
            if matches!(status.as_str(), "closed" | "failed" | "stopping") {
                return Err((StatusCode::CONFLICT, "session_closed"));
            }
            let request_id = ms_runtime::new_id()
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "request_id_unavailable"))?;
            let home = home.to_path_buf();
            let session = (*id).to_owned();
            let ring = ring.clone();
            let request = request_id.clone();
            tokio::spawn(async move {
                if rpc::request(
                    &home,
                    &serde_json::json!({"op":"end_session","session_id":session}),
                )
                .await
                .is_err()
                {
                    let event = ConsoleEvent {
                        id: 0,
                        name: "chat.error".into(),
                        data: serde_json::json!({"type":"chat.error","at":ms_runtime::utc_now(),"request_id":request,"session_id":session,"error":"supervisor_error","text":"The session could not be ended."}),
                    };
                    ring.lock().await.push(event);
                }
            });
            Ok((
                StatusCode::ACCEPTED,
                Some(serde_json::json!({"request_id":request_id})),
            ))
        }
        ["approvals", code, action] if matches!(*action, "approve" | "deny") => {
            if code.len() != 4
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return Err((StatusCode::BAD_REQUEST, "invalid_approval_code"));
            }
            let op = if *action == "approve" {
                "approve"
            } else {
                "deny"
            };
            match rpc::request(home, &serde_json::json!({"op":op,"code":code})).await {
                Ok(result) if result.get("error").is_none() => Ok((
                    StatusCode::OK,
                    Some(serde_json::json!({"decision":result["decision"]})),
                )),
                Ok(result) => Err(approval_error(result["error"].as_str().unwrap_or_default())),
                Err(_) => Err((StatusCode::SERVICE_UNAVAILABLE, "supervisor_down")),
            }
        }
        ["credentials", name] => {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                return Err((StatusCode::BAD_REQUEST, "invalid_credential_name"));
            }
            let value = payload
                .get("value")
                .and_then(serde_json::Value::as_str)
                .ok_or((StatusCode::BAD_REQUEST, "invalid_credential_value"))?;
            match rpc::request(
                home,
                &serde_json::json!({"op":"creds_set","name":name,"value":value}),
            )
            .await
            {
                Ok(result) if result.get("error").is_none() => Ok((StatusCode::NO_CONTENT, None)),
                _ => Err((StatusCode::SERVICE_UNAVAILABLE, "credential_store_failed")),
            }
        }
        ["skills", "install"] => {
            let path = payload
                .get("path")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or((StatusCode::BAD_REQUEST, "invalid_skill_path"))?;
            match rpc::request(
                home,
                &serde_json::json!({"op":"skills_install","path":path}),
            )
            .await
            {
                Ok(result) if result.get("error").is_none() => Ok((StatusCode::OK, Some(result))),
                _ => Err((StatusCode::SERVICE_UNAVAILABLE, "skill_install_failed")),
            }
        }
        _ => Err((StatusCode::NOT_FOUND, "not_found")),
    }
}

fn message_text(payload: &serde_json::Value) -> Result<String, (StatusCode, &'static str)> {
    let text = payload
        .get("text")
        .and_then(serde_json::Value::as_str)
        .ok_or((StatusCode::BAD_REQUEST, "invalid_message"))?;
    if text.trim().is_empty() || text.chars().count() > 8_000 {
        return Err((StatusCode::BAD_REQUEST, "invalid_message"));
    }
    Ok(text.to_owned())
}

fn approval_error(error: &str) -> (StatusCode, &'static str) {
    if error.contains("already resolved") {
        (StatusCode::CONFLICT, "already_resolved")
    } else if error.contains("expired") || error.contains("timed out") {
        (StatusCode::GONE, "expired")
    } else if error.contains("not found") {
        (StatusCode::NOT_FOUND, "not_found")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "approval_failed")
    }
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name != key {
            return None;
        }
        let mut bytes = Vec::with_capacity(value.len());
        let raw = value.as_bytes();
        let mut index = 0;
        while index < raw.len() {
            match raw[index] {
                b'+' => {
                    bytes.push(b' ');
                    index += 1;
                }
                b'%' if index + 2 < raw.len() => {
                    let hex = std::str::from_utf8(&raw[index + 1..index + 3]).ok()?;
                    bytes.push(u8::from_str_radix(hex, 16).ok()?);
                    index += 3;
                }
                b'%' => return None,
                byte => {
                    bytes.push(byte);
                    index += 1;
                }
            }
        }
        String::from_utf8(bytes).ok()
    })
}
fn read_error(error: &str) -> (StatusCode, &'static str) {
    if error.contains("invalid session id") || error.contains("member session required") {
        (StatusCode::BAD_REQUEST, "invalid_session_id")
    } else if error.contains("not found") {
        (StatusCode::NOT_FOUND, "not_found")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "read_failed")
    }
}

async fn route_authenticated(
    method: http::Method,
    path: String,
    request: Request<Incoming>,
    context: RequestContext,
) -> (WebResponse, Option<String>) {
    let RequestContext {
        auth,
        cookie,
        home,
        ring,
        last_event_id,
        query,
        chats,
    } = context;
    if method == http::Method::POST {
        let mut body = request.into_body();
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = match frame {
                Ok(frame) => frame,
                Err(_) => return (json_response(StatusCode::BAD_REQUEST, "invalid_body"), None),
            };
            if let Ok(data) = frame.into_data() {
                bytes.extend_from_slice(&data);
                if bytes.len() > 65_536 {
                    return (
                        json_response(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
                        None,
                    );
                }
            }
        }
        if path == "/api/v1/logout" {
            if let Some(id) = cookie {
                auth.lock().await.logout(&id);
            }
            return (
                empty_response(StatusCode::NO_CONTENT),
                Some("ms_console=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0".into()),
            );
        }
        let payload: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(payload) => payload,
            Err(_) => return (json_response(StatusCode::BAD_REQUEST, "invalid_json"), None),
        };
        return match api_post(&home, &path, payload, chats, ring).await {
            Ok((status, Some(body))) => (
                body_response(
                    status,
                    "application/json; charset=utf-8",
                    serde_json::to_vec(&body).unwrap_or_default(),
                ),
                None,
            ),
            Ok((status, None)) => (empty_response(status), None),
            Err((status, code)) => (json_response(status, code), None),
        };
    }
    if method != http::Method::GET {
        return (
            json_response(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            None,
        );
    }
    if path == "/api/v1/events" {
        return (sse_response(ring, last_event_id).await, None);
    }
    if path.starts_with("/api/v1/") {
        return match api_get(&home, &path, &query) {
            Ok(value) => (
                body_response(
                    StatusCode::OK,
                    "application/json; charset=utf-8",
                    serde_json::to_vec(&value).unwrap_or_default(),
                ),
                None,
            ),
            Err((status, code)) => (json_response(status, code), None),
        };
    }
    if let Some(asset) = assets::get(&path) {
        let mut response = body_response(StatusCode::OK, asset.content_type, asset.bytes.to_vec());
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        response.headers_mut().insert(
            header::ETAG,
            HeaderValue::from_str(&format!("\"console-{}\"", path.replace('/', "_"))).unwrap(),
        );
        return (response, None);
    }
    (json_response(StatusCode::NOT_FOUND, "not_found"), None)
}

fn cookie_id(header: &str) -> Option<&str> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == "ms_console").then_some(value))
}
fn url_decode(value: &str) -> String {
    value
        .replace("%2F", "/")
        .replace("%2f", "/")
        .replace("%3D", "=")
        .replace("%3d", "=")
        .replace('+', " ")
}
fn body_response(status: StatusCode, content_type: &str, body: Vec<u8>) -> WebResponse {
    let mut response = Response::new(
        Full::new(Bytes::from(body))
            .map_err(|never| match never {})
            .boxed(),
    );
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).unwrap(),
    );
    response
}
fn json_response(status: StatusCode, error: &str) -> WebResponse {
    let message = match error {
        "host_not_allowed" => "This request host is not allowed.",
        "login_required" => "Open the one-time console link to sign in.",
        "origin_mismatch" => "The request origin does not match this console.",
        "missing_console_header" => "The console request header is missing.",
        "unsupported_media_type" => "Send this request as application/json.",
        "body_too_large" => "The request body exceeds 64 KiB.",
        "invalid_body" => "The request body could not be read.",
        "not_found" => "The requested console route was not found.",
        "method_not_allowed" => "This method is not available for the route.",
        _ => "The console request could not be completed.",
    };
    let body = format!(
        "{{\"error\":{},\"message\":{}}}",
        serde_json::to_string(error).unwrap(),
        serde_json::to_string(message).unwrap()
    );
    body_response(status, "application/json; charset=utf-8", body.into_bytes())
}
fn text_response(status: StatusCode, text: &str) -> WebResponse {
    body_response(
        status,
        "text/plain; charset=utf-8",
        text.as_bytes().to_vec(),
    )
}
fn empty_response(status: StatusCode) -> WebResponse {
    let mut response = Response::new(
        Full::new(Bytes::new())
            .map_err(|never| match never {})
            .boxed(),
    );
    *response.status_mut() = status;
    response
}

struct SseState {
    receiver: broadcast::Receiver<ConsoleEvent>,
    replay: VecDeque<ConsoleEvent>,
    resync: bool,
    last: u64,
    ping: tokio::time::Interval,
    ring: Arc<Mutex<EventRing>>,
}

async fn sse_response(ring: Arc<Mutex<EventRing>>, after: Option<u64>) -> WebResponse {
    let receiver = ring.lock().await.subscribe();
    let replay = ring.lock().await.replay(after);
    let last = after
        .unwrap_or(0)
        .max(replay.events.last().map(|event| event.id).unwrap_or(0));
    let interval = tokio::time::interval_at(Instant::now() + SSE_PING_INTERVAL, SSE_PING_INTERVAL);
    let state = SseState {
        receiver,
        replay: replay.events.into(),
        resync: replay.resync,
        last,
        ping: interval,
        ring,
    };
    let output = stream::unfold(state, |mut state| async move {
        if state.resync {
            state.resync = false;
            return Some((
                Ok(Frame::data(Bytes::from_static(
                    b"event: resync\ndata: {\"type\":\"resync\"}\n\n",
                ))),
                state,
            ));
        }
        if let Some(event) = state.replay.pop_front() {
            state.last = event.id;
            return Some((
                Ok(Frame::data(Bytes::from(encode_sse_event(&event)))),
                state,
            ));
        }
        tokio::select! {
            received = state.receiver.recv() => match received {
                Ok(event) if event.id > state.last => {
                    state.last = event.id;
                    Some((Ok(Frame::data(Bytes::from(encode_sse_event(&event)))), state))
                }
                Ok(_) => Some((Ok(Frame::data(Bytes::new())), state)),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let replay = state.ring.lock().await.replay(Some(state.last));
                    if replay.resync { Some((Ok(Frame::data(Bytes::from_static(b"event: resync\ndata: {\"type\":\"resync\"}\n\n"))), state)) }
                    else { state.replay = replay.events.into(); Some((Ok(Frame::data(Bytes::new())), state)) }
                }
                Err(broadcast::error::RecvError::Closed) => None,
            },
            _ = state.ping.tick() => Some((Ok(Frame::data(Bytes::from_static(b": ping\n\n"))), state)),
        }
    });
    let mut response = Response::new(StreamBody::new(output).boxed());
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

pub fn encode_sse_event(event: &ConsoleEvent) -> String {
    let mut data = event.data.to_string();
    data.retain(|character| character != '\n' && character != '\r');
    format!(
        "id: {}\nevent: {}\ndata: {}\n\n",
        event.id, event.name, data
    )
}

#[cfg(test)]
mod route_tests {
    use super::*;
    #[test]
    fn cache_bounds_single_session_variants() {
        let home = Path::new("/cache-bound-regression");
        let mut trace = ms_runtime::trace::TraceData {
            sessions: vec![],
            events: vec![],
        };
        for generation in 0..100 {
            trace.events.push(serde_json::json!({"seq":generation}));
            memo_view(home, &trace, format!("timeline:{generation}"), || {
                serde_json::json!(generation)
            });
        }
        let entries = VIEW_CACHE.get().unwrap().lock().unwrap();
        assert!(entries.len() <= 32);
    }

    #[test]
    fn new_routes_reject_bad_ids_before_filesystem_reads() {
        let home = std::path::Path::new("/no-such-ms-home");
        for path in [
            "/api/v1/sessions/../team",
            "/api/v1/sessions/01ARZ3NDEKTSV4RRFFQ69G5FAVX/team",
            "/api/v1/sessions/01arz3ndektsv4rrffq69g5fav/team",
            "/api/v1/members/lowercase/reach",
        ] {
            assert_eq!(
                api_get(home, path, "").unwrap_err(),
                (StatusCode::BAD_REQUEST, "invalid_session_id")
            );
        }
    }
    #[test]
    fn steps_query_validation_rejects_bad_limits_and_filters() {
        assert_eq!(parse_query_usize("limit=0", "limit").unwrap(), Some(0));
        assert_eq!(
            parse_query_i64("after_seq=nope", "after_seq").unwrap_err(),
            (StatusCode::BAD_REQUEST, "invalid_query")
        );
    }

    #[test]
    fn new_session_routes_return_contract_shapes_and_unknown_is_404() {
        use rusqlite::Connection;
        let home = std::env::temp_dir().join(format!("ms-view-routes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let db = Connection::open(home.join("supervisor.db")).unwrap();
        db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,role TEXT,status TEXT,scope_json TEXT,created_at TEXT); CREATE TABLE supervisor_events(seq INTEGER,ts TEXT,ts_mono INTEGER,session_id TEXT,root_session_id TEXT,agent_role TEXT,type TEXT,data TEXT);").unwrap();
        db.execute(
            "INSERT INTO sessions VALUES(?1,?1,NULL,'lead','ready',NULL,'2026-11-19T00:00:00Z')",
            [id],
        )
        .unwrap();
        db.execute("INSERT INTO supervisor_events VALUES(1,'2026-11-19T00:00:00Z',1,?1,?1,'lead','session.created','{}')",[id]).unwrap();
        for seq in 2..=250 {
            db.execute("INSERT INTO supervisor_events VALUES(?1,'2026-11-19T00:00:00Z',?1,?2,?2,'lead','message.inbound','{}')", rusqlite::params![seq,id]).unwrap();
        }
        drop(db);
        let team = api_get(&home, &format!("/api/v1/sessions/{id}/team"), "").unwrap();
        assert!(team["groups"].is_array() && team["agents"].is_array());
        let steps = api_get(&home, &format!("/api/v1/sessions/{id}/steps"), "limit=20").unwrap();
        assert!(
            steps["steps"].is_array()
                && steps["phases"].is_array()
                && steps["last_seq"].is_number()
        );
        let older = api_get(
            &home,
            &format!("/api/v1/sessions/{id}/steps"),
            "before_seq=20&limit=10",
        )
        .unwrap();
        let old_id = older["steps"][0]["step_id"].as_str().unwrap();
        assert!(api_get(&home, &format!("/api/v1/sessions/{id}/steps/{old_id}"), "").is_ok());
        let timeline = api_get(&home, &format!("/api/v1/sessions/{id}/timeline"), "").unwrap();
        assert!(timeline["lanes"].is_array() && timeline["stats"].is_object());
        let missing = api_get(
            &home,
            "/api/v1/sessions/01ARZ3NDEKTSV4RRFFQ69G5FAW/team",
            "",
        )
        .unwrap_err();
        assert_eq!(missing, (StatusCode::NOT_FOUND, "not_found"));
        std::fs::remove_dir_all(home).unwrap();
    }
}
