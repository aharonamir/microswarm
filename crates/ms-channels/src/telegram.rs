use crate::{ChannelAdapter, ChannelInbound, supervisor_rpc};
use ms_proto::{InboundKind, InboundMessage};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

pub struct TelegramAdapter {
    token: String,
    api_base: String,
    client: reqwest::Client,
}

impl TelegramAdapter {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            api_base: "https://api.telegram.org".into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    pub fn api_url(&self, api_base: &str, method: &str) -> String {
        format!(
            "{}/bot{}/{method}",
            api_base.trim_end_matches('/'),
            self.token
        )
    }

    pub async fn get_updates(&self, offset: Option<i64>) -> Result<Vec<Value>, String> {
        let mut url = format!("{}?timeout=20", self.api_url(&self.api_base, "getUpdates"));
        if let Some(offset) = offset {
            url.push_str(&format!("&offset={offset}"));
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "Telegram getUpdates request failed".to_owned())?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Telegram returned invalid JSON".to_owned())?;
        if !status.is_success() || body["ok"] != true {
            return Err(format!("Telegram getUpdates returned HTTP {status}"));
        }
        body["result"]
            .as_array()
            .cloned()
            .ok_or_else(|| "Telegram getUpdates response has no result array".to_owned())
    }

    pub async fn send_message(&self, thread_id: &str, text: &str) -> Result<(), String> {
        let url = self.api_url(&self.api_base, "sendMessage");
        let response = self
            .client
            .post(url)
            .json(&self.send_message_payload(thread_id, text))
            .send()
            .await
            .map_err(|_| "Telegram sendMessage request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "Telegram sendMessage returned HTTP {}",
                response.status()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Telegram returned invalid JSON".to_owned())?;
        if body["ok"] != true {
            return Err("Telegram rejected sendMessage".into());
        }
        Ok(())
    }

    pub fn send_message_payload(&self, thread_id: &str, text: &str) -> Value {
        let chat_id = thread_id
            .parse::<i64>()
            .map_or_else(|_| Value::String(thread_id.to_owned()), |id| json!(id));
        let mut payload = json!({"chat_id":chat_id,"text":text});
        if let Some(code) = approval_code(text) {
            payload["reply_markup"] = json!({"inline_keyboard":[[
                {"text":"Approve","callback_data":format!("/approve {code}")},
                {"text":"Deny","callback_data":format!("/deny {code}")}
            ]]});
        }
        payload
    }

    fn update_message(&self, payload: &Value) -> Option<(String, String)> {
        let message = payload
            .get("message")
            .or_else(|| payload.pointer("/callback_query/message"))?;
        let chat_id = message.pointer("/chat/id")?.as_i64()?.to_string();
        let text = payload
            .pointer("/callback_query/data")
            .and_then(Value::as_str)
            .or_else(|| message.get("text").and_then(Value::as_str))?;
        Some((chat_id, text.to_owned()))
    }
}

impl ChannelAdapter for TelegramAdapter {
    fn kind(&self) -> &'static str {
        "telegram"
    }

    fn parse_inbound(&self, payload: &Value) -> Result<Option<ChannelInbound>, String> {
        let Some((thread_id, text)) = self.update_message(payload) else {
            return Ok(None);
        };
        Ok(Some(ChannelInbound {
            thread_id,
            message: InboundMessage {
                id: None,
                kind: InboundKind::UserMessage,
                body: json!({"text":text}),
                created_at: None,
            },
        }))
    }

    fn reply_payload(&self, thread_id: &str, text: &str) -> Value {
        self.send_message_payload(thread_id, text)
    }
}

pub async fn run(home: PathBuf, socket: PathBuf, credential: String, api_base: String) {
    let mut offset = None;
    let mut missing_credential_reported = false;
    loop {
        let token = match read_credential(&home, &credential) {
            Ok(token) => {
                missing_credential_reported = false;
                token
            }
            Err(error) => {
                if !missing_credential_reported {
                    eprintln!("Telegram channel waiting for its credential: {error}");
                    missing_credential_reported = true;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        let adapter = TelegramAdapter::new(token).with_api_base(api_base.clone());
        match adapter.get_updates(offset).await {
            Ok(updates) => {
                if updates.is_empty() {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    continue;
                }
                for update in updates {
                    if let Some(update_id) = update.get("update_id").and_then(Value::as_i64) {
                        offset = Some(update_id + 1);
                    }
                    let inbound = match adapter.parse_inbound(&update) {
                        Ok(Some(inbound)) => inbound,
                        Ok(None) => continue,
                        Err(error) => {
                            eprintln!("Telegram update was rejected: {error}");
                            continue;
                        }
                    };
                    let text = inbound.message.body["text"].as_str().unwrap_or_default();
                    match supervisor_rpc(
                        &socket,
                        json!({"op":"channel_message","channel":"telegram","thread_id":inbound.thread_id,"text":text}),
                    )
                    .await
                    {
                        Ok(response) => {
                            let reply = response["prompt"]
                                .as_str()
                                .or_else(|| response["text"].as_str())
                                .unwrap_or("The message was processed.");
                            if let Err(error) = adapter.send_message(&inbound.thread_id, reply).await {
                                eprintln!("Telegram reply delivery failed: {error}");
                            }
                        }
                        Err(error) => eprintln!("Telegram message routing failed: {error}"),
                    }
                }
            }
            Err(error) => {
                eprintln!("Telegram polling failed: {error}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
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

fn approval_code(text: &str) -> Option<&str> {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "/approve")
        .map(|pair| pair[1])
        .filter(|code| code.len() == 4 && code.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}
