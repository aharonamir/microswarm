#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use std::time::Duration;

#[test]
#[ignore = "red: T024 — approval flow acceptance test is not implemented yet"]
fn ask_policy_waits_for_approval_before_running_tool() {
    let Some(harness) = IntegrationHarness::start_approval_test_or_skip() else {
        return;
    };
    let mut first_chat = harness.start_chat(None).expect("start chat process");
    first_chat
        .send_line("run approval tool")
        .expect("send tool request");
    first_chat.close_input();

    let prompt = first_chat
        .wait_for("/approve ", Duration::from_secs(10))
        .expect("ask policy should show an approval prompt");
    let code = prompt
        .split("/approve ")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .expect("approval prompt includes a short code")
        .trim();
    assert!(!code.is_empty());

    let sessions = harness.session_dirs().expect("list session directories");
    assert_eq!(sessions.len(), 1);
    let marker = harness.home.join("work/approval-marker");
    assert!(
        !marker.exists(),
        "tool must remain suspended before approval"
    );

    let session_id = sessions[0]
        .file_name()
        .expect("session directory has an id")
        .to_string_lossy();
    let approval_command = format!("/approve {code}");
    let approval = harness
        .chat(Some(&session_id), &[&approval_command])
        .expect("submit approval command");
    assert!(
        approval.status.success(),
        "{}",
        String::from_utf8_lossy(&approval.stderr)
    );

    let reply = first_chat
        .wait_for("fixture reply", Duration::from_secs(10))
        .expect("agent should continue after approval");
    assert!(reply.contains("fixture reply"));
    assert!(marker.is_file(), "tool should run after approval");
    assert!(first_chat.wait().expect("chat process exits").success());
}
