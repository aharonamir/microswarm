//! Reproducible idle event_stream cost fixture for console T203.
use rusqlite::{Connection, params};
use serde_json::json;
use std::{
    env, fs,
    path::Path,
    time::{Duration, Instant},
};

fn fixture(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(home.join("sessions"))?;
    let mut supervisor = Connection::open(home.join("supervisor.db"))?;
    supervisor.execute_batch("CREATE TABLE sessions (id TEXT, root_id TEXT, role TEXT); CREATE TABLE supervisor_events (seq INTEGER PRIMARY KEY, ts TEXT, ts_mono INTEGER, session_id TEXT, root_session_id TEXT, agent_role TEXT, type TEXT, data TEXT);")?;
    let transaction = supervisor.transaction()?;
    for index in 0..50 {
        let id = format!("fixture-{index:02}");
        transaction.execute("INSERT INTO sessions VALUES (?1,?1,'lead')", [&id])?;
        let out = home.join("sessions").join(&id).join("out");
        fs::create_dir_all(&out)?;
        let mut events = Connection::open(out.join("events.db"))?;
        events.execute_batch("CREATE TABLE events (seq INTEGER PRIMARY KEY, ts TEXT, ts_mono INTEGER, session_id TEXT, root_session_id TEXT, agent_role TEXT, type TEXT, data TEXT);")?;
        let event_tx = events.transaction()?;
        for seq in 1..=400 {
            let ts = format!("2026-11-01T00:{:02}:{:02}.000Z", seq / 60, seq % 60);
            event_tx.execute(
                "INSERT INTO events VALUES (?1,?2,?3,?4,?4,'lead','turn.started',?5)",
                params![seq, ts, seq, id, json!({"fixture":true}).to_string()],
            )?;
        }
        event_tx.commit()?;
    }
    transaction.commit()?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = env::temp_dir().join(format!("ms-event-stream-cost-{}", std::process::id()));
    if home.exists() {
        fs::remove_dir_all(&home)?;
    }
    fixture(&home)?;
    let seconds = env::args()
        .nth(1)
        .map_or(60, |s| s.parse::<u64>().unwrap_or(60));
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(seconds),
        ms_runtime::events::stream(&home, &json!({"follow":true}), &mut tokio::io::sink()),
    )
    .await;
    println!(
        "fixture: 50 sessions, 20,000 agent events; elapsed={:.2}s; stream={}",
        started.elapsed().as_secs_f64(),
        match result {
            Err(_) => "timed out as expected",
            Ok(Ok(())) => "ended",
            Ok(Err(_)) => "error",
        }
    );
    fs::remove_dir_all(&home)?;
    Ok(())
}
