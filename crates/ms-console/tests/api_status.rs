use ms_console::{events::EventRing, rpc};
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::Mutex,
    time::{Instant, timeout},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn supervisor_commit_reaches_status_stream_within_500ms() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-status-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let db = Connection::open(home.join("supervisor.db")).unwrap();
    db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,kind TEXT,role TEXT,channel TEXT,channel_thread TEXT,status TEXT,created_at TEXT,last_active_at TEXT,closed_at TEXT,close_reason TEXT,scope_json TEXT); CREATE TABLE approvals(id TEXT,code TEXT,root_session_id TEXT,requesting_session_id TEXT,kind TEXT,payload TEXT,status TEXT,expires_at TEXT,created_at TEXT,resolved_at TEXT,reissued_as TEXT); CREATE TABLE delegations(id TEXT,lead_id TEXT,status TEXT); CREATE TABLE supervisor_events(seq INTEGER,session_id TEXT,type TEXT,data TEXT);").unwrap();
    db.execute("INSERT INTO sessions(id,root_id,kind,role,status,created_at,last_active_at) VALUES(?1,?1,'lead','lead','busy','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')", [id]).unwrap();
    drop(db);
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let ring = Arc::new(Mutex::new(EventRing::new(16)));
    let mut receiver = ring.lock().await.subscribe();
    let follower = tokio::spawn(rpc::follow(home.clone(), ring));
    let (stream, _) = timeout(Duration::from_secs(1), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let request = timeout(Duration::from_secs(1), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&request).unwrap()["op"],
        "event_stream"
    );
    let started = Instant::now();
    write.write_all(format!("{}\n", json!({"seq":1,"session_id":id,"root_session_id":id,"source":"supervisor","ts":"2026-11-01T00:00:00Z","type":"turn.started","data":{}})).as_bytes()).await.unwrap();
    let mut got_status = false;
    while !got_status {
        let event = timeout(Duration::from_millis(500), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        got_status = event.name == "status.changed";
    }
    let elapsed = started.elapsed();
    println!(
        "status.changed arrived in {} µs after the supervisor event was written",
        elapsed.as_micros()
    );
    assert!(elapsed < Duration::from_millis(500));
    follower.abort();
    fs::remove_dir_all(home).unwrap();
}
