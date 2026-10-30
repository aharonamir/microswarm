#[path = "us3_support.rs"]
mod support;

use std::{
    thread,
    time::{Duration, Instant},
};

#[test]
fn restart_stops_container_with_closed_session_label() {
    let Some(mut harness) = support::start() else {
        return;
    };
    harness.chat(None, &["hello"]).unwrap();
    let lead = support::lead_id(&harness.home);
    let container = harness.containers_for(&lead).unwrap().remove(0);
    harness.stop_supervisor_for_test();
    let store = ms_store::SupervisorStore::open(harness.home.join("supervisor.db")).unwrap();
    store.connection().execute("UPDATE sessions SET status='closed',closed_at='2000-01-01T00:00:00Z',close_reason='session_end' WHERE id=?1", [&lead]).unwrap();
    harness.restart_supervisor().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let event: i64 = store.connection().query_row("SELECT COUNT(*) FROM supervisor_events WHERE type='container.orphan_stopped' AND session_id=?1", [&lead], |row| row.get(0)).unwrap();
        if event == 1 {
            break;
        }
        assert!(Instant::now() < until, "orphan was not stopped");
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!harness.container_running(&container).unwrap());
}
