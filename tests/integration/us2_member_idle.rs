#[path = "us2_support.rs"]
mod support;

#[test]
fn member_idle_timeout_closes_the_member_only() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("\n[limits]\nlead_idle_s = 900\nmember_idle_s = 1\ngrace_ms = 1000\n");
    std::fs::write(&config, text).unwrap();
    assert!(
        harness
            .chat(None, &["spawn researcher"])
            .unwrap()
            .status
            .success()
    );
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let (status, reason): (String, Option<String>) = store
            .connection()
            .query_row(
                "SELECT status, close_reason FROM sessions WHERE id=?1",
                [&members[0].0],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        if status == "closed" {
            assert_eq!(reason.as_deref(), Some("idle"));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "member idle timeout did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let lead_id = members[0].2.as_ref().unwrap();
    let lead_status: String = store
        .connection()
        .query_row(
            "SELECT status FROM sessions WHERE id=?1",
            [lead_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lead_status, "ready");
}
