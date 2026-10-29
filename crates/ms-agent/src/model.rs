use crate::agent_loop::{TurnError, TurnOutput, TurnProcessor};
use crate::boot::AgentContext;
use crate::{
    mcp::McpClient,
    permit::{PermitError, authorize_approved_tool, authorize_tool},
};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use ms_proto::{ApprovalBinding, InboundKind, InboundMessage, OutboundKind, OutboundMessage};
use ms_store::HistoryEntry;
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
static NEXT_APPROVAL_LOCAL_ID: AtomicU64 = AtomicU64::new(1);

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
            _ => Err(ModelError::new(format!(
                "unsupported model provider {value:?}"
            ))),
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
    system_instruction: Option<String>,
    mcp: Option<McpClient>,
}

#[derive(Debug)]
pub struct ModelError {
    message: String,
    http_status: Option<u16>,
    transport_failure: bool,
}

impl ModelError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            http_status: None,
            transport_failure: false,
        }
    }

    fn transient(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            http_status: None,
            transport_failure: true,
        }
    }

    fn with_status(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            http_status: Some(status.as_u16()),
            transport_failure: false,
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ModelError {}

impl ModelError {
    fn is_configuration_failure(&self) -> bool {
        match self.http_status {
            Some(400..=499) => !matches!(self.http_status, Some(408 | 429)),
            Some(_) => false,
            None => !self.transport_failure,
        }
    }

    fn is_transient_failure(&self) -> bool {
        self.transport_failure || matches!(self.http_status, Some(408 | 429 | 500..=599))
    }
}

impl ModelClient {
    pub fn new(
        proxy_url: impl AsRef<str>,
        provider: ModelProvider,
        model: impl Into<String>,
        credential_name: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let proxy_uri = Uri::from_str(proxy_url.as_ref())
            .map_err(|error| ModelError::new(format!("invalid proxy URL: {error}")))?;
        let authority = proxy_uri
            .authority()
            .ok_or_else(|| ModelError::new("MS_PROXY_URL has no authority".to_owned()))?;
        if proxy_uri.scheme_str() != Some("http") {
            return Err(ModelError::new(
                "MS_PROXY_URL must use http for the session proxy".to_owned(),
            ));
        }
        let endpoint_path = provider.endpoint(proxy_uri.path());
        let endpoint = Uri::builder()
            .scheme("http")
            .authority(authority.clone())
            .path_and_query(endpoint_path)
            .build()
            .map_err(|error| ModelError::new(format!("invalid proxy endpoint: {error}")))?;
        let model = model.into();
        let credential_name = credential_name.into();
        if model.trim().is_empty() || credential_name.trim().is_empty() {
            return Err(ModelError::new(
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
            system_instruction: None,
            mcp: None,
        })
    }

    pub fn from_env(context: &AgentContext) -> Result<Self, ModelError> {
        let proxy_url = context
            .proxy_url
            .as_deref()
            .ok_or_else(|| ModelError::new("MS_PROXY_URL is missing".to_owned()))?;
        let provider = ModelProvider::parse(
            &std::env::var("MS_MODEL_PROVIDER")
                .map_err(|_| ModelError::new("MS_MODEL_PROVIDER is missing".to_owned()))?,
        )?;
        let model = std::env::var("MS_MODEL_NAME")
            .map_err(|_| ModelError::new("MS_MODEL_NAME is missing".to_owned()))?;
        let credential_name = std::env::var("MS_MODEL_CREDENTIAL")
            .map_err(|_| ModelError::new("MS_MODEL_CREDENTIAL is missing".to_owned()))?;
        if !context.scope.credentials.contains(&credential_name) {
            return Err(ModelError::new(
                "model credential name is outside the session scope".to_owned(),
            ));
        }
        let mut client = Self::new(proxy_url, provider, model, credential_name)?;
        client.system_instruction = std::env::var("MS_ROLE_INSTRUCTIONS")
            .ok()
            .map(std::fs::read_to_string)
            .transpose()
            .map_err(|error| ModelError::new(format!("cannot read role instructions: {error}")))?;
        client.mcp =
            Some(McpClient::from_env(&context.scope).map_err(|error| {
                ModelError::new(format!("cannot configure MCP clients: {error}"))
            })?);
        Ok(client)
    }

    pub async fn complete(
        &self,
        history: &[HistoryEntry],
        user_text: &str,
    ) -> Result<String, ModelError> {
        let response = self.complete_raw(history, user_text, &[]).await?;
        self.text_response(&response)
    }

    async fn complete_raw(
        &self,
        history: &[HistoryEntry],
        user_text: &str,
        tools: &[Value],
    ) -> Result<Value, ModelError> {
        let request_body = self.request_body(history, user_text, tools);
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
            .map_err(|error| ModelError::new(format!("cannot build model request: {error}")))?;
        let response = self.client.request(request).await.map_err(|error| {
            ModelError::transient(format!("model proxy request failed: {error}"))
        })?;
        let status = response.status();
        let body = Limited::new(response.into_body(), MAX_RESPONSE_BYTES)
            .collect()
            .await
            .map_err(|error| {
                ModelError::transient(format!("model proxy response body failed: {error}"))
            })?
            .to_bytes();
        if status != StatusCode::OK {
            let excerpt = String::from_utf8_lossy(&body);
            return Err(ModelError::with_status(
                status,
                format!(
                    "model proxy returned HTTP {status}: {}",
                    excerpt.chars().take(512).collect::<String>()
                ),
            ));
        }
        let response: Value = serde_json::from_slice(&body)
            .map_err(|error| ModelError::new(format!("invalid model response JSON: {error}")))?;
        Ok(response)
    }

    fn text_response(&self, response: &Value) -> Result<String, ModelError> {
        let content = match self.provider {
            ModelProvider::OpenAiCompatible => response
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            ModelProvider::Anthropic => response.pointer("/content/0/text").and_then(Value::as_str),
        }
        .ok_or_else(|| ModelError::new("model response has no text content".to_owned()))?;
        Ok(content.to_owned())
    }

    fn request_body(&self, history: &[HistoryEntry], user_text: &str, tools: &[Value]) -> Value {
        let messages: Vec<Value> = history
            .iter()
            .filter(|entry| matches!(entry.role.as_str(), "user" | "assistant" | "tool"))
            .filter(|entry| entry.content.get("_ms_pending_approval").is_none())
            .map(history_message)
            .chain((!user_text.is_empty()).then(|| json!({ "role": "user", "content": user_text })))
            .collect();
        match self.provider {
            ModelProvider::OpenAiCompatible => {
                let messages = if let Some(instruction) = &self.system_instruction {
                    std::iter::once(json!({"role":"system","content":instruction}))
                        .chain(messages)
                        .collect::<Vec<_>>()
                } else {
                    messages
                };
                json!({
                    "model": self.model,
                    "messages": messages,
                    "tools": tools,
                })
            }
            ModelProvider::Anthropic => json!({
                "model": self.model,
                "max_tokens": 4096,
                "system": self.system_instruction,
                "messages": messages,
                "tools": tools,
            }),
        }
    }

    async fn available_tools(
        &mut self,
        kind: &str,
        delegation_enabled: bool,
    ) -> Result<Vec<Value>, String> {
        let mut tools = match &mut self.mcp {
            Some(mcp) => mcp.model_tools().await.map_err(|error| error.to_string()),
            None => Ok(Vec::new()),
        }?;
        tools.extend(crate::delegate::tool_definitions(
            kind,
            delegation_enabled,
            self.provider == ModelProvider::Anthropic,
        ));
        if self.provider == ModelProvider::Anthropic {
            Ok(tools
                .into_iter()
                .filter_map(|tool| {
                    let function = tool.get("function")?;
                    Some(json!({
                        "name":function.get("name")?,
                        "description":function.get("description")?,
                        "input_schema":function.get("parameters")?,
                    }))
                })
                .collect())
        } else {
            Ok(tools)
        }
    }

    async fn call_tool(
        &mut self,
        context: &mut AgentContext,
        server: &str,
        tool: &str,
        arguments: Value,
        approved: Option<&ApprovalBinding>,
        in_reply_to: Option<i64>,
    ) -> Result<Value, String> {
        let args = arguments
            .as_object()
            .cloned()
            .ok_or_else(|| "model tool arguments must be a JSON object".to_owned())?;
        if server == "microswarm-core" {
            if context.kind != "lead"
                || !context
                    .scope
                    .delegation
                    .as_ref()
                    .is_some_and(|delegation| delegation.enabled)
            {
                return Err("delegation tools are unavailable to this session".into());
            }
            let mut permission_scope = context.scope.clone();
            permission_scope.tools.push(ms_proto::ToolGrant {
                server: "microswarm-core".into(),
                tools: ms_proto::ToolNames::Selected([tool.to_owned()].into_iter().collect()),
                policy: ms_proto::Permission::Allow,
                overrides: Default::default(),
            });
            authorize_tool(&permission_scope, server, tool, Some(args))
                .map_err(|error| error.to_string())?;
            let message = crate::delegate::prepare_outbound(tool, arguments, in_reply_to)?;
            crate::delegate::write_outbound(&mut context.store, &message)?;
            let (event_type, data) = crate::delegate::event_for(&message);
            crate::emit::emit(
                &mut context.store,
                &context.session_id,
                &context.root_session_id,
                &context.role,
                event_type,
                data,
            )
            .map_err(|error| error.to_string())?;
            let expected = crate::delegate::expected_result_kind(tool)?;
            let result = crate::delegate::wait_for_result(
                &mut context.store,
                in_reply_to.unwrap_or(0),
                expected,
                std::time::Duration::from_secs(300),
            )
            .await?;
            return Ok(result.body);
        }
        let permit = match approved {
            Some(binding) => {
                authorize_approved_tool(&context.scope, server, tool, Some(args), binding)
            }
            None => authorize_tool(&context.scope, server, tool, Some(args)),
        };
        let permit = permit.map_err(|error| error.to_string())?;
        let mcp = self
            .mcp
            .as_mut()
            .ok_or_else(|| "no MCP servers are configured".to_owned())?;
        let result = mcp
            .call_tool(permit)
            .await
            .map_err(|error| error.to_string())?;
        serde_json::to_value(result).map_err(|error| error.to_string())
    }
}

fn history_message(entry: &HistoryEntry) -> Value {
    if entry.content.get("_ms_pending_approval").is_some() {
        return Value::Null;
    }
    entry
        .content
        .get("_ms_message")
        .cloned()
        .unwrap_or_else(|| json!({ "role": entry.role, "content": entry.content }))
}

fn model_tool_call(
    provider: ModelProvider,
    response: &Value,
) -> Option<(String, String, String, Value)> {
    match provider {
        ModelProvider::OpenAiCompatible => {
            let call = response.pointer("/choices/0/message/tool_calls/0")?;
            let id = call.get("id")?.as_str()?.to_owned();
            let function = call.get("function")?;
            let (server, tool) = function.get("name")?.as_str()?.split_once('/')?;
            let arguments = function
                .get("arguments")?
                .as_str()
                .and_then(|value| serde_json::from_str(value).ok())?;
            Some((id, server.to_owned(), tool.to_owned(), arguments))
        }
        ModelProvider::Anthropic => {
            let call = response
                .get("content")?
                .as_array()?
                .iter()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))?;
            let id = call.get("id")?.as_str()?.to_owned();
            let (server, tool) = call.get("name")?.as_str()?.split_once('/')?;
            Some((
                id,
                server.to_owned(),
                tool.to_owned(),
                call.get("input")?.clone(),
            ))
        }
    }
}

impl TurnProcessor for ModelClient {
    fn process<'a>(
        &'a mut self,
        context: &'a mut AgentContext,
        message: InboundMessage,
        history: Vec<HistoryEntry>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnOutput, TurnError>> + 'a>> {
        Box::pin(async move {
            match message.kind {
                InboundKind::UserMessage | InboundKind::Task => {
                    let user_text = message
                        .body
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or_else(|| TurnError::Config("user message is missing text".into()))?;
                    self.run_turn(context, history, user_text, message.id, true)
                        .await
                }
                InboundKind::ApprovalResult => {
                    self.resume_approved(context, history, &message.body, message.id)
                        .await
                }
                kind => Err(TurnError::Config(format!(
                    "unsupported inbound message kind: {kind:?}"
                ))),
            }
        })
    }
}

impl ModelClient {
    async fn run_turn(
        &mut self,
        context: &mut AgentContext,
        original_history: Vec<HistoryEntry>,
        user_text: &str,
        in_reply_to: Option<i64>,
        include_user: bool,
    ) -> Result<TurnOutput, TurnError> {
        let mut delta = Vec::new();
        let mut conversation = original_history.clone();
        let mut user_pending = include_user.then(|| HistoryEntry {
            role: "user".into(),
            content: json!(user_text),
        });
        if include_user {
            delta.push(user_pending.clone().expect("user entry exists"));
        }
        for _ in 0..8 {
            let tools = self
                .available_tools(
                    &context.kind,
                    context
                        .scope
                        .delegation
                        .as_ref()
                        .is_some_and(|delegation| delegation.enabled),
                )
                .await
                .map_err(TurnError::Transient)?;
            let request_text = if delta.len() == 1 && include_user {
                user_text
            } else {
                ""
            };
            let response = self
                .complete_raw(&conversation, request_text, &tools)
                .await
                .map_err(map_model_error)?;
            if let Some((call_id, server, tool, arguments)) =
                model_tool_call(self.provider, &response)
            {
                if let Some(user) = user_pending.take() {
                    conversation.push(user);
                }
                let assistant_message = match self.provider {
                    ModelProvider::OpenAiCompatible => response
                        .pointer("/choices/0/message")
                        .cloned()
                        .unwrap_or(Value::Null),
                    ModelProvider::Anthropic => {
                        json!({"role":"assistant", "content":response.get("content").cloned().unwrap_or(Value::Null)})
                    }
                };
                let assistant = HistoryEntry {
                    role: "assistant".into(),
                    content: json!({"_ms_message":assistant_message}),
                };
                conversation.push(assistant.clone());
                delta.push(assistant);
                let binding = ApprovalBinding::for_call(&server, &tool, Some(&arguments));
                match self
                    .call_tool(
                        context,
                        &server,
                        &tool,
                        arguments.clone(),
                        None,
                        in_reply_to,
                    )
                    .await
                {
                    Ok(result) => {
                        let result_entry = tool_result_history(self.provider, &call_id, result);
                        conversation.push(result_entry.clone());
                        delta.push(result_entry);
                    }
                    Err(reason) if reason == PermitError::ApprovalRequired.to_string() => {
                        let local_id = NEXT_APPROVAL_LOCAL_ID.fetch_add(1, Ordering::Relaxed);
                        delta.push(HistoryEntry {
                            role: "tool".into(),
                            content: json!({"_ms_pending_approval":{"local_id":local_id,"call_id":call_id,"server":server,"tool":tool,"arguments":arguments,"binding":binding}}),
                        });
                        return Ok(TurnOutput {
                            outbound: vec![OutboundMessage {
                                id: None,
                                kind: OutboundKind::ApprovalRequest,
                                body: json!({"approval_local_id":local_id,"server":server,"tool":tool,"args":arguments,"binding":binding}),
                                in_reply_to,
                                created_at: None,
                            }],
                            history: delta,
                        });
                    }
                    Err(reason)
                        if reason == PermitError::Denied.to_string()
                            || reason == PermitError::NotGranted.to_string() =>
                    {
                        let reply = format!("Tool call denied: {server}/{tool}.");
                        return Ok(reply_output(reply, in_reply_to, delta));
                    }
                    Err(reason) => return Err(TurnError::Config(reason)),
                }
                continue;
            }
            let reply = self.text_response(&response).map_err(map_model_error)?;
            delta.push(HistoryEntry {
                role: "assistant".into(),
                content: json!(reply),
            });
            return Ok(reply_output(reply, in_reply_to, delta));
        }
        Err(TurnError::Config(
            "model exceeded the maximum of eight tool calls in one turn".into(),
        ))
    }

    async fn resume_approved(
        &mut self,
        context: &mut AgentContext,
        history: Vec<HistoryEntry>,
        result: &Value,
        in_reply_to: Option<i64>,
    ) -> Result<TurnOutput, TurnError> {
        let pending = history
            .iter()
            .rev()
            .find_map(|entry| entry.content.get("_ms_pending_approval"))
            .ok_or_else(|| TurnError::Config("approval_result has no pending tool call".into()))?;
        let server = pending
            .get("server")
            .and_then(Value::as_str)
            .ok_or_else(|| TurnError::Config("pending approval has no server".into()))?;
        let tool = pending
            .get("tool")
            .and_then(Value::as_str)
            .ok_or_else(|| TurnError::Config("pending approval has no tool".into()))?;
        let call_id = pending
            .get("call_id")
            .and_then(Value::as_str)
            .ok_or_else(|| TurnError::Config("pending approval has no call id".into()))?;
        let arguments = pending
            .get("arguments")
            .cloned()
            .ok_or_else(|| TurnError::Config("pending approval has no arguments".into()))?;
        let stored_binding: ApprovalBinding = serde_json::from_value(
            pending
                .get("binding")
                .cloned()
                .ok_or_else(|| TurnError::Config("pending approval has no binding".into()))?,
        )
        .map_err(|error| TurnError::Config(error.to_string()))?;
        let approved_binding: ApprovalBinding = serde_json::from_value(
            result
                .get("binding")
                .cloned()
                .ok_or_else(|| TurnError::Config("approval result has no binding".into()))?,
        )
        .map_err(|error| TurnError::Config(error.to_string()))?;
        if stored_binding != approved_binding
            || !stored_binding.matches_call(server, tool, Some(&arguments))
        {
            return Err(TurnError::Config(
                "approval binding does not match the pending tool call".into(),
            ));
        }
        if result.get("decision").and_then(Value::as_str) != Some("approved") {
            let result_entry = tool_result_history(
                self.provider,
                call_id,
                json!({"isError":true,"content":"approval denied"}),
            );
            let mut conversation = history;
            conversation.push(result_entry.clone());
            let mut delta = vec![result_entry];
            let tools = self
                .available_tools(
                    &context.kind,
                    context
                        .scope
                        .delegation
                        .as_ref()
                        .is_some_and(|delegation| delegation.enabled),
                )
                .await
                .map_err(TurnError::Transient)?;
            let response = self
                .complete_raw(&conversation, "", &tools)
                .await
                .map_err(map_model_error)?;
            let reply = self.text_response(&response).map_err(map_model_error)?;
            delta.push(HistoryEntry {
                role: "assistant".into(),
                content: json!(reply),
            });
            return Ok(reply_output(reply, in_reply_to, delta));
        }
        let result = self
            .call_tool(
                context,
                server,
                tool,
                arguments,
                Some(&approved_binding),
                in_reply_to,
            )
            .await
            .map_err(TurnError::Config)?;
        let result_entry = tool_result_history(self.provider, call_id, result);
        let mut conversation = history;
        conversation.push(result_entry.clone());
        let tools = self
            .available_tools(
                &context.kind,
                context
                    .scope
                    .delegation
                    .as_ref()
                    .is_some_and(|delegation| delegation.enabled),
            )
            .await
            .map_err(TurnError::Transient)?;
        let response = self
            .complete_raw(&conversation, "", &tools)
            .await
            .map_err(map_model_error)?;
        if model_tool_call(self.provider, &response).is_some() {
            return Err(TurnError::Config(
                "additional tool calls after approval require another turn".into(),
            ));
        }
        let reply = self.text_response(&response).map_err(map_model_error)?;
        Ok(reply_output(
            reply.clone(),
            in_reply_to,
            vec![
                result_entry,
                HistoryEntry {
                    role: "assistant".into(),
                    content: json!(reply),
                },
            ],
        ))
    }
}

fn tool_result_history(provider: ModelProvider, call_id: &str, result: Value) -> HistoryEntry {
    let message = match provider {
        ModelProvider::OpenAiCompatible => {
            json!({"role":"tool","tool_call_id":call_id,"content":result.to_string()})
        }
        ModelProvider::Anthropic => {
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":call_id,"content":result.to_string()}]})
        }
    };
    HistoryEntry {
        role: "tool".into(),
        content: json!({"_ms_message":message}),
    }
}

fn reply_output(reply: String, in_reply_to: Option<i64>, history: Vec<HistoryEntry>) -> TurnOutput {
    TurnOutput {
        outbound: vec![OutboundMessage {
            id: None,
            kind: OutboundKind::Reply,
            body: json!({"text":reply}),
            in_reply_to,
            created_at: None,
        }],
        history,
    }
}

fn map_model_error(error: ModelError) -> TurnError {
    let message = error.to_string();
    if error.is_configuration_failure() || !error.is_transient_failure() {
        TurnError::Config(message)
    } else {
        TurnError::Transient(message)
    }
}
