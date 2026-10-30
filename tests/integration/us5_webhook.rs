#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

#[test]
fn webhook_post_routes_to_a_session_and_returns_the_agent_reply() {
    let Some(mut harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let config = harness.home.join("config/microswarm.toml");
    let mut settings = fs::read_to_string(&config).unwrap();
    settings.push_str(&format!(
        "\n[[channels]]\nkind = \"webhook\"\nlisten = \"{address}\"\n"
    ));
    fs::write(config, settings).unwrap();
    harness.restart_supervisor().unwrap();

    let body = r#"{"thread_id":"build-42","text":"hello from webhook"}"#;
    let deadline = Instant::now() + Duration::from_secs(5);
    let response = loop {
        match TcpStream::connect(address) {
            Ok(mut stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                write!(
                    stream,
                    "POST /messages HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                break response;
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => panic!("webhook listener did not start: {error}"),
        }
    };
    let (headers, payload) = response.split_once("\r\n\r\n").unwrap();
    assert!(headers.contains("200 OK"), "{headers}");
    let payload: Value = serde_json::from_str(payload).unwrap();
    assert_eq!(payload["thread_id"], "build-42");
    assert_eq!(payload["text"], "fixture reply");

    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let (channel, thread): (String, String) = store
        .connection()
        .query_row(
            "SELECT channel,channel_thread FROM sessions WHERE kind='lead' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(channel, "webhook");
    assert_eq!(thread, "build-42");
}

#[test]
fn webhook_approval_returns_the_completed_agent_reply() {
    let Some(mut harness) = IntegrationHarness::start_approval_test_or_skip() else {
        return;
    };
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let config = harness.home.join("config/microswarm.toml");
    let mut settings = fs::read_to_string(&config).unwrap();
    settings.push_str(&format!(
        "\n[[channels]]\nkind = \"webhook\"\nlisten = \"{address}\"\n"
    ));
    fs::write(config, settings).unwrap();
    harness.restart_supervisor().unwrap();
    let pending = post_message(address, "approval-thread", "run approval tool");
    assert_eq!(pending["pending"], true);
    let prompt = pending["text"].as_str().unwrap();
    let code = prompt
        .split("/approve ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    let marker = harness.home.join("work/approval-marker");
    assert!(!marker.exists());
    let result = post_message(address, "approval-thread", &format!("/approve {code}"));
    assert_eq!(result["text"], "fixture reply");
    assert!(marker.exists());
}

fn post_message(address: std::net::SocketAddr, thread_id: &str, text: &str) -> Value {
    let body = serde_json::json!({"thread_id":thread_id,"text":text}).to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(address) {
            Ok(mut stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(20)))
                    .unwrap();
                write!(stream, "POST /messages HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                let (headers, payload) = response.split_once("\r\n\r\n").unwrap();
                assert!(headers.contains("200 OK"), "{headers}: {payload}");
                return serde_json::from_str(payload).unwrap();
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => panic!("webhook listener did not start: {error}"),
        }
    }
}
