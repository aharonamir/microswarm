use ms_proto::{MountMode, NarrowedReason, Scope};
use serde_json::{Value, json};

pub fn derive(view: &Value, lead: bool) -> Result<Value, String> {
    let granted = view
        .get("granted")
        .cloned()
        .ok_or("granted scope missing")?;
    let scope: Scope = serde_json::from_value(granted.clone()).map_err(|e| e.to_string())?;
    let requested = view
        .get("rows")
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|r| r["dimension"] == "mounts"))
        .map(|r| r["requested"].clone())
        .unwrap_or(Value::Null);
    let lead_scope = view.get("lead_scope").cloned().unwrap_or(Value::Null);
    let ceiling = view
        .get("rows")
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|r| r["dimension"] == "mounts"))
        .map(|r| r["ceiling"].clone())
        .unwrap_or(Value::Null);
    let can_reach: Vec<Value> = scope.mounts.iter().map(|m|json!({"icon":"folder","text":{"key":"reach.folder","args":{"path":m.container_path.display().to_string()}},"tag":if m.mode==MountMode::ReadOnly{"read only"}else{"read and write"}})).chain(scope.tools.iter().map(|tool|json!({"icon":"tool","text":{"key":"reach.tool","args":{"name":tool.server}},"tag":match tool.policy {ms_proto::Permission::Ask=>"asks you",_=>"allowed"}}))).chain(scope.credentials.iter().map(|name|json!({"icon":"credential","text":{"key":"reach.credential","args":{"name":name}},"tag":"allowed"}))).collect();
    let reduced: Vec<Value> = if lead {
        vec![]
    } else {
        scope.narrowed.iter().map(|item| {
        let reason = match item.reason {NarrowedReason::NotInLeadScope=>"not_in_lead_scope",NarrowedReason::Ceiling=>"ceiling",NarrowedReason::ModeDowngraded=>"mode_downgraded",NarrowedReason::PolicyTightened=>"policy_tightened",NarrowedReason::LimitCapped=>"limit_capped"};
        json!({"dimension":format!("{:?}",item.dimension).to_lowercase(),"item":item.item,"reason":reason,"text":{"key":format!("reach.reason.{reason}"),"args":{}}})
    }).collect()
    };
    let requested_mounts = requested.as_array().cloned().unwrap_or_default();
    let requested_chips=requested_mounts.iter().map(|mount| {
        let path=mount["container_path"].as_str().or(mount["path"].as_str()).unwrap_or("");
        let granted=scope.mounts.iter().find(|item|item.container_path.to_string_lossy()==path);
        json!({"text":path,"struck":granted.is_none(),"lowered":granted.is_some_and(|item|mount["mode"]=="rw"&&item.mode==MountMode::ReadOnly)})
    }).collect::<Vec<_>>();
    let lead_chips = lead_scope["mounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|mount| json!({"text":mount["container_path"],"struck":false,"lowered":false}))
        .collect::<Vec<_>>();
    let ceiling_chips = ceiling
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| json!({"text":item,"struck":false,"lowered":false}))
        .collect::<Vec<_>>();
    let folders = scope.mounts.len();
    let network = if scope.credentials.is_empty() {
        "none"
    } else {
        "proxy_only"
    };
    Ok(json!({
        "summary":{"key":"reach.summary","args":{"folders":folders,"network":network,"credentials":scope.credentials.len()}},
        "can_reach":can_reach,"reduced":reduced,
        "derivation":if lead {Value::Null} else {json!({"requested":requested_chips,"lead":lead_chips,"ceiling":ceiling_chips,"granted":scope.mounts.iter().map(|m|json!({"text":m.container_path.display().to_string(),"struck":false,"lowered":false})).collect::<Vec<_>>()})},
        "container":{"files_key":"reach.container.files","process_key":"reach.container.process","network_key":"reach.container.network","args":{"root_read_only":true,"no_new_privileges":true,"network":network,"mounts":folders}},
        "raw":{"granted":granted,"requested":view["rows"],"lead":lead_scope,"ceiling":ceiling}
    }))
}

pub fn summary(scope_value: &Value) -> Value {
    let scope: Result<Scope, _> = serde_json::from_value(scope_value.clone());
    let Ok(scope) = scope else {
        return json!({"folders":[],"network":"none","credentials":[],"tools_ask":[]});
    };
    json!({"folders":scope.mounts.iter().map(|mount|json!({"path":mount.container_path,"mode":if mount.mode==MountMode::ReadOnly{"ro"}else{"rw"}})).collect::<Vec<_>>(),"network":if scope.credentials.is_empty(){"none"}else{"proxy_only"},"credentials":scope.credentials,"tools_ask":scope.tools.iter().filter(|tool|tool.policy==ms_proto::Permission::Ask).map(|tool|tool.server.clone()).collect::<Vec<_>>()})
}
