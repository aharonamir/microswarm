#[path = "us2_support.rs"]
mod support;

#[test]
#[ignore = "red: T051 — respawning a role must create a fresh session with empty history"]
fn respawn_same_role_uses_new_id_empty_history_and_fresh_scope() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let output = harness
        .chat(
            None,
            &["spawn and dismiss researcher; spawn researcher again"],
        )
        .unwrap();
    assert!(output.status.success());
    let members = support::members(&harness);
    assert_eq!(members.len(), 2);
    assert_ne!(members[0].0, members[1].0);
    for (id, _, _) in members {
        let session = ms_store::open_session_dir(harness.home.join("sessions").join(&id)).unwrap();
        let count: i64 = session
            .outbound()
            .query_row("SELECT COUNT(*) FROM outbound", [], |row| row.get(0))
            .unwrap();
        assert!(count > 0);
        let history_count: i64 = session
            .outbound()
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(history_count, 0);
    }
}
