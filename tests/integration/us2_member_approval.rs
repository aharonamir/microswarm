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
    let lead_id = support::members(&harness)[0].2.clone().unwrap();
    let lead = ms_store::open_session_dir(harness.home.join("sessions").join(lead_id)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let replies: i64 = lead
            .inbound()
            .query_row(
                "SELECT COUNT(*) FROM inbound WHERE kind='member_reply'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if replies == 1 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "approved member reply was not routed to the lead"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_second_send_does_not_enter_a_member_turn_waiting_for_approval() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    let config = harness.home.join("config/microswarm.toml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("policy = \"allow\"", "policy = \"ask\"");
    std::fs::write(config, text).unwrap();
    let mut chat = harness.start_chat(None).unwrap();
    chat.send_line("spawn researcher").unwrap();
    chat.wait_for("fixture reply", Duration::from_secs(10))
        .unwrap();
    chat.send_line("send read file approval task").unwrap();
    let prompt = chat.wait_for("/approve ", Duration::from_secs(15)).unwrap();
    let code = prompt
        .split("/approve ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    chat.send_line("send another task").unwrap();
    chat.wait_for("fixture reply", Duration::from_secs(15))
        .unwrap();
    let member_id = &support::members(&harness)[0].0;
    let member = ms_store::open_session_dir(harness.home.join("sessions").join(member_id)).unwrap();
    let tasks: i64 = member
        .inbound()
        .query_row(
            "SELECT COUNT(*) FROM inbound WHERE kind='task'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        tasks, 2,
        "the second send entered the unfinished member turn"
    );
    chat.send_line(&format!("/approve {code}")).unwrap();
    chat.wait_for("fixture reply", Duration::from_secs(15))
        .unwrap();
}
