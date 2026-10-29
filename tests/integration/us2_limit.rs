#[path = "us2_support.rs"]
mod support;

#[test]
fn active_member_limit_returns_a_limit_denial() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, text.replace("max_members = 4", "max_members = 1")).unwrap();
    let output = harness.chat(None, &["spawn researcher twice"]).unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("denied"));
    assert_eq!(support::members(&harness).len(), 1);
}
