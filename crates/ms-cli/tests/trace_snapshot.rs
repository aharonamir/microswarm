mod common;

#[test]
#[ignore = "red: T092 — trace command not implemented"]
fn trace_renders_two_member_session_tree_snapshot() {
    let home = common::fixture();
    let output = common::trace(&home, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).unwrap();
    let expected = concat!(
        " 01ARZ3NDEKTSV4RRFFQ69G5FAV lead closed  scope: 1 mounts · 1 tools · 1 creds\n",
        "├─  1 session.created\n",
        "├─  1 tool.call microswarm-tools fs.read\n",
        "└─ 01ARZ3NDEKTSV4RRFFQ69G5FAW researcher closed  scope: 1 mounts · 1 tools · 0 creds\n",
        "   │  2 delegation.spawned\n",
    );
    assert_eq!(actual, expected);
    common::cleanup(&home);
}

#[test]
#[ignore = "red: T094 — trace event filtering not implemented"]
fn events_filter_keeps_only_selected_event_types() {
    let home = common::fixture();
    let output = common::trace(&home, &["--events", "tool.call"]);
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).unwrap();
    assert!(actual.contains("tool.call microswarm-tools fs.read"));
    assert!(!actual.contains("session.created"));
    assert!(!actual.contains("delegation.spawned"));
    common::cleanup(&home);
}
