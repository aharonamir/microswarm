use ms_agent::delegate::{prepare_outbound, tool_definitions};
use ms_proto::OutboundKind;
use serde_json::json;

#[test]
fn delegation_tools_are_exposed_only_to_enabled_leads() {
    let lead = tool_definitions("lead", true, false);
    assert_eq!(lead.len(), 4);
    assert!(
        lead.iter()
            .any(|tool| tool["function"]["name"] == "microswarm-core/spawn_member")
    );
    assert!(tool_definitions("member", true, false).is_empty());
    assert!(tool_definitions("lead", false, false).is_empty());
}

#[test]
fn spawn_tool_call_becomes_a_validated_delegation_request() {
    let message = prepare_outbound(
        "spawn_member",
        json!({
            "role": "researcher",
            "task": "Review the auth module",
            "scope": { "mounts": [{ "path": "/work/src/auth", "mode": "ro" }] }
        }),
        Some(41),
    )
    .unwrap();
    assert_eq!(message.kind, OutboundKind::DelegationRequest);
    assert_eq!(message.in_reply_to, Some(41));
    assert_eq!(message.body["role"], "researcher");
    assert!(prepare_outbound("spawn_member", json!({"role":"researcher"}), None).is_err());
}

#[test]
fn all_delegation_operations_map_to_their_contract_outbound_kinds() {
    let send = prepare_outbound(
        "send_to_member",
        json!({"member_id":"01J00000000000000000000001","text":"run tests"}),
        None,
    )
    .unwrap();
    assert_eq!(send.kind, OutboundKind::SendToMember);
    let dismiss = prepare_outbound(
        "dismiss_member",
        json!({"member_id":"01J00000000000000000000001"}),
        None,
    )
    .unwrap();
    assert_eq!(dismiss.kind, OutboundKind::Dismiss);
    let list = prepare_outbound("list_members", json!({}), None).unwrap();
    assert_eq!(list.kind, OutboundKind::DelegationRequest);
    assert_eq!(list.body["op"], "list_members");
}

#[test]
fn delegation_request_is_written_to_the_session_outbound_database() {
    let directory = std::env::temp_dir().join(format!(
        "ms-delegation-agent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let connections = ms_store::open_session_dir(&directory).unwrap();
    let mut store = ms_store::SessionStore::open_dir(&directory).unwrap();
    let message = prepare_outbound(
        "spawn_member",
        json!({"role":"researcher","task":"Review auth","scope":{}}),
        Some(9),
    )
    .unwrap();
    ms_agent::delegate::write_outbound(&mut store, &message).unwrap();
    let row: (String, String, Option<i64>) = connections
        .outbound()
        .query_row(
            "SELECT kind, body, in_reply_to FROM outbound ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(row.0, "delegation_request");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&row.1).unwrap()["task"],
        "Review auth"
    );
    assert_eq!(row.2, Some(9));
    drop(connections);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn delegation_turn_waits_for_result_and_acknowledges_that_result() {
    let directory = std::env::temp_dir().join(format!(
        "ms-delegation-wait-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let supervisor = ms_store::open_session_dir(&directory).unwrap();
    let inbound = ms_store::open_session_dir(&directory).unwrap();
    let mut agent = ms_store::SessionStore::open_dir(&directory).unwrap();
    let response_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        inbound.inbound().execute(
            "INSERT INTO inbound(kind, body) VALUES ('delegation_result', '{\"status\":\"spawned\",\"member_id\":\"01J00000000000000000000001\"}')",
            [],
        ).unwrap();
    });
    let result = ms_agent::delegate::wait_for_result(
        &mut agent,
        0,
        vec![ms_proto::InboundKind::DelegationResult],
        std::time::Duration::from_secs(2),
    )
    .await
    .unwrap();
    response_task.await.unwrap();
    assert_eq!(result.body["status"], "spawned");
    assert_eq!(agent.cursor().unwrap(), result.id.unwrap());
    drop(agent);
    drop(supervisor);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn next_delegation_wait_starts_after_the_acknowledged_cursor() {
    let directory = std::env::temp_dir().join(format!(
        "ms-delegation-cursor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let supervisor = ms_store::open_session_dir(&directory).unwrap();
    let mut agent = ms_store::SessionStore::open_dir(&directory).unwrap();
    supervisor.inbound().execute(
        "INSERT INTO inbound(kind, body) VALUES ('delegation_result', '{\"status\":\"spawned\"}')",
        [],
    ).unwrap();
    agent.acknowledge_inbound(1).unwrap();
    supervisor
        .inbound()
        .execute(
            "INSERT INTO inbound(kind, body) VALUES ('member_reply', '{\"text\":\"done\"}')",
            [],
        )
        .unwrap();
    let reply = ms_agent::delegate::wait_for_result(
        &mut agent,
        0,
        vec![ms_proto::InboundKind::MemberReply],
        std::time::Duration::from_millis(100),
    )
    .await;
    assert_eq!(reply.unwrap().body["text"], "done");
    drop(agent);
    drop(supervisor);
    std::fs::remove_dir_all(directory).unwrap();
}
