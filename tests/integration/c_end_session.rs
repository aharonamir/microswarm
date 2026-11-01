#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
};

fn rpc(home: &std::path::Path, request: Value) -> Value {
    let mut stream = UnixStream::connect(home.join("supervisor.sock")).unwrap();
    stream
        .write_all(format!("{}\n", request).as_bytes())
        .unwrap();
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).unwrap();
    serde_json::from_str(&response).unwrap()
}

#[test]
fn ends_lead_and_emits_operator_close() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let output = harness.chat(None, &["end-session fixture"]).unwrap();
    assert!(output.status.success());
    let session_id = harness.session_dirs().unwrap()[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let result = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":session_id}),
    );
    assert_eq!(result["status"], "closed");
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let event: String = store.connection().query_row("SELECT data FROM supervisor_events WHERE type='session.closed' ORDER BY seq DESC LIMIT 1", [], |row| row.get(0)).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&event).unwrap()["by"],
        "operator"
    );
}

fn lead(harness: &IntegrationHarness) -> String {
    let output = harness.chat(None, &["end-session fixture"]).unwrap();
    assert!(output.status.success());
    harness.session_dirs().unwrap()[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn rejects_member_id_and_closed_lead() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let lead_id = lead(&harness);
    let member_id = "01J00000000000000000000001";
    let db = rusqlite::Connection::open(harness.home.join("supervisor.db")).unwrap();
    db.execute("INSERT INTO sessions(id,root_id,parent_id,role,kind,channel,status,created_at,last_active_at) VALUES (?1,?2,?2,'tester','member',NULL,'closed','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')", rusqlite::params![member_id,lead_id]).unwrap();
    let member = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":member_id}),
    );
    assert_eq!(member["error"], "not_a_lead");
    let ended = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":lead_id}),
    );
    assert_eq!(ended["status"], "closed");
    let again = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":lead_id}),
    );
    assert_eq!(again["error"], "already_closed");
}

#[test]
fn denies_pending_approvals_in_session_tree() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let lead_id = lead(&harness);
    let db = rusqlite::Connection::open(harness.home.join("supervisor.db")).unwrap();
    db.execute("INSERT INTO approvals(id,code,root_session_id,requesting_session_id,kind,payload,status,expires_at,created_at) VALUES ('approval-fixture','A1B2',?1,?1,'tool_call','{}','pending','2099-01-01T00:00:00Z','2026-11-01T00:00:00Z')", [&lead_id]).unwrap();
    let ended = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":lead_id}),
    );
    assert_eq!(ended["status"], "closed");
    let status: String = db
        .query_row(
            "SELECT status FROM approvals WHERE id='approval-fixture'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "denied");
}

#[test]
fn retries_session_left_stopping_after_partial_failure() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let lead_id = lead(&harness);
    let db = rusqlite::Connection::open(harness.home.join("supervisor.db")).unwrap();
    db.execute(
        "UPDATE sessions SET status='stopping' WHERE id=?1",
        [&lead_id],
    )
    .unwrap();
    let ended = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":lead_id}),
    );
    assert_eq!(ended["status"], "closed");
}

#[test]
fn closes_members_before_the_lead() {
    let Some(harness) = IntegrationHarness::start_swarm_or_skip() else {
        return;
    };
    let skill = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/skills/code-review-team");
    let installed = harness
        .run_cli(&["skills", "install", skill.to_str().unwrap()])
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    std::fs::copy(
        harness.home.join("credentials/fixture-model-key"),
        harness.home.join("credentials/model-key"),
    )
    .unwrap();
    let config = harness.home.join("config/microswarm.toml");
    let contents = std::fs::read_to_string(&config)
        .unwrap()
        .replace("fixture-model-key", "model-key");
    std::fs::write(config, contents).unwrap();
    let output = harness
        .chat(None, &["spawn researcher for auth review"])
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lead_id = harness
        .session_dirs()
        .unwrap()
        .iter()
        .find_map(|path| {
            let db = rusqlite::Connection::open(harness.home.join("supervisor.db")).ok()?;
            let id = path.file_name()?.to_string_lossy().into_owned();
            let kind: String = db
                .query_row("SELECT kind FROM sessions WHERE id=?1", [&id], |r| r.get(0))
                .ok()?;
            (kind == "lead").then_some(id)
        })
        .unwrap();
    let ended = rpc(
        &harness.home,
        json!({"op":"end_session","session_id":lead_id}),
    );
    assert_eq!(ended["closed_members"], 1);
    let db = rusqlite::Connection::open(harness.home.join("supervisor.db")).unwrap();
    let order: Vec<String> = {
        let mut q = db.prepare("SELECT agent_role FROM supervisor_events WHERE type='session.closed' AND root_session_id=?1 ORDER BY seq DESC LIMIT 2").unwrap();
        q.query_map([&lead_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(order[0], "lead", "lead should close after member");
    assert_ne!(order[1], "lead", "member close event must precede lead");
}
