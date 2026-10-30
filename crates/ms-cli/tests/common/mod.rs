use rusqlite::Connection;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
pub const ROOT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
pub const MEMBER: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";

pub fn home() -> PathBuf {
    std::env::temp_dir().join(format!(
        "ms-trace-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

pub fn fixture() -> PathBuf {
    let home = home();
    fs::create_dir_all(&home).unwrap();
    let db = ms_store::open_supervisor_db(home.join("supervisor.db")).unwrap();
    for (id, parent, role, kind, scope) in [
        (
            ROOT,
            None,
            "lead",
            "lead",
            r#"{"mounts":[{}],"tools":[{}],"credentials":["key"]}"#,
        ),
        (
            MEMBER,
            Some(ROOT),
            "researcher",
            "member",
            r#"{"mounts":[{}],"tools":[{}],"credentials":[]}"#,
        ),
    ] {
        db.execute("INSERT INTO sessions(id,root_id,parent_id,role,kind,status,scope_hash,scope_json,created_at,last_active_at) VALUES (?1,?2,?3,?4,?5,'closed','hash',?6,'2026-10-01T00:00:00Z','2026-10-01T00:00:00Z')", rusqlite::params![id, ROOT, parent, role, kind, scope]).unwrap();
    }
    db.execute("INSERT INTO supervisor_events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES (1,'2026-10-01T00:00:00Z',1,?1,?1,'lead','session.created','{}'),(2,'2026-10-01T00:00:01Z',2,?2,?1,'lead','delegation.spawned','{}')", rusqlite::params![ROOT, MEMBER]).unwrap();
    drop(db);
    let events = home.join("sessions").join(ROOT).join("out/events.db");
    fs::create_dir_all(events.parent().unwrap()).unwrap();
    let agent = Connection::open(events).unwrap();
    agent.execute_batch("CREATE TABLE events(seq INTEGER PRIMARY KEY,ts TEXT NOT NULL,ts_mono INTEGER NOT NULL,session_id TEXT NOT NULL,root_session_id TEXT NOT NULL,agent_role TEXT NOT NULL,type TEXT NOT NULL,data TEXT NOT NULL);").unwrap();
    agent.execute("INSERT INTO events VALUES (1,'2026-10-01T00:00:02Z',3,?1,?1,'lead','tool.call','{\"call_id\":\"call1\",\"server\":\"microswarm-tools\",\"tool\":\"fs.read\",\"args\":{}}')", [ROOT]).unwrap();
    home
}

pub fn trace(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_microswarm"))
        .args(["trace", ROOT, "--home", home.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}

pub fn cleanup(home: &std::path::Path) {
    let _ = fs::remove_dir_all(home);
}
