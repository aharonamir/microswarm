#[path = "us2_support.rs"]
mod support;

use std::time::Duration;

#[test]
#[ignore = "red: T047 — ask policy must show the concrete delegation request before spawn"]
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
}
