#[path = "us3_support.rs"]
mod support;

use std::{fs, time::Duration};

fn assert_notice(language: &str, expected: &str) {
    let Some(harness) = support::start() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str(&format!("\n[home]\nlang = {language:?}\n"));
    fs::write(config, text).unwrap();
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("transient always").unwrap();
    let lead = support::wait_for_lead_id(&harness.home, Duration::from_secs(5));
    let output = chat.wait_for(expected, Duration::from_secs(25)).unwrap();
    assert!(output.contains(&lead));
    support::wait_for_status(&harness.home, &lead, "failed", Duration::from_secs(2));
    assert_eq!(support::session_state(&harness.home, &lead).1, 3);
}

#[test]
fn lead_retry_exhaustion_is_announced_in_english_and_chinese() {
    assert_notice("en", "The session exhausted its restart attempts.");
    assert_notice("zh", "会话已用尽重启次数。");
}
