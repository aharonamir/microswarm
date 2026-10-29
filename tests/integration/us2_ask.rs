#[path = "us2_support.rs"]
mod support;

use std::time::Duration;

#[test]
fn ask_role_shows_role_task_scope_and_waits_for_approval() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn tester to run tests").unwrap();
    let prompt = chat.wait_for("/approve ", Duration::from_secs(10)).unwrap();
    for detail in ["tester", "run tests", "mounts", "tools", "credentials"] {
        assert!(prompt.contains(detail), "approval prompt omits {detail}");
    }
    assert!(support::members(&harness).is_empty());
    let code = prompt
        .split("/approve ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    chat.send_line(&format!("/approve {code}")).unwrap();
    let completed = chat
        .wait_for("fixture reply", Duration::from_secs(10))
        .unwrap();
    assert!(completed.contains("fixture reply"));
    assert_eq!(support::members(&harness).len(), 1);
}
