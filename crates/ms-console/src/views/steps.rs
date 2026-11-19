use crate::views::{agent_by_id, sentences::sentence};
use serde_json::{Value, json};

pub fn derive(
    events: &[Value],
    agents: &[Value],
    before_seq: Option<i64>,
    after_seq: Option<i64>,
    limit: usize,
    filter: &str,
) -> Value {
    let mut pending = Vec::<Value>::new();
    let mut spawn_requests = std::collections::BTreeMap::<String, Value>::new();
    let mut spawn_indexes = std::collections::BTreeMap::<String, usize>::new();
    let mut steps = Vec::<(Value, i64, i64)>::new();
    for (index, event) in events.iter().enumerate() {
        let kind = event["type"].as_str().unwrap_or_default();
        let data = &event["data"];
        let sid = event["session_id"].as_str().unwrap_or_default();
        let call_id = data["call_id"].as_str().unwrap_or_default();
        match kind {
            "tool.call" => pending.push(event.clone()),
            "permit.decision" => {
                if let Some(call) = pending
                    .iter_mut()
                    .rev()
                    .find(|call| call["session_id"] == sid && call["data"]["call_id"] == call_id)
                {
                    call["_decision"] = data["decision"].clone();
                    call["_rule"] = data["rule"].clone();
                    call["_decision_seq"] = event["seq"].clone();
                }
            }
            "approval.requested" => {
                let target = data["requesting_session_id"].as_str().unwrap_or(sid);
                if let Some(call) = pending.iter_mut().rev().find(|call| {
                    call["session_id"] == target
                        && call["_approval"].is_null()
                        && (data["call_id"].is_null() || call["data"]["call_id"] == data["call_id"])
                }) {
                    call["_approval"] = json!({"id":data["approval_id"].as_str().or(data["id"].as_str()).unwrap_or_default(),"code":data["code"],"state":"pending"});
                    call["_approval_seq"] = event["seq"].clone();
                }
            }
            "tool.result" => {
                if let Some(pos) = pending.iter().rposition(|call| {
                    call["session_id"] == sid && call["data"]["call_id"] == call_id
                }) {
                    let call = pending.remove(pos);
                    let cursor = events
                        .iter()
                        .position(|item| item["session_id"] == sid && item["seq"] == call["seq"])
                        .unwrap_or(index);
                    steps.push((
                        tool_step(&call, event, agents, false),
                        (cursor + 1) as i64,
                        event["ts_mono"].as_i64().unwrap_or(0),
                    ));
                }
            }
            "agent.exit" => {
                let had_pending = pending.iter().any(|call| call["session_id"] == sid);
                let mut pos = 0;
                while pos < pending.len() {
                    if pending[pos]["session_id"] == sid {
                        let call = pending.remove(pos);
                        steps.push((
                            tool_step(&call, event, agents, true),
                            (index + 1) as i64,
                            event["ts_mono"].as_i64().unwrap_or(0),
                        ));
                    } else {
                        pos += 1;
                    }
                }
                if !had_pending {
                    let s = sentence(kind, data);
                    steps.push((
                        simple_step(event, index, agents, "exit", s.key, s.args, Some("failed")),
                        (index + 1) as i64,
                        event["ts_mono"].as_i64().unwrap_or(0),
                    ));
                }
            }
            "delegation.request" => {
                spawn_requests.insert(
                    data["delegation_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    event.clone(),
                );
            }
            "delegation.spawned" | "delegation.denied" => {
                let id = data["delegation_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                if let Some(request) = spawn_requests.remove(&id) {
                    let denied = kind == "delegation.denied";
                    let s = sentence(
                        if denied { kind } else { "delegation.spawned" },
                        &request["data"],
                    );
                    let mut step = json!({"step_id":format!("{}:{}",request["session_id"].as_str().unwrap_or_default(),request["seq"]),"agent":agent_by_id(agents,request["session_id"].as_str().unwrap_or_default()),"at":request["ts"],"ended_at":event["ts"],"kind":"spawn","sentence":{"key":s.key,"args":s.args},"phase_id":"phase-1","event_seqs":[request["seq"],event["seq"]]});
                    if denied {
                        step["decision"] = json!({"value":"denied_by_you"});
                    }
                    let cursor = events
                        .iter()
                        .position(|item| {
                            item["session_id"] == request["session_id"]
                                && item["seq"] == request["seq"]
                        })
                        .unwrap_or(index);
                    spawn_indexes.insert(id, steps.len());
                    steps.push((
                        step,
                        (cursor + 1) as i64,
                        event["ts_mono"].as_i64().unwrap_or(0),
                    ));
                }
            }
            "scope.narrowed" => {
                let id = data["delegation_id"].as_str().unwrap_or_default();
                if let Some(pos) = spawn_indexes.get(id).copied() {
                    if data["narrowed"]
                        .as_array()
                        .is_some_and(|items| !items.is_empty())
                    {
                        steps[pos].0["decision"] = json!({"value":"reduced"});
                        if let Some(seqs) = steps[pos].0["event_seqs"].as_array_mut() {
                            seqs.push(event["seq"].clone());
                        }
                    }
                }
            }
            "model.call" | "turn.start" | "agent.ready" => {}
            _ => {
                let s = sentence(kind, data);
                let no_step = matches!(
                    kind,
                    "message.delivered"
                        | "proxy.request"
                        | "supervisor.started"
                        | "turn.start"
                        | "model.call"
                        | "agent.ready"
                );
                if !no_step {
                    let step_kind = match kind {
                        "message.inbound" => "message_in",
                        "delegation.reply" => "report",
                        "delegation.dismiss" | "delegation.dismissed" => "dismiss",
                        "approval.requested" | "approval.resolved" | "approval.expired" => {
                            "approval"
                        }
                        "delegation.request" | "delegation.spawned" | "delegation.denied" => {
                            "spawn"
                        }
                        "scope.narrowed" => "reduced",
                        _ => "model",
                    };
                    let decision = match kind {
                        "approval.resolved" => match data["decision"].as_str() {
                            Some("approved") => Some("approved"),
                            Some("denied") => Some("denied_by_you"),
                            _ => None,
                        },
                        "approval.expired" => Some("expired"),
                        "delegation.timeout" => Some("timed_out"),
                        _ => None,
                    };
                    steps.push((
                        simple_step(event, index, agents, step_kind, s.key, s.args, decision),
                        (index + 1) as i64,
                        event["ts_mono"].as_i64().unwrap_or(0),
                    ));
                }
            }
        }
    }
    for call in pending {
        let cursor = events
            .iter()
            .position(|event| {
                event["session_id"] == call["session_id"] && event["seq"] == call["seq"]
            })
            .unwrap_or(0) as i64
            + 1;
        steps.push((
            tool_step(&call, &Value::Null, agents, false),
            cursor,
            call["ts_mono"].as_i64().unwrap_or(0),
        ));
    }
    steps.sort_by_key(|step| step.1);
    fold_reads(&mut steps);
    let last_seq = events.len() as i64;
    let filtered = steps
        .into_iter()
        .filter(|(step, _, _)| filter_step(step, filter))
        .collect::<Vec<_>>();
    let limit = limit.clamp(1, 200);
    let (page, more) = if let Some(after) = after_seq {
        (
            filtered
                .into_iter()
                .filter(|step| step.1 > after)
                .take(limit)
                .collect::<Vec<_>>(),
            false,
        )
    } else {
        let candidates = filtered
            .into_iter()
            .filter(|step| step.1 < before_seq.unwrap_or(i64::MAX))
            .collect::<Vec<_>>();
        let start = candidates.len().saturating_sub(limit);
        (
            candidates.into_iter().skip(start).collect::<Vec<_>>(),
            start > 0,
        )
    };
    let next_before_seq = if more {
        page.first().map(|step| step.1)
    } else {
        None
    };
    json!({"steps":page.into_iter().map(|step|step.0).collect::<Vec<_>>(),"phases":[],"next_before_seq":next_before_seq,"last_seq":last_seq})
}

fn tool_step(call: &Value, result: &Value, agents: &[Value], exited: bool) -> Value {
    let mut data = call["data"].clone();
    data["decision"] = call["_decision"].clone();
    let s = sentence("tool.call", &data);
    let sid = call["session_id"].as_str().unwrap_or_default();
    let mut seqs = vec![call["seq"].clone()];
    if !call["_decision_seq"].is_null() {
        seqs.push(call["_decision_seq"].clone());
    }
    if !call["_approval_seq"].is_null() {
        seqs.push(call["_approval_seq"].clone());
    }
    if !result.is_null() {
        seqs.push(result["seq"].clone());
    }
    let decision = if exited || result["ok"] == false {
        Some("failed")
    } else {
        match call["_decision"].as_str() {
            Some("allow") => Some("allowed"),
            Some("ask") => Some("asked_you"),
            Some("deny") => Some("denied_by_policy"),
            _ => None,
        }
    };
    let mut value = json!({"step_id":format!("{sid}:{}",call["seq"]),"agent":agent_by_id(agents,sid),"at":call["ts"],"kind":"tool","sentence":{"key":s.key,"args":s.args},"phase_id":"phase-1","event_seqs":seqs});
    if !result.is_null() {
        value["ended_at"] = result["ts"].clone();
        value["duration_ms"] = result["data"]["duration_ms"].clone();
    }
    if let Some(decision) = decision {
        value["decision"] = json!({"value":decision,"rule":call["_rule"]});
    }
    if !call["_approval"].is_null() {
        value["approval"] = call["_approval"].clone();
    }
    value
}

fn simple_step(
    event: &Value,
    index: usize,
    agents: &[Value],
    kind: &str,
    key: String,
    args: Value,
    decision: Option<&str>,
) -> Value {
    let sid = event["session_id"].as_str().unwrap_or_default();
    let agent = if kind == "message_in" {
        json!({"kind":"you"})
    } else {
        agent_by_id(agents, sid)
    };
    let mut step = json!({"step_id":format!("{sid}:{}",event["seq"]),"agent":agent,"at":event["ts"],"kind":kind,"sentence":{"key":key,"args":args},"phase_id":"phase-1","event_seqs":[event["seq"]]});
    if let Some(decision) = decision {
        step["decision"] = json!({"value":decision});
    }
    let _ = index;
    step
}

fn filter_step(step: &Value, filter: &str) -> bool {
    match filter {
        "decisions" => matches!(
            step["decision"]["value"].as_str(),
            Some(
                "asked_you"
                    | "denied_by_you"
                    | "denied_by_policy"
                    | "reduced"
                    | "approved"
                    | "expired"
            )
        ),
        "problems" => matches!(
            step["decision"]["value"].as_str(),
            Some("denied_by_you" | "denied_by_policy" | "failed" | "timed_out")
        ),
        _ => true,
    }
}

fn fold_reads(steps: &mut Vec<(Value, i64, i64)>) {
    let mut folded = Vec::<(Value, i64, i64)>::new();
    for current in steps.drain(..) {
        let fold = folded.last().is_some_and(|last| {
            last.0["sentence"]["key"] == "step.read"
                && current.0["sentence"]["key"] == "step.read"
                && last.0["agent"]["id"] == current.0["agent"]["id"]
                && last.0["decision"]["value"] == "allowed"
                && current.0["decision"]["value"] == "allowed"
                && current.2.saturating_sub(last.2) <= 5000
        });
        if fold {
            let last = folded.last_mut().unwrap();
            let first = last.0["sentence"]["args"]["path"].clone();
            let n = last.0["sentence"]["args"]["n"].as_u64().unwrap_or(0) + 1;
            last.0["sentence"] = json!({"key":"step.read_many","args":{"first":first,"n":n}});
            last.0["event_seqs"].as_array_mut().map(|seqs| {
                seqs.extend(
                    current.0["event_seqs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned(),
                )
            });
            last.0["ended_at"] = current.0["ended_at"].clone();
            last.0["duration_ms"] = json!(
                last.0["duration_ms"].as_u64().unwrap_or(0)
                    + current.0["duration_ms"].as_u64().unwrap_or(0)
            );
            last.1 = current.1;
        } else {
            folded.push(current);
        }
    }
    *steps = folded;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: i64, kind: &str, data: Value) -> Value {
        json!({"seq":seq,"ts":format!("2026-11-19T12:00:{:02}Z",seq%60),"ts_mono":seq*1000,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","root_session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","agent_role":"lead","source":"agent","type":kind,"data":data})
    }
    fn tool(seq: i64, call: &str) -> Value {
        event(
            seq,
            "tool.call",
            json!({"call_id":call,"server":"filesystem","tool":"fs.read","args":{"path":"/work/src/a.rs"}}),
        )
    }
    fn agents() -> Vec<Value> {
        vec![
            json!({"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","kind":"lead","role":"lead","colour_index":0,"initial":"L"}),
        ]
    }

    #[test]
    fn pairs_tool_call_decision_and_result() {
        let events = vec![
            tool(1, "c1"),
            event(
                2,
                "permit.decision",
                json!({"call_id":"c1","decision":"allow","rule":"allow"}),
            ),
            event(
                3,
                "tool.result",
                json!({"call_id":"c1","ok":true,"duration_ms":80}),
            ),
        ];
        let result = derive(&events, &agents(), None, None, 20, "everything");
        assert_eq!(result["steps"].as_array().unwrap().len(), 1);
        assert_eq!(result["steps"][0]["sentence"]["key"], "step.read");
        assert_eq!(result["steps"][0]["decision"]["value"], "allowed");
        assert_eq!(result["steps"][0]["event_seqs"], json!([1, 2, 3]));
    }

    #[test]
    fn identical_calls_keep_distinct_results_and_approvals() {
        let events = vec![
            tool(1, "one"),
            tool(2, "two"),
            event(
                3,
                "permit.decision",
                json!({"call_id":"one","decision":"ask","rule":"ask"}),
            ),
            event(
                4,
                "approval.requested",
                json!({"call_id":"one","id":"A1","code":"K7QF"}),
            ),
            event(
                5,
                "tool.result",
                json!({"call_id":"two","ok":true,"duration_ms":10}),
            ),
            event(
                6,
                "tool.result",
                json!({"call_id":"one","ok":true,"duration_ms":15}),
            ),
        ];
        let result = derive(&events, &agents(), None, None, 20, "everything");
        assert_eq!(result["steps"].as_array().unwrap().len(), 2);
        assert_eq!(result["steps"][0]["approval"]["id"], "A1");
        assert_eq!(result["steps"][0]["decision"]["value"], "asked_you");
    }

    #[test]
    fn missing_result_followed_by_exit_is_a_failed_step() {
        let events = vec![
            tool(1, "c1"),
            event(
                2,
                "permit.decision",
                json!({"call_id":"c1","decision":"allow","rule":"allow"}),
            ),
            event(3, "agent.exit", json!({"reason":"safety","code":1})),
        ];
        let result = derive(&events, &agents(), None, None, 20, "everything");
        assert_eq!(result["steps"].as_array().unwrap().len(), 1);
        assert_eq!(result["steps"][0]["decision"]["value"], "failed");
    }

    #[test]
    fn spawn_and_scope_narrowed_form_a_reduced_step() {
        let mut request = event(
            1,
            "delegation.request",
            json!({"delegation_id":"D1","role":"tester","task":"run tests"}),
        );
        request["session_id"] = json!("01ARZ3NDEKTSV4RRFFQ69G5FAW");
        let mut spawned = event(
            2,
            "delegation.spawned",
            json!({"delegation_id":"D1","member_id":"01ARZ3NDEKTSV4RRFFQ69G5FAW","role":"tester"}),
        );
        spawned["source"] = json!("supervisor");
        let mut narrowed = event(
            3,
            "scope.narrowed",
            json!({"delegation_id":"D1","narrowed":[{"dimension":"mount","item":"/work","reason":"mode_downgraded"}]}),
        );
        narrowed["source"] = json!("supervisor");
        let result = derive(
            &[request, spawned, narrowed],
            &agents(),
            None,
            None,
            20,
            "everything",
        );
        assert_eq!(result["steps"][0]["decision"]["value"], "reduced");
    }

    #[test]
    fn the_three_filters_include_only_their_contract_steps() {
        let events = vec![
            tool(1, "a"),
            event(
                2,
                "permit.decision",
                json!({"call_id":"a","decision":"allow","rule":"allow"}),
            ),
            event(
                3,
                "tool.result",
                json!({"call_id":"a","ok":true,"duration_ms":10}),
            ),
            tool(4, "b"),
            event(
                5,
                "permit.decision",
                json!({"call_id":"b","decision":"deny","rule":"deny"}),
            ),
            event(
                6,
                "tool.result",
                json!({"call_id":"b","ok":false,"duration_ms":1}),
            ),
        ];
        assert_eq!(
            derive(&events, &agents(), None, None, 20, "everything")["steps"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            derive(&events, &agents(), None, None, 20, "decisions")["steps"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            derive(&events, &agents(), None, None, 20, "problems")["steps"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn step_pages_support_before_and_after_cursors() {
        let events = vec![
            event(1, "message.inbound", json!({"text":"one"})),
            event(2, "message.inbound", json!({"text":"two"})),
            event(3, "message.inbound", json!({"text":"three"})),
        ];
        let newer = derive(&events, &agents(), None, None, 2, "everything");
        assert_eq!(newer["steps"].as_array().unwrap().len(), 2);
        assert!(newer["next_before_seq"].is_number());
        assert_eq!(
            derive(&events, &agents(), None, Some(1), 20, "everything")["steps"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}
