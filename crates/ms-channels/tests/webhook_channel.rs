use ms_channels::webhook::WebhookAdapter;
use serde_json::json;
use std::{fs, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UnixListener},
};

#[test]
fn webhook_adapter_maps_threaded_messages_and_replies() {
    let adapter = WebhookAdapter::new();
    let inbound = adapter
        .parse_inbound(&json!({"thread_id":"ticket-42","text":"please investigate","sender":"ci"}))
        .unwrap();
    assert_eq!(inbound.thread_id, "ticket-42");
    assert_eq!(inbound.message.body["text"], "please investigate");
    let response = adapter.reply_payload("ticket-42", "investigation complete");
    assert_eq!(response["thread_id"], "ticket-42");
    assert_eq!(response["text"], "investigation complete");
}

#[tokio::test]
async fn webhook_http_adapter_routes_a_message_and_returns_the_reply() {
    let home = std::env::temp_dir().join(format!("ms-webhook-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join("credentials")).unwrap();
    fs::write(home.join("credentials/webhook-key"), "fixture-secret\n").unwrap();
    let supervisor_path = home.join("supervisor.sock");
    let supervisor = UnixListener::bind(&supervisor_path).unwrap();
    let supervisor_task = tokio::spawn(async move {
        let (stream, _) = supervisor.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);
        let mut request = String::new();
        reader.read_line(&mut request).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["op"], "channel_message");
        assert_eq!(request["channel"], "webhook");
        assert_eq!(request["thread_id"], "ticket-9");
        assert_eq!(request["text"], "please investigate");
        write
            .write_all(b"{\"text\":\"investigation complete\"}\n")
            .await
            .unwrap();
    });
    let available = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = available.local_addr().unwrap();
    drop(available);
    let server_home = home.clone();
    let server_socket = supervisor_path.clone();
    let listen = address.to_string();
    let server = tokio::spawn(async move {
        ms_channels::webhook::run(
            server_home,
            server_socket,
            listen,
            Some("webhook-key".into()),
        )
        .await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = reqwest::Client::new();
    let url = format!("http://{address}/messages");
    let payload = json!({"thread_id":"ticket-9","text":"please investigate"});
    let unauthorized = client.post(&url).json(&payload).send().await.unwrap();
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
    let response = client
        .post(&url)
        .bearer_auth("fixture-secret")
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = response.json().await.unwrap();
    assert_eq!(payload["thread_id"], "ticket-9");
    assert_eq!(payload["text"], "investigation complete");
    supervisor_task.await.unwrap();
    server.abort();
    fs::remove_dir_all(home).unwrap();
}
