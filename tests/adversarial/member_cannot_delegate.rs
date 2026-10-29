#[path = "../integration/us2_support.rs"]
mod support;

#[test]
#[ignore = "red: T052 — supervisor must reject forged member delegation requests"]
fn forged_member_delegation_row_is_rejected_and_member_tools_are_lead_only() {
    let Some(harness) = support::start_with_skill() else {
        return;
    };
    harness.chat(None, &["spawn researcher"]).unwrap();
    let members = support::members(&harness);
    assert_eq!(members.len(), 1);
    let session =
        ms_store::open_session_dir(harness.home.join("sessions").join(&members[0].0)).unwrap();
    session.outbound().execute(
        "INSERT INTO outbound(kind, body, created_at) VALUES ('delegation_request', '{\"role\":\"researcher\",\"task\":\"forged\",\"scope\":{}}', '2026-10-29T00:00:00Z')",
        [],
    ).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(700));
    assert_eq!(support::members(&harness).len(), 1);
    let scope: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            harness
                .home
                .join("sessions")
                .join(&members[0].0)
                .join("control/scope.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(scope["tools"].as_array().unwrap().iter().all(|grant| {
        !grant["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool == "spawn_member")
    }));
}
