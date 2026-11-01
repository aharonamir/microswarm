use ms_console::read::{self, valid_session_id};
use rusqlite::Connection;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn home() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ms-console-read-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn session_reads_use_database_authority_and_return_status_words() {
    let home = home();
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,role TEXT,kind TEXT,channel TEXT,channel_thread TEXT,status TEXT,scope_hash TEXT,scope_json TEXT,created_at TEXT,last_active_at TEXT,closed_at TEXT,close_reason TEXT); CREATE TABLE approvals(id TEXT,code TEXT,root_session_id TEXT,requesting_session_id TEXT,kind TEXT,payload TEXT,status TEXT,expires_at TEXT,created_at TEXT,resolved_at TEXT,reissued_as TEXT); CREATE TABLE delegations(id TEXT,lead_id TEXT,status TEXT); CREATE TABLE supervisor_events(seq INTEGER,session_id TEXT,type TEXT,data TEXT);").unwrap();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    db.execute("INSERT INTO sessions(id,root_id,role,kind,status,created_at,last_active_at) VALUES (?1,?1,'lead','lead','ready',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",[id]).unwrap();
    drop(db);
    let result = read::session(&home, id).unwrap();
    assert_eq!(result["session"]["id"], id);
    assert_eq!(
        result["session"]["status_words"]["en"],
        "Ready for your next message"
    );
    assert!(
        read::sessions(&home, "active").unwrap()["sessions"]
            .as_array()
            .unwrap()
            .len()
            == 1
    );
    let status = read::status_event(
        &home,
        &serde_json::json!({"session_id":id,"ts":"2026-11-01T00:00:00Z"}),
    )
    .unwrap();
    assert_eq!(status["type"], "status.changed");
    assert_eq!(status["status_words"]["key"], "status.ready");
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn session_id_is_checked_before_any_database_or_path_access() {
    assert!(valid_session_id("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    assert!(!valid_session_id("../../etc/passwd"));
    let absent = PathBuf::from("/path/that/does/not/exist");
    assert!(
        read::session(&absent, "../../etc/passwd")
            .unwrap_err()
            .contains("invalid session id")
    );
}
