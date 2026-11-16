use ms_console::{events::ConsoleEvent, server::encode_sse_event};
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn hostile_agent_strings_remain_inert_json_in_api_and_sse_payloads() {
    let payloads = [
        "<script>alert(1)</script>",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
    ];
    let event = ConsoleEvent {
        id: 7,
        name: "agent".into(),
        data: json!({"type":"agent","event":{"data":{"text":payloads}}}),
    };
    let api_body = serde_json::to_string(&event.data).unwrap();
    let frame = encode_sse_event(&event);
    for payload in payloads {
        assert!(api_body.contains(payload));
        assert!(frame.contains(payload));
    }
    assert!(frame.starts_with("id: 7\nevent: agent\ndata: {"));
}

#[test]
fn hostile_lead_message_is_returned_as_json_text_and_sse_data() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-xss-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let inbound = home.join("sessions").join(id).join("control");
    fs::create_dir_all(&inbound).unwrap();
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    drop(db);
    let db = Connection::open(inbound.join("inbound.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE inbound(id INTEGER PRIMARY KEY,kind TEXT,body TEXT,created_at TEXT);",
    )
    .unwrap();
    let malicious = "<script>alert(1)</script> javascript:alert(1) data:text/html,<script>";
    db.execute(
        "INSERT INTO inbound(kind,body,created_at) VALUES ('user_message',?1,'2026-11-01T00:00:00Z')",
        [json!({"text":malicious}).to_string()],
    )
    .unwrap();
    drop(db);
    let response = ms_console::read::messages(&home, id, None, 100).unwrap();
    assert_eq!(response["items"][0]["text"], malicious);
    let frame = encode_sse_event(&ConsoleEvent {
        id: 9,
        name: "chat.reply".into(),
        data: json!({"type":"chat.reply","text":response["items"][0]["text"]}),
    });
    assert!(frame.contains(malicious));
    fs::remove_dir_all(home).unwrap();
}
