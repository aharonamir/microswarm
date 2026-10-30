#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

#[test]
fn host_event_stream_merges_all_events_with_gapless_session_sequences() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    assert!(harness.chat(None, &["hello"]).unwrap().status.success());
    let socket = UnixStream::connect(harness.home.join("supervisor.sock")).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(socket.try_clone().unwrap());
    let mut writer = socket;
    writer
        .write_all(b"{\"op\":\"event_stream\",\"follow\":false}\n")
        .unwrap();
    let mut events = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or_default() == 0 {
            break;
        }
        let event: Value = serde_json::from_str(&line).unwrap();
        events.push(event);
    }
    let mut per_session = std::collections::BTreeMap::<String, Vec<i64>>::new();
    let mut merge_order = Vec::new();
    let mut sources = std::collections::BTreeSet::new();
    for event in &events {
        sources.insert(event["source"].as_str().unwrap().to_owned());
        let ts = event["ts"].as_str().unwrap();
        merge_order.push(if ts.contains('.') {
            ts.to_owned()
        } else {
            ts.replace('Z', ".000Z")
        });
        per_session
            .entry(event["session_id"].as_str().unwrap().to_owned())
            .or_default()
            .push(event["seq"].as_i64().unwrap());
    }
    assert!(
        !per_session.is_empty(),
        "event stream should contain session events"
    );
    assert_eq!(
        sources,
        ["agent", "supervisor"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert!(
        merge_order.windows(2).all(|pair| pair[0] <= pair[1]),
        "merged NDJSON order must use the shared wall-clock timestamp"
    );
    for (session_id, sequences) in &per_session {
        assert_eq!(
            sequences,
            &(1..=sequences.len() as i64).collect::<Vec<_>>(),
            "merged sequence should be gapless and in order for {session_id}"
        );
        let db =
            ms_store::open_session_dir(harness.home.join("sessions").join(session_id)).unwrap();
        let agent_count: i64 = db
            .events()
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();
        let supervisor =
            ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
        let supervisor_count: i64 = supervisor
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM supervisor_events WHERE session_id=?1",
                [session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sequences.len() as i64, agent_count + supervisor_count);
    }
    let agent = events
        .iter()
        .find(|event| event["source"] == "agent")
        .unwrap();
    let session_id = agent["session_id"].as_str().unwrap();
    let last = *per_session[session_id].last().unwrap();
    let events_path = harness
        .home
        .join("sessions")
        .join(session_id)
        .join("out/events.db");
    let db = rusqlite::Connection::open(events_path).unwrap();
    let source_seq: i64 = db
        .query_row("SELECT MAX(seq) FROM events", [], |row| row.get(0))
        .unwrap();
    for (seq, claimed_session) in [
        (source_seq + 1, session_id),
        (source_seq + 2, "forged-other-session"),
    ] {
        db.execute("INSERT INTO events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES (?1,'2000-01-01T00:00:00.000Z',1,?2,?3,?4,'agent.started','{}')",
            rusqlite::params![seq, claimed_session, agent["root_session_id"].as_str().unwrap(), agent["agent_role"].as_str().unwrap()]).unwrap();
    }
    let socket = UnixStream::connect(harness.home.join("supervisor.sock")).unwrap();
    let mut reader = BufReader::new(socket.try_clone().unwrap());
    let mut writer = socket;
    let mut after = serde_json::Map::new();
    after.insert(session_id.to_owned(), json!(last));
    writeln!(
        writer,
        "{}",
        json!({"op":"event_stream","follow":false,"after":after})
    )
    .unwrap();
    let mut resumed = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or_default() == 0 {
            break;
        }
        resumed.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    assert_eq!(
        resumed.len(),
        1,
        "late and forged events must not replay or cross sessions"
    );
    assert_eq!(resumed[0]["session_id"], session_id);
    assert_eq!(resumed[0]["seq"], last + 1);
}
