use ms_agent::permit::{PermitError, authorize_approved_tool};
use ms_proto::{ApprovalBinding, Permission, Scope, ToolGrant, ToolNames};
use serde_json::json;
use std::collections::BTreeSet;

fn ask_scope() -> Scope {
    let mut scope: Scope = serde_json::from_value(json!({
        "version":1, "session_id":"session-1", "derived_from":"config",
        "source_ref":"test", "mounts":[], "credentials":[], "delegation":null,
        "tools":[
          {"server":"weather","tools":["forecast","other"],"policy":"ask","overrides":{}},
          {"server":"other","tools":["forecast"],"policy":"ask","overrides":{}}
        ],
        "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":1,"max_tokens":10},
        "narrowed":[],
        "hash":""
    }))
    .unwrap();
    scope.update_hash();
    scope
}

#[test]
fn approval_authorizes_only_the_exact_server_tool_and_argument_hash() {
    let scope = ask_scope();
    let arguments = json!({"city":"Jerusalem"}).as_object().cloned();
    let approval_args = json!({"city":"Jerusalem"});
    let approved = ApprovalBinding::for_call("weather", "forecast", Some(&approval_args));

    let call = authorize_approved_tool(&scope, "weather", "forecast", arguments.clone(), &approved);
    assert!(call.is_ok());
    assert_eq!(
        authorize_approved_tool(&scope, "other", "forecast", arguments.clone(), &approved)
            .unwrap_err(),
        PermitError::ApprovalMismatch
    );
    assert_eq!(
        authorize_approved_tool(&scope, "weather", "other", arguments.clone(), &approved)
            .unwrap_err(),
        PermitError::ApprovalMismatch
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
