#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn cli_message_gets_reply_from_one_labeled_container() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
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
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
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

#[test]
fn idle_lead_is_closed_with_idle_reason() {
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let config_path = harness.home.join("config/microswarm.toml");
    let mut config = fs::read_to_string(&config_path).unwrap();
    config.push_str("\n[limits]\nlead_idle_s = 1\n");
    fs::write(config_path, config).unwrap();

    let output = harness
        .chat(None, &["create a session before idle timeout"])
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let session_id = harness.session_dirs().unwrap()[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let db = harness.home.join("supervisor.db");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let store = ms_store::SupervisorStore::open_readonly(&db).unwrap();
        let status: (String, Option<String>) = store
            .connection()
            .query_row(
                "SELECT status, close_reason FROM sessions WHERE id=?1",
                [&session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        if status == ("closed".to_owned(), Some("idle".to_owned())) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "idle session did not close: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
