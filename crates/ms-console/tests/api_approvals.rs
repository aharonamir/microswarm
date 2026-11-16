use ms_console::{chat::ChatManager, events::EventRing, read, server::api_post};
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::Mutex,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const ROOT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const MEMBER: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
fn home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "ms-console-approval-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    home
}
fn fixture() -> PathBuf {
    let home = home();
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,role TEXT,kind TEXT,channel TEXT,channel_thread TEXT,status TEXT,scope_hash TEXT,scope_json TEXT,created_at TEXT,last_active_at TEXT,closed_at TEXT,close_reason TEXT); CREATE TABLE approvals(id TEXT,code TEXT,root_session_id TEXT,requesting_session_id TEXT,kind TEXT,payload TEXT,status TEXT,expires_at TEXT,created_at TEXT,resolved_at TEXT,reissued_as TEXT);").unwrap();
    db.execute("INSERT INTO sessions(id,root_id,role,kind,status,scope_json) VALUES(?1,?1,'lead','lead','waiting_approval','{}'),(?2,?1,'tester','member','waiting_approval','{\"mounts\":[{\"path\":\"/work\",\"mode\":\"rw\"}]}')", rusqlite::params![ROOT, MEMBER]).unwrap();
    db.execute("INSERT INTO approvals VALUES('approval-1','A1B2',?1,?2,'tool_call',?3,'pending','2099-01-01','2026-01-01',NULL,NULL)", rusqlite::params![ROOT, MEMBER, json!({"binding":{"server":"microswarm-tools","tool":"shell.run","argument_hash":"sha256:abcd"},"arguments":{"command":"cargo test --workspace"}}).to_string()]).unwrap();
    home
}

#[test]
fn approval_views_include_exact_binding_args_and_parsed_command() {
    let home = fixture();
    let list = read::approvals(&home, "pending").unwrap();
    assert_eq!(list["approvals"].as_array().unwrap().len(), 1);
    let detail = read::approval(&home, "A1B2").unwrap()["approval"].clone();
    assert_eq!(detail["tool_call"]["server"], "microswarm-tools");
    assert_eq!(
        detail["tool_call"]["arguments"]["command"],
        "cargo test --workspace"
    );
    assert_eq!(detail["tool_call"]["parsed"]["kind"], "simple");
    assert_eq!(detail["requesting_role"], "tester");
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn approval_decision_sends_code_only_to_supervisor() {
    let home = home();
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        BufReader::new(read).read_line(&mut line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request, json!({"op":"approve","code":"A1B2"}));
        write
            .write_all(b"{\"decision\":\"approved\"}\n")
            .await
            .unwrap();
    });
    let result = api_post(
        &home,
        "/api/v1/approvals/A1B2/approve",
        json!({"anything":"ignored"}),
        Arc::new(Mutex::new(ChatManager::default())),
        Arc::new(Mutex::new(EventRing::new(8))),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(result.0, http::StatusCode::OK);
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn approval_decisions_report_resolved_conflict_and_expiry_gone() {
    let home = home();
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let server = tokio::spawn(async move {
        for error in ["already resolved", "approval expired"] {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            BufReader::new(read).read_line(&mut line).await.unwrap();
            write
                .write_all(
                    format!("{{\"error\":{}}}\n", serde_json::to_string(error).unwrap()).as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let chats = Arc::new(Mutex::new(ChatManager::default()));
    let ring = Arc::new(Mutex::new(EventRing::new(8)));
    let conflict = api_post(
        &home,
        "/api/v1/approvals/A1B2/approve",
        json!({}),
        chats.clone(),
        ring.clone(),
    )
    .await
    .unwrap_err();
    let expired = api_post(&home, "/api/v1/approvals/A1B2/deny", json!({}), chats, ring)
        .await
        .unwrap_err();
    assert_eq!(conflict.0, http::StatusCode::CONFLICT);
    assert_eq!(expired.0, http::StatusCode::GONE);
    server.await.unwrap();
    fs::remove_dir_all(home).unwrap();
}
