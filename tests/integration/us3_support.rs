#![allow(dead_code)]

#[path = "harness.rs"]
pub mod harness;

use harness::IntegrationHarness;
use ms_store::SupervisorStore;
use serde_json::Value;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub fn start() -> Option<IntegrationHarness> {
    IntegrationHarness::start_swarm_or_skip()
}

pub fn start_with_skill() -> Option<IntegrationHarness> {
    let harness = start()?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/us2-team");
    let output = harness
        .run_cli(&["skills", "install", path.to_str().unwrap()])
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(harness)
}

pub fn lead_id(home: &Path) -> String {
    SupervisorStore::open_readonly(home.join("supervisor.db"))
        .unwrap()
        .connection()
        .query_row(
            "SELECT id FROM sessions WHERE kind='lead' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

pub fn wait_for_lead_id(home: &Path, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let result =
            SupervisorStore::open_readonly(home.join("supervisor.db")).and_then(|store| {
                store.connection().query_row(
                "SELECT id FROM sessions WHERE kind='lead' ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            ).map_err(ms_store::StoreError::from)
            });
        if let Ok(id) = result {
            return id;
        }
        assert!(Instant::now() < deadline, "no lead session was created");
        thread::sleep(Duration::from_millis(20));
    }
}

pub fn session_state(home: &Path, id: &str) -> (String, u8, Option<String>) {
    let store = SupervisorStore::open_readonly(home.join("supervisor.db")).unwrap();
    store
        .connection()
        .query_row(
            "SELECT status,restart_count,container_id FROM sessions WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
}

pub fn wait_for_status(home: &Path, id: &str, expected: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if session_state(home, id).0 == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "session {id} did not reach {expected}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn wait_for_inbound(home: &Path, id: &str, kind: &str, timeout: Duration) -> Value {
    let session = ms_store::open_session_dir(home.join("sessions").join(id)).unwrap();
    let deadline = Instant::now() + timeout;
    loop {
        let result = session.inbound().query_row(
            "SELECT body FROM inbound WHERE kind=?1 ORDER BY id DESC LIMIT 1",
            [kind],
            |row| row.get::<_, String>(0),
        );
        if let Ok(body) = result {
            return serde_json::from_str(&body).unwrap();
        }
        assert!(Instant::now() < deadline, "lead did not receive {kind}");
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn runtime_name(harness: &IntegrationHarness) -> &'static str {
    match harness.runtime {
        harness::Runtime::Docker => "docker",
        harness::Runtime::Podman => "podman",
    }
}
