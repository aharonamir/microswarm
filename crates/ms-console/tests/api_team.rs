use ms_console::read;
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
#[test]
fn team_activity_marks_agent_events_as_observations() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-team-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,created_at TEXT); CREATE TABLE supervisor_events(seq INTEGER,ts_mono INTEGER,session_id TEXT,root_session_id TEXT,agent_role TEXT,type TEXT,data TEXT,ts TEXT);").unwrap();
    db.execute("INSERT INTO sessions VALUES(?1,?1,'2026-01-01')", [id])
        .unwrap();
    db.execute("INSERT INTO supervisor_events VALUES(1,10,?1,?1,'lead','session.created','{}','2026-01-01T00:00:00Z')", [id]).unwrap();
    drop(db);
    let event_dir = home.join("sessions").join(id).join("out");
    fs::create_dir_all(&event_dir).unwrap();
    let agent = Connection::open(event_dir.join("events.db")).unwrap();
    agent.execute_batch("CREATE TABLE events(seq INTEGER,ts_mono INTEGER,agent_role TEXT,type TEXT,data TEXT,ts TEXT);").unwrap();
    agent
        .execute(
            "INSERT INTO events VALUES(2,20,'lead','tool.result',?1,'2026-01-01T00:00:01Z')",
            [json!({"text":"command completed"}).to_string()],
        )
        .unwrap();
    drop(agent);
    let result = read::recent_activity(&home, id).unwrap();
    assert_eq!(result["events"][0]["type"], "tool.result");
    assert_eq!(result["events"][0]["observation"], true);
    assert_eq!(result["events"][1]["observation"], false);
    fs::remove_dir_all(home).unwrap();
}
