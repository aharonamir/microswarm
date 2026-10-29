#[path = "us2_support.rs"]
mod support;

use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[test]
fn quickstart_swarm_flow_works_with_reference_team_skill() {
    let Some(harness) = support::harness::IntegrationHarness::start_swarm_or_skip() else {
        return;
    };
    std::fs::copy(
        harness.home.join("credentials/fixture-model-key"),
        harness.home.join("credentials/model-key"),
    )
    .unwrap();
    let config = harness.home.join("config/microswarm.toml");
    let config_text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("fixture-model-key", "model-key");
    std::fs::write(config, config_text).unwrap();
    let skill =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/skills/code-review-team");
    let install = harness
        .run_cli(&["skills", "install", skill.to_str().unwrap()])
        .unwrap();
    assert!(
        install.status.success(),
        "skill install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn tester to run tests").unwrap();
    let prompt = chat.wait_for("/approve ", Duration::from_secs(10)).unwrap();
    for detail in ["tester", "run tests", "/work", "shell.run"] {
        assert!(
            prompt.contains(detail),
            "approval prompt omits {detail}: {prompt}"
        );
    }
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

    chat.send_line("send run tests").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !support::has_supervisor_event(&harness, "delegation.reply") {
        assert!(
            Instant::now() < deadline,
            "member reply was not routed to lead"
        );
        thread::sleep(Duration::from_millis(50));
    }
    chat.send_line("dismiss member").unwrap();

    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM sessions WHERE id=?1",
                [&members[0].0],
                |row| row.get(0),
            )
            .unwrap();
        if status == "closed" {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "member dismissal did not complete"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status, "closed");
    assert!(support::has_supervisor_event(
        &harness,
        "delegation.dismissed"
    ));
}
