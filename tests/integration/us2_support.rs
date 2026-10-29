#![allow(dead_code)]

#[path = "harness.rs"]
pub mod harness;

use harness::IntegrationHarness;
use ms_store::SupervisorStore;
use std::path::PathBuf;

pub fn start_with_skill() -> Option<IntegrationHarness> {
    let harness = IntegrationHarness::start_swarm_or_skip()?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/us2-team");
    let path = fixture.to_str().expect("fixture path is UTF-8");
    let output = harness
        .run_cli(&["skills", "install", path])
        .expect("run skills install");
    assert!(
        output.status.success(),
        "install team skill failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(harness)
}

pub fn members(harness: &IntegrationHarness) -> Vec<(String, String, Option<String>)> {
    let store = SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let mut statement = store
        .connection()
        .prepare("SELECT id, role, parent_id FROM sessions WHERE kind='member' ORDER BY created_at")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
