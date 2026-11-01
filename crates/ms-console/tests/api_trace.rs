use ms_console::read;
use rusqlite::Connection;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
#[test]
fn trace_endpoint_events_match_shared_trace_builder() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-trace-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,role TEXT,status TEXT,scope_json TEXT,created_at TEXT); CREATE TABLE supervisor_events(seq INTEGER,ts TEXT,ts_mono INTEGER,session_id TEXT,root_session_id TEXT,agent_role TEXT,type TEXT,data TEXT);").unwrap();
    db.execute(
        "INSERT INTO sessions VALUES(?1,?1,NULL,'lead','ready',NULL,'2026-01-01')",
        [id],
    )
    .unwrap();
    db.execute("INSERT INTO supervisor_events VALUES(1,'2026-01-01T00:00:00Z',1,?1,?1,'lead','session.created','{}')", [id]).unwrap();
    drop(db);
    let expected = ms_runtime::trace::build(&home, id, None).unwrap();
    let result = read::trace(&home, id).unwrap();
    assert_eq!(
        result["events"],
        serde_json::to_value(expected.events).unwrap()
    );
    fs::remove_dir_all(home).unwrap();
}
