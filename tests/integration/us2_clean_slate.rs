#[path = "us2_support.rs"]
mod support;

#[test]
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
        let connection = session.outbound();
        let mut statement = connection
            .prepare("SELECT role, content FROM history ORDER BY id")
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(1)?).unwrap(),
                ))
            })
            .unwrap();
        let history = rows.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            history.len(),
            2,
            "each member should contain only its assigned task turn"
        );
        assert_eq!(history[0].0, "user");
        assert_eq!(history[0].1, "review the auth module");
        assert_eq!(history[1].0, "assistant");
    }
}
