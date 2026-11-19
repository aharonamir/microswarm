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
    let chips = |value: &Value| -> Vec<Value> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| json!({"text":item,"struck":false,"lowered":false}))
            .collect()
    };
    let folders = scope.mounts.len();
    let network = if scope.credentials.is_empty() {
        "none"
    } else {
        "proxy_only"
    };
    Ok(json!({
        "summary":{"key":"reach.summary","args":{"folders":folders,"network":network,"credentials":scope.credentials.len()}},
        "can_reach":can_reach,"reduced":reduced,
        "derivation":if lead {Value::Null} else {json!({"requested":chips(&requested),"lead":chips(&lead_scope),"ceiling":chips(&ceiling),"granted":scope.mounts.iter().map(|m|json!({"text":m.container_path.display().to_string(),"struck":false,"lowered":m.mode==MountMode::ReadOnly})).collect::<Vec<_>>()})},
        "container":{"files_key":"reach.container.files","process_key":"reach.container.process","network_key":"reach.container.network","args":{"root_read_only":true,"no_new_privileges":true,"network":network,"mounts":folders}},
        "raw":{"granted":granted,"requested":view["rows"],"lead":lead_scope,"ceiling":ceiling}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_every_narrowing_reason_and_builds_derivation_chips() {
        let reasons = [
            "not_in_lead_scope",
            "ceiling",
            "mode_downgraded",
            "policy_tightened",
            "limit_capped",
        ];
        let narrowed: Vec<Value> = reasons
            .iter()
            .map(|reason| json!({"dimension":"tool","item":"x","reason":reason}))
            .collect();
        let scope = json!({"version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"delegation","source_ref":"team","mounts":[],"credentials":[],"tools":[],"delegation":null,"limits":{"memory_mb":1,"pids":1,"tmp_mb":1,"max_turns":1,"max_tokens":1},"narrowed":narrowed,"hash":"x"});
        let view = json!({"granted":scope,"rows":[{"dimension":"mounts","requested":["/a"],"lead":["/a","/b"],"ceiling":[]} ]});
        let reach = derive(&view, false).unwrap();
        assert_eq!(reach["reduced"].as_array().unwrap().len(), 5);
        assert_eq!(
            reach["reduced"][0]["text"]["key"],
            "reach.reason.not_in_lead_scope"
        );
        assert_eq!(reach["derivation"]["requested"][0]["text"], "/a");
    }
    #[test]
    fn lead_has_no_derivation_or_reductions() {
        let scope = json!({"version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config","source_ref":"cfg","mounts":[],"credentials":[],"tools":[],"delegation":null,"limits":{"memory_mb":1,"pids":1,"tmp_mb":1,"max_turns":1,"max_tokens":1},"narrowed":[],"hash":"x"});
        let result = derive(&json!({"granted":scope}), true).unwrap();
        assert!(result["derivation"].is_null());
        assert_eq!(result["reduced"], json!([]));
    }
}
