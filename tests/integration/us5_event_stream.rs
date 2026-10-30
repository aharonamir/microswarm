#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use serde_json::Value;
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
    for event in events {
        sources.insert(event["source"].as_str().unwrap().to_owned());
        merge_order.push((
            event["ts_mono"].as_i64().unwrap(),
            event["source"].as_str().unwrap().to_owned(),
            event["seq"].as_i64().unwrap(),
        ));
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
        "merged NDJSON order must be stable by timestamp, source, and sequence"
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
}
