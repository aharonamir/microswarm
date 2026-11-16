use ms_console::{
    events::{ConsoleEvent, EventRing},
    rpc::SupervisorLink,
    server::SSE_PING_INTERVAL,
};
use serde_json::json;
use std::time::Duration;

#[test]
fn event_ring_replays_after_id_and_resyncs_outside_capacity() {
    let mut ring = EventRing::new(2);
    let mut client_a = ring.subscribe();
    let mut client_b = ring.subscribe();
    ring.push(ConsoleEvent {
        id: 1,
        name: "agent".into(),
        data: json!({}),
    });
    assert_eq!(client_a.try_recv().unwrap().id, 1);
    assert_eq!(client_b.try_recv().unwrap().id, 1);
    ring.push(ConsoleEvent {
        id: 2,
        name: "agent".into(),
        data: json!({}),
    });
    ring.push(ConsoleEvent {
        id: 3,
        name: "agent".into(),
        data: json!({}),
    });
    assert_eq!(
        ring.replay(Some(2))
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [3]
    );
    assert!(ring.replay(Some(0)).resync);
    assert_eq!(SSE_PING_INTERVAL, Duration::from_secs(15));
}

#[test]
fn supervisor_link_emits_down_and_up_only_on_reconnect_transitions() {
    let mut link = SupervisorLink::default();
    assert_eq!(link.set_connected(true), None);
    assert_eq!(link.set_connected(false), Some("supervisor.down"));
    assert_eq!(link.set_connected(false), None);
    assert_eq!(link.set_connected(true), Some("supervisor.up"));
}
