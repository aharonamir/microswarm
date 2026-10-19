use ms_agent::permit::{ApprovalBinding, PermitError, authorize_approved_tool};
use ms_proto::{Permission, Scope, ToolGrant, ToolNames};
use serde_json::json;
use std::collections::BTreeSet;

fn ask_scope() -> Scope {
    let mut scope: Scope = serde_json::from_value(json!({
        "session_id":"session-1", "parent_session_id":null,
        "source_ref":"test", "mounts":[], "credentials":[], "delegation":null,
        "tools":[{"server":"weather","tools":["forecast"],"policy":"ask","overrides":{}}],
        "limits":{"max_turns":1,"max_tokens":10,"max_runtime_s":1,"max_members":0},
        "hash":""
    })).unwrap();
    scope.update_hash();
    scope
}

#[test]
#[ignore = "red: T032 — approval results are not yet bound to an exact MCP call"]
fn approval_authorizes_only_the_exact_server_tool_and_argument_hash() {
    let scope = ask_scope();
    let arguments = json!({"city":"Jerusalem"}).as_object().cloned();
    let approved = ApprovalBinding::for_call("weather", "forecast", arguments.as_ref())
        .expect("serializable arguments produce a binding");

    let call = authorize_approved_tool(
        &scope,
        "weather",
        "forecast",
        arguments.clone(),
        &approved,
    );
    assert!(call.is_ok());
    assert_eq!(
        authorize_approved_tool(&scope, "other", "forecast", arguments.clone(), &approved)
            .unwrap_err(),
        PermitError::NotGranted
    );
    assert_eq!(
        authorize_approved_tool(&scope, "weather", "other", arguments.clone(), &approved)
            .unwrap_err(),
        PermitError::NotGranted
    );
    assert_eq!(
        authorize_approved_tool(
            &scope,
            "weather",
            "forecast",
            json!({"city":"Haifa"}).as_object().cloned(),
            &approved,
        )
        .unwrap_err(),
        PermitError::ApprovalMismatch
    );
}

#[test]
fn binding_fixture_uses_an_ask_policy() {
    // Keep the policy helper referenced as part of this compileable red test.
    let grant = ToolGrant {
        server: "weather".into(),
        tools: ToolNames::Selected(BTreeSet::from(["forecast".into()])),
        policy: Permission::Ask,
        overrides: Default::default(),
    };
    assert_eq!(grant.policy, Permission::Ask);
}
