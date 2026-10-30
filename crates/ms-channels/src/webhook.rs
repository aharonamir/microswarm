use crate::{ChannelAdapter, ChannelInbound, supervisor_rpc};
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper_util::rt::TokioIo;
use ms_proto::{InboundKind, InboundMessage};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

#[derive(Clone, Copy, Debug, Default)]
pub struct WebhookAdapter;

impl WebhookAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn parse_inbound(&self, payload: &Value) -> Result<ChannelInbound, String> {
        let thread_id = payload
            .get("thread_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("webhook payload requires a non-empty thread_id")?;
        let text = payload
            .get("text")
            .and_then(Value::as_str)
            .ok_or("webhook payload requires a text string")?;
        Ok(ChannelInbound {
            thread_id: thread_id.to_owned(),
            message: InboundMessage {
                id: None,
                kind: InboundKind::UserMessage,
                body: json!({"text":text}),
                created_at: None,
            },
        })
    }

    pub fn reply_payload(&self, thread_id: &str, text: &str) -> Value {
        json!({"thread_id":thread_id,"text":text})
    }
}

impl ChannelAdapter for WebhookAdapter {
    fn kind(&self) -> &'static str {
        "webhook"
    }

    fn parse_inbound(&self, payload: &Value) -> Result<Option<ChannelInbound>, String> {
        WebhookAdapter::parse_inbound(self, payload).map(Some)
    }

    fn reply_payload(&self, thread_id: &str, text: &str) -> Value {
        WebhookAdapter::reply_payload(self, thread_id, text)
    }
}

pub async fn run(home: PathBuf, socket: PathBuf, listen: String, credential: Option<String>) {
    let address = match listen.parse::<std::net::SocketAddr>() {
        Ok(address) if address.ip().is_loopback() => address,
        _ => {
            eprintln!(
                "webhook channel requires a loopback listen address; use a TLS-terminating reverse proxy for public access"
            );
            return;
        }
    };
    let listener = match TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("webhook channel could not bind {address}: {error}");
            return;
        }
    };
    let pending = Arc::new(Mutex::new(HashMap::<String, (String, i64)>::new()));
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let home = home.clone();
                let socket = socket.clone();
                let credential = credential.clone();
                let pending = Arc::clone(&pending);
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(move |request| {
                        let home = home.clone();
                        let socket = socket.clone();
                        let credential = credential.clone();
                        let pending = Arc::clone(&pending);
                        async move {
                            Ok::<_, std::convert::Infallible>(
                                handle_request(
                                    &home,
                                    &socket,
                                    credential.as_deref(),
                                    &pending,
                                    request,
                                )
                                .await,
                            )
                        }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await
                    {
                        eprintln!("webhook connection ended: {error}");
                    }
                });
            }
            Err(error) => {
                eprintln!("webhook listener accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_request(
    home: &std::path::Path,
    socket: &std::path::Path,
    credential: Option<&str>,
    pending: &Mutex<HashMap<String, (String, i64)>>,
    request: Request<Incoming>,
) -> Response<Full<Bytes>> {
    if request.method() != Method::POST || request.uri().path() != "/messages" {
        return response(StatusCode::NOT_FOUND, json!({"error":"use POST /messages"}));
    }
    let token = match credential
        .map(|name| read_credential(home, name))
        .transpose()
    {
        Ok(token) => token,
        Err(_) => {
            return response(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"error":"webhook credential is not available"}),
            );
        }
    };
    if let Some(expected) = token.as_deref() {
        let actual = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !actual.is_some_and(|actual| constant_time_eq(actual.as_bytes(), expected.as_bytes())) {
            return response(StatusCode::UNAUTHORIZED, json!({"error":"unauthorized"}));
        }
    }
    let body = match Limited::new(request.into_body(), 1024 * 1024)
        .collect()
        .await
    {
        Ok(body) => body.to_bytes(),
        Err(_) => {
            return response(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"error":"request body too large"}),
            );
        }
    };
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return response(StatusCode::BAD_REQUEST, json!({"error":"invalid JSON"})),
    };
    let inbound = match WebhookAdapter::new().parse_inbound(&payload) {
        Ok(inbound) => inbound,
        Err(error) => return response(StatusCode::BAD_REQUEST, json!({"error":error})),
    };
    let text = inbound.message.body["text"].as_str().unwrap_or_default();
    match supervisor_rpc(socket, json!({"op":"channel_message","channel":"webhook","thread_id":inbound.thread_id,"text":text})).await {
        Ok(mut result) => {
            if result["pending"] == true {
                if let (Some(session), Some(after)) = (result["session_id"].as_str(), result["after_id"].as_i64()) {
                    pending.lock().expect("pending lock").insert(inbound.thread_id.clone(), (session.to_owned(), after));
                }
            } else if text.starts_with("/approve ") || text.starts_with("/deny ") {
                let wait = pending.lock().expect("pending lock").remove(&inbound.thread_id);
                if let Some((session_id, after_id)) = wait {
                    match supervisor_rpc(socket, json!({"op":"wait","session_id":session_id,"after_id":after_id})).await {
                        Ok(reply) => result = reply,
                        Err(error) => return response(StatusCode::SERVICE_UNAVAILABLE, json!({"error":error})),
                    }
                }
            }
            let reply = result["prompt"].as_str().or_else(|| result["text"].as_str()).unwrap_or("The message was processed.");
            let mut body = WebhookAdapter::new().reply_payload(&inbound.thread_id, reply);
            if let Some(pending) = result.get("pending") { body["pending"] = pending.clone(); }
            response(StatusCode::OK, body)
        }
        Err(error) => response(StatusCode::SERVICE_UNAVAILABLE, json!({"error":error})),
    }
}

fn response(status: StatusCode, body: Value) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .expect("webhook response is valid")
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let length = left.len().max(right.len());
    let mut difference = left.len() ^ right.len();
    for index in 0..length {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn read_credential(home: &std::path::Path, name: &str) -> Result<String, String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        return Err("invalid credential name".into());
    }
    let value = std::fs::read_to_string(home.join("credentials").join(name))
        .map_err(|_| format!("credential {name:?} is not available"))?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(format!("credential {name:?} is empty"));
    }
    Ok(value)
}
