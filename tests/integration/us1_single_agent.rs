#[path = "harness.rs"]
#[allow(dead_code)]
mod harness;

use harness::IntegrationHarness;
use std::fs;

#[test]
fn cli_message_gets_reply_from_one_labeled_container() {
    let harness = IntegrationHarness::start().expect("integration harness should start");
    let output = harness
        .chat(None, &["hello"])
        .expect("CLI chat should complete");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("fixture reply"));

    let sessions = harness.session_dirs().expect("read session directories");
    assert_eq!(
        sessions.len(),
        1,
        "one CLI thread should create one session"
    );
    let session_dir = &sessions[0];
    let session_id = session_dir.file_name().unwrap().to_string_lossy();
    for filename in [
        "control/inbound.db",
        "control/scope.json",
        "out/outbound.db",
        "out/events.db",
    ] {
        assert!(
            fs::metadata(session_dir.join(filename)).is_ok(),
            "missing {filename} in {}",
            session_dir.display()
        );
    }
    let containers = harness
        .containers_for(&session_id)
        .expect("query containers by session label");
    assert_eq!(containers.len(), 1, "expected exactly one agent container");
}

#[test]
fn second_cli_message_reuses_the_session_container() {
    let harness = IntegrationHarness::start().expect("integration harness should start");
    let first = harness
        .chat(None, &["first message"])
        .expect("first chat should complete");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let sessions = harness.session_dirs().expect("read session directories");
    assert_eq!(sessions.len(), 1);
    let session_id = sessions[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let before = harness
        .containers_for(&session_id)
        .expect("query initial session container");
    assert_eq!(before.len(), 1);

    let second = harness
        .chat(Some(&session_id), &["second message"])
        .expect("second chat should complete");
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stdout).contains("fixture reply"));
    let after = harness
        .containers_for(&session_id)
        .expect("query session container after second message");
    assert_eq!(
        after, before,
        "second message must not create another container"
    );
}
