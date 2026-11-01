use crate::{
    events::{ConsoleEvent, EventRing},
    rpc,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct ChatManager {
    in_flight: BTreeMap<String, String>,
}

impl ChatManager {
    pub fn begin(&mut self, session: &str, request: &str) -> bool {
        if self.in_flight.contains_key(session) {
            return false;
        }
        self.in_flight.insert(session.into(), request.into());
        true
    }
    pub fn finish(&mut self, session: &str, request: &str) {
        if self
            .in_flight
            .get(session)
            .is_some_and(|active| active == request)
        {
            self.in_flight.remove(session);
        }
    }
    pub fn has_in_flight(&self, session: &str) -> bool {
        self.in_flight.contains_key(session)
    }
}

pub async fn submit(
    home: PathBuf,
    session_id: Option<String>,
    thread_id: String,
    text: String,
    manager: Arc<Mutex<ChatManager>>,
    ring: Arc<Mutex<EventRing>>,
) -> Result<Value, &'static str> {
    let request_id = ms_runtime::new_id().map_err(|_| "request_id_unavailable")?;
    let lock_key = session_id
        .clone()
        .unwrap_or_else(|| format!("thread:{thread_id}"));
    if !manager.lock().await.begin(&lock_key, &request_id) {
        return Err("session_busy");
    }
    let response = json!({"request_id":request_id,"thread_id":thread_id,"session_id":session_id});
    tokio::spawn(async move {
        let first = json!({"op":"chat","session_id":session_id,"channel":"web","thread_id":thread_id,"text":text});
        let result = rpc::request(&home, &first).await;
        match result {
            Ok(mut reply) => {
                let mut current_session = reply["session_id"]
                    .as_str()
                    .map(str::to_owned)
                    .or(session_id.clone());
                loop {
                    if reply.get("error").is_some() {
                        emit_error(
                            &ring,
                            &request_id,
                            current_session.as_deref(),
                            error_code(reply["error"].as_str().unwrap_or_default()),
                        )
                        .await;
                        break;
                    }
                    if reply["pending"] == true {
                        emit(&ring,"chat.pending",json!({"type":"chat.pending","at":ms_runtime::utc_now(),"request_id":request_id,"session_id":current_session})).await;
                        let (Some(id), Some(after)) =
                            (reply["session_id"].as_str(), reply["after_id"].as_i64())
                        else {
                            emit_error(
                                &ring,
                                &request_id,
                                current_session.as_deref(),
                                "supervisor_error",
                            )
                            .await;
                            break;
                        };
                        current_session = Some(id.to_owned());
                        reply = match rpc::request(
                            &home,
                            &json!({"op":"wait","session_id":id,"after_id":after}),
                        )
                        .await
                        {
                            Ok(next) => next,
                            Err(_) => {
                                emit_error(
                                    &ring,
                                    &request_id,
                                    current_session.as_deref(),
                                    "supervisor_error",
                                )
                                .await;
                                break;
                            }
                        };
                        continue;
                    }
                    if let Some(text) = reply["text"].as_str() {
                        emit(&ring,"chat.reply",json!({"type":"chat.reply","at":ms_runtime::utc_now(),"request_id":request_id,"session_id":current_session,"text":text})).await;
                    } else {
                        emit_error(
                            &ring,
                            &request_id,
                            current_session.as_deref(),
                            "supervisor_error",
                        )
                        .await;
                    }
                    break;
                }
            }
            Err(_) => {
                emit_error(
                    &ring,
                    &request_id,
                    session_id.as_deref(),
                    "supervisor_error",
                )
                .await
            }
        }
        manager.lock().await.finish(&lock_key, &request_id);
    });
    Ok(response)
}

fn error_code(error: &str) -> &'static str {
    if error.contains("timed out") {
        "timeout"
    } else if error.contains("closed") {
        "session_closed"
    } else if error.contains("model") {
        "model_not_configured"
    } else {
        "supervisor_error"
    }
}

async fn emit_error(
    ring: &Arc<Mutex<EventRing>>,
    request_id: &str,
    session: Option<&str>,
    error: &str,
) {
    emit(ring,"chat.error",json!({"type":"chat.error","at":ms_runtime::utc_now(),"request_id":request_id,"session_id":session,"error":error,"text":"The message could not be completed."})).await;
}

async fn emit(ring: &Arc<Mutex<EventRing>>, name: &str, data: Value) {
    ring.lock().await.push(ConsoleEvent {
        id: 0,
        name: name.into(),
        data,
    });
}
