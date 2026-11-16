use ms_console::read;
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const LEAD: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const MEMBER: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
fn home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "ms-console-scope-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    home
}
fn scope(id: &str) -> serde_json::Value {
    json!({"version":1,"session_id":id,"derived_from":"delegation","source_ref":"delegation-1","mounts":[],"credentials":[],"tools":[],"delegation":null,"limits":{"memory_mb":128,"pids":32,"tmp_mb":32,"max_turns":20,"max_tokens":4000},"narrowed":[],"hash":"sha256:test"})
}
#[test]
fn member_scope_comparison_and_lead_profile_match_stored_values() {
    let home = home();
    fs::create_dir_all(home.join("sessions").join(LEAD).join("control")).unwrap();
    fs::create_dir_all(home.join("sessions").join(LEAD).join("out")).unwrap();
    fs::create_dir_all(home.join("sessions").join(MEMBER).join("control")).unwrap();
    fs::create_dir_all(home.join("sessions").join(MEMBER).join("out")).unwrap();
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,role TEXT,scope_json TEXT); CREATE TABLE delegations(member_id TEXT,scope_request TEXT);").unwrap();
    db.execute(
        "INSERT INTO sessions VALUES(?1,?1,NULL,'lead',?2)",
        rusqlite::params![LEAD, scope(LEAD).to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO sessions VALUES(?1,?2,?2,'reviewer',?3)",
        rusqlite::params![MEMBER, LEAD, scope(MEMBER).to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO delegations VALUES(?1,?2)",
        rusqlite::params![MEMBER, json!({"mounts":[]}).to_string()],
    )
    .unwrap();
    drop(db);
    let result = read::scope(&home, MEMBER, true).unwrap();
    assert_eq!(result["rows"][0]["granted"], json!([]));
    assert_eq!(result["rows"][0]["requested"], json!([]));
    assert_eq!(result["scope_hash"], "sha256:test");
    let lead = read::scope(&home, LEAD, false).unwrap();
    assert_eq!(lead["granted"]["limits"]["memory_mb"], 128);
    assert_eq!(lead["profile"]["filesystem"]["root_read_only"], true);
    assert!(read::scope(&home, LEAD, true).is_err());
    fs::remove_dir_all(home).unwrap();
}
