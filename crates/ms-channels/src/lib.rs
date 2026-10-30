//! Shared MicroSwarm crate.
pub mod cli;
pub mod telegram;
pub mod webhook;

use ms_proto::InboundMessage;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelInbound {
    pub thread_id: String,
    pub message: InboundMessage,
}

/// Maps channel-native payloads to and from the supervisor's message contract.
pub trait ChannelAdapter {
    fn kind(&self) -> &'static str;
    fn parse_inbound(&self, payload: &Value) -> Result<Option<ChannelInbound>, String>;
    fn reply_payload(&self, thread_id: &str, text: &str) -> Value;
}

pub fn validate_config(
    kind: &str,
    credential: Option<&str>,
    api_base: Option<&str>,
    listen: Option<&str>,
) -> Result<(), String> {
    if !matches!(kind, "cli" | "telegram" | "webhook") {
        return Err(format!("unsupported channel kind {kind:?}"));
    }
    if credential.is_some_and(|name| !valid_credential_name(name)) {
        return Err("channel token must be a credential name, never a secret value".into());
    }
    match kind {
        "telegram" => {
            if credential.is_none() {
                return Err("telegram channel requires a credential name in token".into());
            }
            if let Some(base) = api_base {
                validate_url(base, "Telegram API base")?;
            }
        }
        "webhook" => {
            let address = listen
                .unwrap_or("127.0.0.1:8788")
                .parse::<std::net::SocketAddr>()
                .map_err(|error| format!("invalid webhook listen address: {error}"))?;
            if !address.ip().is_loopback() {
                return Err("webhook listener must bind to loopback; use a TLS-terminating reverse proxy for public access".into());
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn valid_credential_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && (name.as_bytes()[0].is_ascii_lowercase() || name.as_bytes()[0].is_ascii_digit())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
}

pub fn validate_url(value: &str, field: &str) -> Result<(), String> {
    let uri = value
        .parse::<http::Uri>()
        .map_err(|error| format!("invalid {field}: {error}"))?;
    let local_http = uri.scheme_str() == Some("http")
        && uri
            .authority()
            .is_some_and(|authority| matches!(authority.host(), "localhost" | "127.0.0.1"));
    if uri
        .authority()
        .is_some_and(|authority| !authority.as_str().contains('@'))
        && (uri.scheme_str() == Some("https") || local_http)
    {
        Ok(())
    } else {
        Err(format!(
            "{field} must use HTTPS (HTTP is allowed for loopback only)"
        ))
    }
}

#[derive(Clone, Debug)]
pub enum RuntimeChannel {
    Telegram {
        credential: String,
        api_base: String,
    },
    Webhook {
        listen: String,
        credential: Option<String>,
    },
}

pub fn spawn(home: std::path::PathBuf, socket: std::path::PathBuf, channels: Vec<RuntimeChannel>) {
    for channel in channels {
        let home = home.clone();
        let socket = socket.clone();
        match channel {
            RuntimeChannel::Telegram {
                credential,
                api_base,
            } => {
                tokio::spawn(telegram::run(home, socket, credential, api_base));
            }
            RuntimeChannel::Webhook { listen, credential } => {
                tokio::spawn(webhook::run(home, socket, listen, credential));
            }
        }
    }
}

pub fn spawn_from_config(home: std::path::PathBuf, socket: std::path::PathBuf, config: Value) {
    let channels = config
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|channel| {
            let kind = channel.get("kind")?.as_str()?;
            match kind {
                "telegram" => Some(RuntimeChannel::Telegram {
                    credential: channel.get("token")?.as_str()?.to_owned(),
                    api_base: channel
                        .get("api_base")
                        .and_then(Value::as_str)
                        .unwrap_or("https://api.telegram.org")
                        .to_owned(),
                }),
                "webhook" => Some(RuntimeChannel::Webhook {
                    listen: channel
                        .get("listen")
                        .and_then(Value::as_str)
                        .unwrap_or("127.0.0.1:8788")
                        .to_owned(),
                    credential: channel
                        .get("token")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }),
                _ => None,
            }
        })
        .collect();
    spawn(home, socket, channels);
}

pub(crate) async fn supervisor_rpc(
    socket_path: &std::path::Path,
    request: Value,
) -> Result<Value, String> {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixStream,
    };
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|_| "cannot connect to microswarmd".to_owned())?;
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    write
        .write_all(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|error| error.to_string())?;
    let response: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(error.to_owned());
    }
    Ok(response)
}
