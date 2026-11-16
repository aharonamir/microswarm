use ms_console::{chat::ChatManager, events::EventRing, read, server::api_post};
use serde_json::json;
use std::{
    fs,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn credential_values_never_appear_in_host_view() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-secret-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let credentials = home.join("credentials");
    fs::create_dir_all(&credentials).unwrap();
    fs::write(
        credentials.join("model-key"),
        "never-return-this-secret-value",
    )
    .unwrap();
    let response = read::host(&home).to_string();
    assert!(response.contains("model-key"));
    assert!(!response.contains("never-return-this-secret-value"));
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn credential_set_value_is_absent_from_api_sse_and_host_responses() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-secret-api-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&home).unwrap();
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["op"], "creds_set");
        assert_eq!(request["value"], "never-return-this-secret-value");
        write.write_all(b"{\"ok\":true}\n").await.unwrap();
    });
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let (status, body) = api_post(
        &home,
        "/api/v1/credentials/model-key",
        json!({"value":"never-return-this-secret-value"}),
        chats,
        ring.clone(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(status, http::StatusCode::NO_CONTENT);
    assert!(body.is_none());
    let events = ring.lock().await.replay(None);
    let response = serde_json::to_string(&body).unwrap()
        + &serde_json::to_string(
            &events
                .events
                .iter()
                .map(|event| &event.data)
                .collect::<Vec<_>>(),
        )
        .unwrap()
        + &read::host(&home).to_string();
    assert!(!response.contains("never-return-this-secret-value"));
    fs::remove_dir_all(home).unwrap();
}
