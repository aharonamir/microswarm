use ms_console::{events::ConsoleEvent, server::encode_sse_event};
use serde_json::json;

#[test]
fn hostile_agent_strings_remain_inert_json_in_api_and_sse_payloads() {
    let payloads = [
        "<script>alert(1)</script>",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
    ];
    let event = ConsoleEvent {
        id: 7,
        name: "agent".into(),
        data: json!({"type":"agent","event":{"data":{"text":payloads}}}),
    };
    let api_body = serde_json::to_string(&event.data).unwrap();
    let frame = encode_sse_event(&event);
    for payload in payloads {
        assert!(api_body.contains(payload));
        assert!(frame.contains(payload));
    }
    assert!(frame.starts_with("id: 7\nevent: agent\ndata: {"));
}
