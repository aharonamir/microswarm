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
        let sid = event["session_id"].as_str().unwrap_or_default();
        if typ == "message.inbound" {
            current_anchor = event["data"]["message_id"].as_str().map(str::to_owned);
        }
        if event["agent_role"] == "lead" && typ == "turn.start" {
            lead_turn += 1;
            anchor = current_anchor.clone();
            group_for_turn = None;
        }
        if typ == "delegation.spawned" || typ == "delegation.request" {
            let idx = *group_for_turn.get_or_insert_with(|| {
                groups.push(json!({"group_id":format!("group-{lead_turn}"),"anchor_message_id":anchor,"started_at":event["ts"],"ended_at":null,"members":[],"more":0}));
                groups.len()-1
            });
            let data = &event["data"];
            let member_id = data["member_id"]
                .as_str()
                .or_else(|| data["session_id"].as_str())
                .unwrap_or(sid);
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
        if typ == "turn.end" && event["agent_role"] == "lead" {
            if let Some(idx) = group_for_turn {
                groups[idx]["ended_at"] = event["ts"].clone();
            }
        }
        if let Some(member) = members.get_mut(sid) {
            let state = match typ {
                "turn.start" => "working",
                "approval.requested" => "waiting_you",
                "approval.resolved" => "working",
                "delegation.reply" | "agent.report" => "reported",
                "session.closed" => "dismissed",
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
                member["pending_approval"] = json!({"id":event["data"]["id"],"code":event["data"]["code"],"summary":{"key":"step.shell.ask","args":{}}});
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
        if let Some(step) = agent_steps.last() {
            member["current_step"] = step["sentence"].clone();
        }
        member["reduced_count"] = json!(
            events
                .iter()
                .filter(|e| e["session_id"] == id && e["type"] == "scope.narrowed")
                .count()
        );
    }
    for group in &mut groups {
        let ids = group["members"].as_array().cloned().unwrap_or_default();
        let rows: Vec<Value> = ids
            .iter()
            .filter_map(|id| members.get(id.as_str()?).cloned())
            .collect();
        group["members"] = json!(rows);
    }
    json!({"groups":groups,"agents":agents})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(seq: i64, typ: &str, role: &str, sid: &str, data: Value) -> Value {
        json!({"seq":seq,"ts":format!("2026-11-19T12:00:{:02}Z",seq),"ts_mono":seq*1000,"type":typ,"agent_role":role,"session_id":sid,"data":data})
    }
    #[test]
    fn groups_spawned_members_by_lead_turn_and_anchors_to_prior_message() {
        let events = vec![
            ev(
                1,
                "message.inbound",
                "lead",
                "lead",
                json!({"message_id":"msg-1"}),
            ),
            ev(2, "turn.start", "lead", "lead", json!({})),
            ev(
                3,
                "delegation.spawned",
                "lead",
                "lead",
                json!({"member_id":"m1","role":"tester","task":"check tests"}),
            ),
            ev(4, "turn.end", "lead", "lead", json!({})),
            ev(
                5,
                "delegation.reply",
                "tester",
                "m1",
                json!({"text":"Status: done\nAll green"}),
            ),
        ];
        let team = derive(
            &events,
            &[
                json!({"id":"lead","kind":"lead"}),
                json!({"id":"m1","kind":"member","role":"tester"}),
            ],
            &[],
        );
        assert_eq!(team["groups"].as_array().unwrap().len(), 1);
        assert_eq!(team["groups"][0]["anchor_message_id"], "msg-1");
        assert_eq!(team["groups"][0]["members"][0]["state"], "reported");
        assert_eq!(
            team["groups"][0]["members"][0]["report_excerpt"],
            "Status: done\nAll green"
        );
    }
    #[test]
    fn maps_approval_wait_and_preserves_all_agents() {
        let events = vec![
            ev(
                1,
                "delegation.request",
                "lead",
                "lead",
                json!({"member_id":"m1","task":"x"}),
            ),
            ev(
                2,
                "approval.requested",
                "tester",
                "m1",
                json!({"id":"a1","code":"ABCD"}),
            ),
        ];
        let team = derive(&events, &[json!({"id":"lead"}), json!({"id":"m1"})], &[]);
        assert_eq!(team["groups"][0]["members"][0]["state"], "waiting_you");
        assert_eq!(
            team["groups"][0]["members"][0]["pending_approval"]["id"],
            "a1"
        );
        assert_eq!(team["agents"].as_array().unwrap().len(), 2);
    }
}
