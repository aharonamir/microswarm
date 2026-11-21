use serde_json::{Value, json};

pub fn derive(events: &[Value], agents: &[Value], steps: &[Value]) -> Value {
    let mut groups: Vec<Value> = Vec::new();
    let mut group_for_turn: Option<usize> = None;
    let mut lead_turn = 0usize;
    let mut anchor: Option<String> = None;
    let mut current_anchor: Option<String> = None;
    let mut members = serde_json::Map::new();
    for event in events {
        let typ = event["type"].as_str().unwrap_or_default();
        let sid = event["data"]["requesting_session_id"]
            .as_str()
            .or(event["session_id"].as_str())
            .unwrap_or_default();
        if typ == "message.inbound" {
            current_anchor = event["data"]["message_id"].as_str().map(str::to_owned);
        }
        if event["agent_role"] == "lead" && typ == "turn.start" {
            lead_turn += 1;
            anchor = current_anchor.clone();
            group_for_turn = None;
        }
        if typ == "delegation.spawned" || typ == "delegation.request" || typ == "delegation.denied"
        {
            let idx = *group_for_turn.get_or_insert_with(|| {
                groups.push(json!({"group_id":format!("group-{lead_turn}"),"anchor_message_id":anchor,"started_at":event["ts"],"ended_at":null,"members":[],"more":0}));
                groups.len()-1
            });
            let data = &event["data"];
            let Some(member_id) = data["member_id"].as_str() else {
                continue;
            };
            let role = data["role"].as_str().unwrap_or("member");
            members.entry(member_id.to_owned()).or_insert_with(|| json!({
                "agent":agents.iter().find(|a|a["id"]==member_id).cloned().unwrap_or(json!({"id":member_id,"kind":"member","role":role,"colour_index":1,"initial":"M"})),
                "delegation_id":data["delegation_id"].as_str().unwrap_or(member_id),"task":data["task"],"task_truncated":false,
                "state":"starting","step_count":0,"started_at":event["ts"],"elapsed_ms":0,"reach_summary":{"folders":[],"network":"none","credentials":[],"tools_ask":[]},"reduced_count":0
            }));
            groups[idx]["members"]
                .as_array_mut()
                .unwrap()
                .push(json!(member_id));
        }
        if typ == "turn.end"
            && event["agent_role"] == "lead"
            && let Some(idx) = group_for_turn
        {
            groups[idx]["ended_at"] = event["ts"].clone();
        }
        if let Some(member) = members.get_mut(sid) {
            let state = match typ {
                "turn.start" | "tool.call" => "working",
                "approval.requested" => "waiting_you",
                "approval.resolved" => "working",
                "delegation.reply" | "agent.report" => "reported",
                "session.closed" => "dismissed",
                "delegation.timeout" => "timed_out",
                "delegation.denied" => "denied",
                "delegation.dismissed" => "dismissed",
                "tool.denied" => "denied",
                "turn.failed" => "failed",
                _ => continue,
            };
            member["state"] = json!(state);
            if typ == "delegation.reply" {
                member["report_excerpt"] = json!(
                    event["data"]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .chars()
                        .take(280)
                        .collect::<String>()
                );
            }
            if typ == "approval.requested" {
                let sentence = super::sentences::sentence(typ, &event["data"]);
                member["pending_approval"] = json!({"id":event["data"]["approval_id"].as_str().or(event["data"]["id"].as_str()),"code":event["data"]["code"],"summary":{"key":sentence.key,"args":sentence.args}});
            }
        }
    }
    for member in members.values_mut() {
        let id = member["agent"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let agent_steps: Vec<&Value> = steps.iter().filter(|s| s["agent"]["id"] == id).collect();
        member["step_count"] = json!(agent_steps.len());
        if member["state"] == "working"
            && let Some(step) = agent_steps.last()
        {
            member["current_step"] = step["sentence"].clone();
        }
        let delegation_id = member["delegation_id"].as_str().unwrap_or_default();
        member["reduced_count"] = json!(
            events
                .iter()
                .filter(|e| e["type"] == "scope.narrowed"
                    && (e["session_id"] == id || e["data"]["delegation_id"] == delegation_id))
                .count()
        );
    }
    for group in &mut groups {
        let ids = group["members"].as_array().cloned().unwrap_or_default();
        let all_rows: Vec<Value> = ids
            .iter()
            .filter_map(|id| members.get(id.as_str()?).cloned())
            .collect();
        group["more"] = json!(all_rows.len().saturating_sub(4));
        group["members"] = json!(all_rows.into_iter().take(4).collect::<Vec<_>>());
    }
    json!({"groups":groups,"agents":agents})
}
