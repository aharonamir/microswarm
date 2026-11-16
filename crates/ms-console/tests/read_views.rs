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
    let busy_id = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
    let ended_id = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
    db.execute("INSERT INTO sessions(id,root_id,role,kind,status,created_at,last_active_at) VALUES (?1,?1,'lead','lead','busy',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",[busy_id]).unwrap();
    db.execute("INSERT INTO sessions(id,root_id,role,kind,status,created_at,last_active_at,closed_at) VALUES (?1,?1,'lead','lead','closed',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),datetime('now'))",[ended_id]).unwrap();
    drop(db);
    let result = read::session(&home, id).unwrap();
    assert_eq!(result["session"]["id"], id);
    assert_eq!(
        result["session"]["status_words"]["en"],
        "Ready for your next message"
    );
    assert_eq!(
        read::session(&home, busy_id).unwrap()["session"]["in_flight"],
        true
    );
    assert!(
        read::sessions(&home, "active").unwrap()["sessions"]
            .as_array()
            .unwrap()
            .len()
            == 2
    );
    assert_eq!(
        read::sessions(&home, "ended").unwrap()["sessions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(read::sessions(&home, "invalid").is_err());
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

#[test]
fn message_pages_use_a_stable_cursor_across_sources() {
    let home = home();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let supervisor = Connection::open(home.join("supervisor.db")).unwrap();
    supervisor.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,role TEXT,kind TEXT,channel TEXT,channel_thread TEXT,status TEXT,scope_hash TEXT,scope_json TEXT,created_at TEXT,last_active_at TEXT,closed_at TEXT,close_reason TEXT);").unwrap();
    drop(supervisor);
    let session = home.join("sessions").join(id);
    fs::create_dir_all(session.join("control")).unwrap();
    fs::create_dir_all(session.join("out")).unwrap();
    let inbound = Connection::open(session.join("control/inbound.db")).unwrap();
    inbound
        .execute_batch(
            "CREATE TABLE inbound(id INTEGER PRIMARY KEY,kind TEXT,body TEXT,created_at TEXT);",
        )
        .unwrap();
    inbound.execute("INSERT INTO inbound(kind,body,created_at) VALUES('user_message','{\"text\":\"one\"}','2026-01-01T00:00:00Z'),('user_message','{\"text\":\"two\"}','2026-01-01T00:00:00Z')", []).unwrap();
    drop(inbound);
    let outbound = Connection::open(session.join("out/outbound.db")).unwrap();
    outbound
        .execute_batch(
            "CREATE TABLE outbound(id INTEGER PRIMARY KEY,kind TEXT,body TEXT,created_at TEXT);",
        )
        .unwrap();
    outbound.execute("INSERT INTO outbound(kind,body,created_at) VALUES('reply','{\"text\":\"three\"}','2026-01-01T00:00:00Z')", []).unwrap();
    drop(outbound);

    let newer = read::messages(&home, id, None, 2).unwrap();
    assert_eq!(newer["items"][0]["text"], "two");
    assert_eq!(newer["items"][1]["text"], "three");
    let cursor = newer["next_before"].as_str().unwrap();
    let older = read::messages(&home, id, Some(cursor), 2).unwrap();
    assert_eq!(older["items"].as_array().unwrap().len(), 1);
    assert_eq!(older["items"][0]["text"], "one");
    assert!(older["next_before"].is_null());
    assert!(read::messages(&home, id, Some("bad-cursor"), 2).is_err());
    fs::remove_dir_all(home).unwrap();
}
