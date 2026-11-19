use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn derive(events: &[Value], steps: &[Value]) -> Vec<Value> {
    let mut phases = Vec::<Value>::new();
    let mut working = BTreeMap::<String, String>::new();
    let mut waiting = BTreeSet::<String>::new();
    let mut index = 0usize;
    while index < events.len() {
        let event = &events[index];
        let typ = event["type"].as_str().unwrap_or_default();
        match typ {
            "message.inbound" => {
                let turn_end = events[index + 1..]
                    .iter()
                    .position(|item| item["type"] == "turn.end")
                    .map(|offset| index + 1 + offset)
                    .unwrap_or(events.len());
                let plans = events[index + 1..turn_end]
                    .iter()
                    .any(|item| item["type"] == "delegation.request");
                add_phase(
                    &mut phases,
                    if plans {
                        "phase.lead_plans"
                    } else {
                        "phase.lead_works"
                    },
                    json!({}),
                    event["ts"].clone(),
                    steps,
                );
            }
            "delegation.spawned" => {
                let mut added = Vec::<(String, String)>::new();
                let mut end = index;
                while end < events.len()
                    && matches!(
                        events[end]["type"].as_str(),
                        Some("delegation.spawned" | "scope.narrowed")
                    )
                {
                    if events[end]["type"] == "delegation.spawned" {
                        let data = &events[end]["data"];
                        let role = data["role"]
                            .as_str()
                            .or(events[end]["agent_role"].as_str())
                            .unwrap_or("member")
                            .to_owned();
                        let id = data["member_id"].as_str().unwrap_or(&role).to_owned();
                        if !working.contains_key(&id) {
                            added.push((id, role));
                        }
                    }
                    end += 1;
                }
                let prior = working.len();
                for (id, role) in &added {
                    working.insert(id.clone(), role.clone());
                }
                if !added.is_empty() {
                    let count = working.len();
                    let (key, args) = if count > 1 {
                        ("phase.parallel", json!({"count":count}))
                    } else {
                        ("phase.one_member", json!({"role":added[0].1}))
                    };
                    if prior > 0 && count == 1 {
                        add_phase(
                            &mut phases,
                            "phase.one_member",
                            json!({"role":added[0].1}),
                            event["ts"].clone(),
                            steps,
                        );
                    } else {
                        add_phase(&mut phases, key, args, event["ts"].clone(), steps);
                    }
                }
                index = end.saturating_sub(1);
            }
            "delegation.reply"
            | "delegation.failed"
            | "delegation.timeout"
            | "delegation.dismissed" => {
                let id = event["session_id"].as_str().unwrap_or_default();
                let was_working = !working.is_empty();
                if working.remove(id).is_none() {
                    let role = event["agent_role"].as_str().unwrap_or_default();
                    let matching = working
                        .iter()
                        .find(|(_, value)| value.as_str() == role)
                        .map(|(key, _)| key.clone());
                    if let Some(key) = matching {
                        working.remove(&key);
                    }
                }
                if was_working && working.is_empty() {
                    add_phase(
                        &mut phases,
                        "phase.lead_checks",
                        json!({}),
                        event["ts"].clone(),
                        steps,
                    );
                }
            }
            "approval.requested" => {
                let id = event["data"]["approval_id"]
                    .as_str()
                    .or(event["data"]["id"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                let newly_waiting = waiting.insert(id);
                if newly_waiting && working.is_empty() {
                    add_phase(
                        &mut phases,
                        "phase.waiting_you",
                        json!({}),
                        event["ts"].clone(),
                        steps,
                    );
                }
            }
            "approval.resolved" | "approval.expired" => {
                let id = event["data"]["approval_id"]
                    .as_str()
                    .or(event["data"]["id"].as_str())
                    .unwrap_or_default();
                waiting.remove(id);
            }
            _ => {}
        }
        index += 1;
    }
    if phases.is_empty() {
        add_phase(
            &mut phases,
            "phase.lead_works",
            json!({}),
            Value::Null,
            steps,
        );
    }
    for index in 0..phases.len() {
        phases[index]["phase_id"] = json!(format!("phase-{}", index + 1));
        if index + 1 < phases.len() {
            phases[index]["ended_at"] = phases[index + 1]["started_at"].clone();
        }
    }
    phases
}

fn add_phase(phases: &mut Vec<Value>, key: &str, args: Value, at: Value, steps: &[Value]) {
    let phase_id = format!("phase-{}", phases.len() + 1);
    let step_ids = steps
        .iter()
        .filter(|step| step["at"] == at)
        .map(|step| step["step_id"].clone())
        .collect::<Vec<_>>();
    phases.push(json!({"phase_id":phase_id,"title_key":key,"title_args":args,"started_at":at,"step_ids":step_ids}));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: i64, typ: &str, role: &str, data: Value) -> Value {
        json!({"seq":seq,"ts":format!("2026-11-19T12:00:{:02}Z",seq),"ts_mono":seq*1000,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","agent_role":role,"type":typ,"data":data})
    }

    #[test]
    fn operator_messages_start_the_matching_lead_phase() {
        let events = vec![
            event(1, "message.inbound", "lead", json!({})),
            event(2, "delegation.request", "lead", json!({"role":"tester"})),
            event(3, "turn.end", "lead", json!({})),
            event(4, "message.inbound", "lead", json!({})),
        ];
        let phases = derive(&events, &[]);
        assert_eq!(phases[0]["title_key"], "phase.lead_plans");
        assert_eq!(phases[1]["title_key"], "phase.lead_works");
    }

    #[test]
    fn member_work_changes_start_parallel_and_lead_check_phases() {
        let events = vec![
            event(
                1,
                "delegation.spawned",
                "lead",
                json!({"role":"researcher"}),
            ),
            event(2, "delegation.spawned", "lead", json!({"role":"tester"})),
            event(
                3,
                "delegation.reply",
                "researcher",
                json!({"status":"done"}),
            ),
            event(4, "delegation.reply", "tester", json!({"status":"done"})),
        ];
        let phases = derive(&events, &[]);
        assert_eq!(phases[0]["title_key"], "phase.parallel");
        assert_eq!(phases[0]["title_args"]["count"], 2);
        assert_eq!(phases[1]["title_key"], "phase.lead_checks");
    }

    #[test]
    fn pending_approval_starts_a_waiting_phase_when_the_team_is_idle() {
        let events = vec![
            event(1, "approval.requested", "tester", json!({"id":"A1"})),
            event(
                2,
                "approval.resolved",
                "tester",
                json!({"id":"A1","decision":"approved"}),
            ),
        ];
        let phases = derive(&events, &[]);
        assert_eq!(phases[0]["title_key"], "phase.waiting_you");
    }

    #[test]
    fn missing_events_still_produce_a_degraded_phase() {
        let phases = derive(&[], &[]);
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0]["title_key"], "phase.lead_works");
    }
}
