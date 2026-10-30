#[path = "us3_support.rs"]
mod support;

use std::{thread, time::{Duration, Instant}};

#[test]
#[ignore = "red: T076 — boot does not adopt running sessions"]
fn restart_adopts_running_lead_and_member_without_orphans() {
    let Some(mut harness) = support::start_with_skill() else { return };
    harness.chat(None, &["spawn researcher"]).unwrap();
    let store = ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let ids: Vec<String> = {
        let mut stmt = store.connection().prepare("SELECT id FROM sessions WHERE status='ready' ORDER BY kind").unwrap();
        stmt.query_map([], |row| row.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(ids.len(), 2);
    harness.stop_supervisor_for_test();
    harness.restart_supervisor().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let adopted: i64 = store.connection().query_row("SELECT COUNT(*) FROM supervisor_events WHERE type='container.adopted' AND session_id IN (?1,?2)", [&ids[0], &ids[1]], |row| row.get(0)).unwrap();
        if adopted == 2 { break }
        assert!(Instant::now() < until, "expected both containers adopted, got {adopted}");
        thread::sleep(Duration::from_millis(25));
    }
    for id in &ids {
        assert_eq!(support::session_state(&harness.home, id).0, "ready");
        let container = harness.containers_for(id).unwrap().remove(0);
        assert!(harness.container_running(&container).unwrap());
    }
    let orphans: i64 = store.connection().query_row("SELECT COUNT(*) FROM supervisor_events WHERE type='container.orphan_stopped'", [], |row| row.get(0)).unwrap();
    assert_eq!(orphans, 0);
}
