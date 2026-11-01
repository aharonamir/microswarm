use crate::{
    assets,
    auth::ConsoleAuth,
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
        Err("console --listen must use a loopback address".into())
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
    let auth = Arc::new(Mutex::new(ConsoleAuth::new()?));
    let ring = Arc::new(Mutex::new(EventRing::new(2_000)));
    tokio::spawn(rpc::follow(home.clone(), ring.clone()));
    let token = auth.lock().await.token().to_owned();
    eprintln!(
        "MicroSwarm console: http://{}:{}/login?token={token}",
        listen.ip(),
        listen.port()
    );
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let auth = auth.clone();
        let home = home.clone();
        let ring = ring.clone();
        let host = format!("{}:{}", listen.ip(), listen.port());
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |request| {
                let auth = auth.clone();
                let home = home.clone();
                let host = host.clone();
                let ring = ring.clone();
                async move { Ok::<_, Infallible>(handle_request(request, auth, home, host, ring).await) }
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
}

fn api_get(
    home: &Path,
    path: &str,
    query: &str,
) -> Result<serde_json::Value, (StatusCode, &'static str)> {
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
        ["sessions", id] => crate::read::session(home, id).map_err(|e| read_error(&e)),
        ["sessions", id, "messages"] => {
            if !crate::read::valid_session_id(id) {
                return Err((StatusCode::BAD_REQUEST, "invalid_session_id"));
            }
            let limit = query_value(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(100);
            crate::read::messages(home, id, limit).map_err(|e| read_error(&e))
        }
        ["sessions", id, "trace"] => crate::read::trace(home, id).map_err(|e| read_error(&e)),
        ["approvals", id] => crate::read::approval(home, id).map_err(|e| read_error(&e)),
        _ => Err((StatusCode::NOT_FOUND, "not_found")),
    }
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.replace('+', " "))
    })
}
fn read_error(error: &str) -> (StatusCode, &'static str) {
    if error.contains("invalid session id") {
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
        return (json_response(StatusCode::NOT_FOUND, "not_found"), None);
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
