#[path = "us2_support.rs"]
mod support;

use std::time::Duration;

#[test]
fn member_approval_prompt_names_the_member_and_resumes_after_approval() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("policy = \"allow\"", "policy = \"ask\"");
    std::fs::write(&config, text).unwrap();
    std::fs::write(
        harness.home.join("work/approval-target"),
        "approved content",
    )
    .unwrap();
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn researcher").unwrap();
    chat.wait_for("fixture reply", Duration::from_secs(10))
        .unwrap();
    chat.send_line("send read file approval task").unwrap();
    let prompt = chat.wait_for("/approve ", Duration::from_secs(15)).unwrap();
    assert!(
        prompt.contains("[researcher "),
        "member role/id prefix missing: {prompt}"
    );
    let code = prompt
        .split("/approve ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    chat.send_line(&format!("/approve {code}")).unwrap();
    chat.wait_for("fixture reply", Duration::from_secs(15))
        .unwrap();
}
