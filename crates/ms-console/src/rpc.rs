use crate::events::{ConsoleEvent, EventRing};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::Mutex,
};

pub async fn request(home: &Path, request: &Value) -> Result<Value, String> {
    let socket = home.join("supervisor.sock");
    let mut stream = UnixStream::connect(&socket)
        .await
        .map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await.map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&line).map_err(|e| e.to_string())
}

pub async fn follow(home: std::path::PathBuf, ring: Arc<Mutex<EventRing>>) {
    let mut link = SupervisorLink::default();
    let mut cursors = BTreeMap::<String, i64>::new();
    loop {
        let path = home.join("supervisor.sock");
        if let Ok(mut stream) = UnixStream::connect(path).await {
            if let Some(name) = link.set_connected(true) {
                push(&ring, name, json!({})).await;
            }
            let request = json!({"op":"event_stream","follow":true,"after":cursors});
            let mut request = serde_json::to_vec(&request).unwrap_or_default();
            request.push(b'\n');
            if stream.write_all(&request).await.is_err() {
                if let Some(name) = link.set_connected(false) {
                    push(&ring, name, json!({})).await;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => {
                        break;
                    }
                    Ok(_) => {
                        let Ok(event) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        if let (Some(session), Some(seq)) =
                            (event["session_id"].as_str(), event["seq"].as_i64())
                        {
                            cursors.insert(session.to_owned(), seq);
                        }
                        let source = event["source"].as_str().unwrap_or("supervisor");
                        let kind = if source == "agent" {
                            "agent"
                        } else {
                            "supervisor"
                        };
                        let payload = json!({"type":kind,"at":event["ts"],"event":event});
                        push(&ring, kind, payload).await;
                    }
                }
            }
        }
        if let Some(name) = link.set_connected(false) {
            push(&ring, name, json!({})).await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[derive(Default)]
pub struct SupervisorLink {
    connected: bool,
    initialized: bool,
}

impl SupervisorLink {
    pub fn set_connected(&mut self, connected: bool) -> Option<&'static str> {
        let changed = self.initialized && self.connected != connected;
        self.connected = connected;
        self.initialized = true;
        changed.then_some(if connected {
            "supervisor.up"
        } else {
            "supervisor.down"
        })
    }
}

async fn push(ring: &Arc<Mutex<EventRing>>, name: &str, data: Value) {
    ring.lock().await.push(ConsoleEvent {
        id: 0,
        name: name.to_owned(),
        data,
    });
}
