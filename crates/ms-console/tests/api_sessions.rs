use ms_console::{
    chat::{ChatManager, submit},
    events::EventRing,
    server::api_post,
};
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::Mutex,
    time::{Duration, timeout},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn home() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ms-console-api-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}
fn session_db(path: &Path, id: &str, status: &str) {
    let db = Connection::open(path.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,kind TEXT,status TEXT);")
        .unwrap();
    db.execute(
        "INSERT INTO sessions VALUES (?1,'lead',?2)",
        rusqlite::params![id, status],
    )
    .unwrap();
}

#[test]
fn chat_manager_rejects_second_session_job_until_first_finishes() {
    let mut jobs = ChatManager::default();
    assert!(jobs.begin("01ARZ3NDEKTSV4RRFFQ69G5FAV", "req-one"));
    assert!(!jobs.begin("01ARZ3NDEKTSV4RRFFQ69G5FAV", "req-two"));
    jobs.finish("01ARZ3NDEKTSV4RRFFQ69G5FAV", "req-one");
    assert!(jobs.begin("01ARZ3NDEKTSV4RRFFQ69G5FAV", "req-two"));
}

#[tokio::test]
async fn new_session_returns_accepted_with_request_and_thread_ids() {
    let home = home();
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let (status, body) = api_post(
        &home,
        "/api/v1/sessions",
        json!({"text":"hello"}),
        chats,
        ring,
    )
    .await
    .unwrap();
    assert_eq!(status, http::StatusCode::ACCEPTED);
    let body = body.unwrap();
    assert!(body["request_id"].as_str().is_some());
    assert!(body["thread_id"].as_str().is_some());
    assert!(body["session_id"].is_null());
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn new_web_session_uses_supervisor_chat_with_web_channel() {
    let home = home();
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        BufReader::new(read).read_line(&mut line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["op"], "chat");
        assert_eq!(request["channel"], "web");
        assert_eq!(request["text"], "hello from console");
        assert!(request["thread_id"].as_str().is_some());
        write
            .write_all(b"{\"session_id\":\"01ARZ3NDEKTSV4RRFFQ69G5FAV\",\"text\":\"hello back\"}\n")
            .await
            .unwrap();
    });
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let mut events = ring.lock().await.subscribe();
    let (status, response) = api_post(
        &home,
        "/api/v1/sessions",
        json!({"text":"hello from console"}),
        chats,
        ring,
    )
    .await
    .unwrap();
    assert_eq!(status, http::StatusCode::ACCEPTED);
    assert!(response.unwrap()["thread_id"].as_str().is_some());
    server.await.unwrap();
    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.name, "chat.reply");
    assert_eq!(event.data["text"], "hello back");
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn second_message_gets_conflict_while_the_console_job_is_active() {
    let home = home();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    session_db(&home, id, "ready");
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    assert!(chats.lock().await.begin(id, "active"));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let error = api_post(
        &home,
        &format!("/api/v1/sessions/{id}/messages"),
        json!({"text":"again"}),
        chats,
        ring,
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, http::StatusCode::CONFLICT);
    assert_eq!(error.1, "session_busy");
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn closed_session_rejects_a_new_message() {
    let home = home();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    session_db(&home, id, "closed");
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let error = api_post(
        &home,
        &format!("/api/v1/sessions/{id}/messages"),
        json!({"text":"again"}),
        chats,
        ring,
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, http::StatusCode::CONFLICT);
    assert_eq!(error.1, "session_closed");
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn end_session_returns_accepted_and_dispatches_to_supervisor() {
    let home = home();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    session_db(&home, id, "ready");
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        BufReader::new(read).read_line(&mut line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request, json!({"op":"end_session","session_id":id}));
        write.write_all(b"{\"ended\":true}\n").await.unwrap();
    });
    let response = api_post(
        &home,
        &format!("/api/v1/sessions/{id}/end"),
        json!({}),
        Arc::new(Mutex::new(ChatManager::default())),
        Arc::new(Mutex::new(EventRing::new(8))),
    )
    .await
    .unwrap();
    assert_eq!(response.0, http::StatusCode::ACCEPTED);
    server.await.unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn pending_approval_resumes_through_wait_and_publishes_final_chat_reply() {
    let home = home();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let fake = tokio::spawn(async move {
        for expected in ["chat", "wait"] {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["op"], expected);
            let response = if expected == "chat" {
                json!({"session_id":id,"pending":true,"after_id":4,"prompt":"approval needed"})
            } else {
                json!({"session_id":id,"text":"completed"})
            };
            write
                .write_all(format!("{}\n", response).as_bytes())
                .await
                .unwrap();
        }
    });
    let manager = Arc::new(Mutex::new(ChatManager::default()));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let mut receiver = ring.lock().await.subscribe();
    let accepted = submit(
        home.clone(),
        Some(id.into()),
        "thread".into(),
        "hello".into(),
        manager.clone(),
        ring,
    )
    .await
    .unwrap();
    assert!(accepted["request_id"].as_str().is_some());
    let first = timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let second = timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.name, "chat.pending");
    assert_eq!(second.name, "chat.reply");
    assert_eq!(second.data["text"], "completed");
    fake.await.unwrap();
    for _ in 0..20 {
        if !manager.lock().await.has_in_flight(id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!manager.lock().await.has_in_flight(id));
    fs::remove_dir_all(home).unwrap();
}
