#[path = "us2_support.rs"]
mod support;

#[test]
fn lead_idle_shutdown_cascades_to_members() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("\n[limits]\nlead_idle_s = 1\nmember_idle_s = 900\ngrace_ms = 1000\n");
    std::fs::write(&config, text).unwrap();
    let output = harness.chat(None, &["spawn researcher"]).unwrap();
    assert!(output.status.success());
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let lead_id = members[0].2.as_ref().unwrap();
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let (lead_status, member_status): (String, String) = store.connection().query_row(
            "SELECT l.status, m.status FROM sessions l JOIN sessions m ON m.parent_id=l.id WHERE l.id=?1 AND m.id=?2",
            [lead_id, &members[0].0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        if lead_status == "closed" {
            assert_eq!(member_status, "closed");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "lead idle shutdown did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let containers = harness.containers_for(&members[0].0).unwrap();
    assert_eq!(containers.len(), 1);
    assert!(!harness.container_running(&containers[0]).unwrap());
}
