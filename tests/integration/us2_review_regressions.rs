#[path = "us2_support.rs"]
mod support;

use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn member_role_instructions_are_mounted_read_only_and_sent_to_the_model() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    assert!(
        harness
            .chat(None, &["spawn researcher"])
            .unwrap()
            .status
            .success()
    );
    let member = support::members(&harness).remove(0);
    let container = harness.containers_for(&member.0).unwrap().remove(0);
    let runtime = match harness.runtime {
        support::harness::Runtime::Docker => "docker",
        support::harness::Runtime::Podman => "podman",
    };
    let mounts = Command::new(runtime)
        .args([
            "inspect",
            "--format",
            "{{range .Mounts}}{{println .Destination .RW}}{{end}}",
            &container,
        ])
        .output()
        .unwrap();
    assert!(mounts.status.success());
    let mounts = String::from_utf8_lossy(&mounts.stdout);
    assert!(
        mounts
            .lines()
            .any(|line| line == "/skills/us2-test-team/role.md false"),
        "role instructions mount missing or writable: {mounts}"
    );

    let session =
        ms_store::open_session_dir(harness.home.join("sessions").join(&member.0)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let body = loop {
        let result = session.outbound().query_row(
            "SELECT body FROM outbound WHERE kind='reply' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        );
        if let Ok(body) = result {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "member did not reply to initial task"
        );
        thread::sleep(Duration::from_millis(50));
    };
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["text"], "role instructions reached model");
}

#[test]
fn initial_member_task_reply_is_routed_to_the_lead() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    assert!(
        harness
            .chat(None, &["spawn researcher"])
            .unwrap()
            .status
            .success()
    );
    let member = support::members(&harness).remove(0);
    let lead = ms_store::open_session_dir(
        harness
            .home
            .join("sessions")
            .join(member.2.as_deref().unwrap()),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let reply = loop {
        let result = lead.inbound().query_row(
            "SELECT body FROM inbound WHERE kind='delegation_result' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        );
        if let Ok(reply) = result {
            break reply;
        }
        assert!(
            Instant::now() < deadline,
            "initial task reply was not routed to the lead"
        );
        thread::sleep(Duration::from_millis(50));
    };
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["member_id"], member.0);
    assert_eq!(reply["initial_reply"], "role instructions reached model");
}

#[test]
fn initial_member_task_approval_is_prompted_to_the_lead() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("policy = \"allow\"", "policy = \"ask\"");
    std::fs::write(config, text).unwrap();
    std::fs::write(
        harness.home.join("work/approval-target"),
        "approved content",
    )
    .unwrap();
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn researcher and read file").unwrap();
    let prompt = chat.wait_for("/approve ", Duration::from_secs(8)).unwrap();
    assert!(
        prompt.contains("[researcher "),
        "member role/id prefix missing: {prompt}"
    );
}

#[test]
fn adhoc_role_uses_default_policy_without_an_installed_skill() {
    let Some(harness) = support::harness::IntegrationHarness::start_swarm_or_skip() else {
        return;
    };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn adhoc").unwrap();
    let prompt = chat.wait_for("/approve ", Duration::from_secs(8)).unwrap();
    assert!(
        prompt.contains("adhoc"),
        "adhoc role missing from prompt: {prompt}"
    );
    let code = prompt
        .split("/approve ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    chat.send_line(&format!("/approve {code}")).unwrap();
    chat.wait_for("fixture reply", Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        support::members(&harness)
            .iter()
            .filter(|member| member.1 == "adhoc")
            .count(),
        1
    );
}

#[test]
fn timed_out_send_stops_the_member_container() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(None, &["spawn researcher and send hang"])
        .unwrap();
    assert!(
        output.status.success(),
        "chat failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let member = support::members(&harness).remove(0);
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let (status, reason): (String, Option<String>) = store
        .connection()
        .query_row(
            "SELECT status, close_reason FROM sessions WHERE id=?1",
            [&member.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (status.as_str(), reason.as_deref()),
        ("failed", Some("failed"))
    );
    let container = harness.containers_for(&member.0).unwrap().remove(0);
    assert!(
        !harness.container_running(&container).unwrap(),
        "timed-out member is still running"
    );
}
