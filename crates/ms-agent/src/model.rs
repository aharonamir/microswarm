use crate::agent_loop::{TurnOutput, TurnProcessor};
use crate::boot::AgentContext;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use ms_proto::{InboundKind, InboundMessage, OutboundKind, OutboundMessage};
use ms_store::HistoryEntry;
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelProvider {
    OpenAiCompatible,
    Anthropic,
}

impl ModelProvider {
    fn parse(value: &str) -> Result<Self, ModelError> {
        match value {
            "openai-compatible" => Ok(Self::OpenAiCompatible),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(ModelError(format!("unsupported model provider {value:?}"))),
        }
    }

    fn endpoint(self, base_path: &str) -> String {
        let base_path = base_path.trim_end_matches('/');
        match (self, base_path.ends_with("/v1")) {
            (Self::OpenAiCompatible, true) => format!("{base_path}/chat/completions"),
            (Self::Anthropic, true) => format!("{base_path}/messages"),
            (Self::OpenAiCompatible, false) => format!("{base_path}/v1/chat/completions"),
            (Self::Anthropic, false) => format!("{base_path}/v1/messages"),
        }
    }
}

pub struct ModelClient {
    client: Client<HttpConnector, Full<Bytes>>,
    endpoint: Uri,
    provider: ModelProvider,
    model: String,
    credential_name: String,
}

#[derive(Debug)]
pub struct ModelError(String);

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ModelError {}

impl ModelClient {
    pub fn new(
        proxy_url: impl AsRef<str>,
        provider: ModelProvider,
        model: impl Into<String>,
        credential_name: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let proxy_uri = Uri::from_str(proxy_url.as_ref())
            .map_err(|error| ModelError(format!("invalid proxy URL: {error}")))?;
        let authority = proxy_uri
            .authority()
            .ok_or_else(|| ModelError("MS_PROXY_URL has no authority".to_owned()))?;
        if proxy_uri.scheme_str() != Some("http") {
            return Err(ModelError(
                "MS_PROXY_URL must use http for the session proxy".to_owned(),
            ));
        }
        let endpoint_path = provider.endpoint(proxy_uri.path());
        let endpoint = Uri::builder()
            .scheme("http")
            .authority(authority.clone())
            .path_and_query(endpoint_path)
            .build()
            .map_err(|error| ModelError(format!("invalid proxy endpoint: {error}")))?;
        let model = model.into();
        let credential_name = credential_name.into();
        if model.trim().is_empty() || credential_name.trim().is_empty() {
            return Err(ModelError(
                "model and credential name must not be empty".to_owned(),
            ));
        }
        let client = Client::builder(TokioExecutor::new()).build_http();
        Ok(Self {
            client,
            endpoint,
            provider,
            model,
            credential_name,
        })
    }

    pub fn from_env(context: &AgentContext) -> Result<Self, ModelError> {
        let proxy_url = context
            .proxy_url
            .as_deref()
            .ok_or_else(|| ModelError("MS_PROXY_URL is missing".to_owned()))?;
        let provider = ModelProvider::parse(
            &std::env::var("MS_MODEL_PROVIDER")
                .map_err(|_| ModelError("MS_MODEL_PROVIDER is missing".to_owned()))?,
        )?;
        let model = std::env::var("MS_MODEL_NAME")
            .map_err(|_| ModelError("MS_MODEL_NAME is missing".to_owned()))?;
        let credential_name = std::env::var("MS_MODEL_CREDENTIAL")
            .map_err(|_| ModelError("MS_MODEL_CREDENTIAL is missing".to_owned()))?;
        if !context.scope.credentials.contains(&credential_name) {
            return Err(ModelError(
                "model credential name is outside the session scope".to_owned(),
            ));
        }
        Self::new(proxy_url, provider, model, credential_name)
    }

    pub async fn complete(
        &self,
        history: &[HistoryEntry],
        user_text: &str,
    ) -> Result<String, ModelError> {
        let request_body = self.request_body(history, user_text);
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(self.endpoint.clone())
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .header("x-ms-credential", &self.credential_name);
        if self.provider == ModelProvider::Anthropic {
            request = request.header("anthropic-version", "2023-06-01");
        }
        let request = request
            .body(Full::new(Bytes::from(request_body.to_string())))
            .map_err(|error| ModelError(format!("cannot build model request: {error}")))?;
        let response = self
            .client
            .request(request)
            .await
            .map_err(|error| ModelError(format!("model proxy request failed: {error}")))?;
        let status = response.status();
        let body = Limited::new(response.into_body(), MAX_RESPONSE_BYTES)
            .collect()
            .await
            .map_err(|error| ModelError(format!("model proxy response body failed: {error}")))?
            .to_bytes();
        if status != StatusCode::OK {
            let excerpt = String::from_utf8_lossy(&body);
            return Err(ModelError(format!(
                "model proxy returned HTTP {status}: {}",
                excerpt.chars().take(512).collect::<String>()
            )));
        }
        let response: Value = serde_json::from_slice(&body)
            .map_err(|error| ModelError(format!("invalid model response JSON: {error}")))?;
        let content = match self.provider {
            ModelProvider::OpenAiCompatible => response
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            ModelProvider::Anthropic => response.pointer("/content/0/text").and_then(Value::as_str),
        }
        .ok_or_else(|| ModelError("model response has no text content".to_owned()))?;
        Ok(content.to_owned())
    }

    fn request_body(&self, history: &[HistoryEntry], user_text: &str) -> Value {
        let messages: Vec<Value> = history
            .iter()
            .filter(|entry| matches!(entry.role.as_str(), "user" | "assistant"))
            .map(|entry| json!({ "role": entry.role, "content": entry.content }))
            .chain(std::iter::once(
                json!({ "role": "user", "content": user_text }),
            ))
            .collect();
        match self.provider {
            ModelProvider::OpenAiCompatible => json!({
                "model": self.model,
                "messages": messages,
            }),
            ModelProvider::Anthropic => json!({
                "model": self.model,
                "max_tokens": 4096,
                "messages": messages,
            }),
        }
    }
}

impl TurnProcessor for ModelClient {
    fn process<'a>(
        &'a mut self,
        _context: &'a AgentContext,
        message: InboundMessage,
        mut history: Vec<HistoryEntry>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnOutput, String>> + 'a>> {
        Box::pin(async move {
            if message.kind != InboundKind::UserMessage {
                return Err(format!(
                    "unsupported inbound message kind: {:?}",
                    message.kind
                ));
            }
            let user_text = message
                .body
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "user message is missing text".to_owned())?;
            let reply = self
                .complete(&history, user_text)
                .await
                .map_err(|error| error.to_string())?;
            history.push(HistoryEntry {
                role: "user".to_owned(),
                content: json!(user_text),
            });
            history.push(HistoryEntry {
                role: "assistant".to_owned(),
                content: json!(reply),
            });
            Ok(TurnOutput {
                outbound: vec![OutboundMessage {
                    id: None,
                    kind: OutboundKind::Reply,
                    body: json!({ "text": reply }),
                    in_reply_to: message.id,
                    created_at: None,
                }],
                history,
            })
        })
    }
}
