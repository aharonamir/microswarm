use serde_json::{Value, json};

pub fn derive(events: &[Value], agents: &[Value], steps: &[Value], since: Option<&str>) -> Value {
    let selected: Vec<&Value> = events
        .iter()
        .filter(|event| {
            since.is_none_or(|since| event["ts"].as_str().is_some_and(|ts| ts >= since))
        })
        .collect();
    let start = since.map(str::to_owned).or_else(|| {
        selected
            .first()
            .and_then(|e| e["ts"].as_str().map(str::to_owned))
    });
    let end = selected
        .last()
        .and_then(|e| e["ts"].as_str().map(str::to_owned));
    let mut lanes: Vec<Value> = agents
        .iter()
        .map(|agent| {
            json!({
                "agent": agent, "spans": [], "marks": []
            })
        })
        .collect();
    lanes.push(
        json!({"agent":{"id":"you","kind":"you","role":"you","initial":"Y"},"spans":[],"marks":[]}),
    );
    let lane_index = |event: &Value| -> usize {
        let id = event["session_id"].as_str().unwrap_or_default();
        agents
            .iter()
            .position(|agent| agent["id"].as_str() == Some(id))
            .unwrap_or(0)
            .min(agents.len().saturating_sub(1))
    };
    let mut waiting_ms = 0i64;
    let mut active: Vec<Option<(String, i64)>> = vec![None; lanes.len()];
    let mut last_work: Vec<Option<(String, i64)>> = vec![None; lanes.len()];
    for event in &selected {
        let i = lane_index(event);
        let typ = event["type"].as_str().unwrap_or_default();
        let ts = event["ts"].as_str().unwrap_or_default().to_owned();
        let mono = event["ts_mono"].as_i64().unwrap_or(0);
        match typ {
            "turn.start" => active[i] = Some((ts.clone(), mono)),
            "approval.requested" => {
                if let Some((work_start, work_mono)) = active[i].take() {
                    push_span(&mut lanes[i], work_start, ts.clone(), "working");
                    last_work[i] = Some((ts.clone(), mono.max(work_mono)));
                }
                active[i] = Some((ts.clone(), mono));
            }
            "approval.resolved" | "approval.granted" | "approval.denied" | "turn.end" => {
                if let Some((span_start, span_mono)) = active[i].take() {
                    let waiting = matches!(
                        typ,
                        "approval.resolved" | "approval.granted" | "approval.denied"
                    );
                    push_span(
                        &mut lanes[i],
                        span_start,
                        ts.clone(),
                        if waiting { "waiting_you" } else { "working" },
                    );
                    if waiting {
                        waiting_ms += mono.saturating_sub(span_mono);
                    } else {
                        last_work[i] = Some((ts.clone(), mono));
                    }
                }
            }
            _ => {}
        }
        let kind = match typ {
            "tool.call" | "tool.started" | "tool.finished" => Some("tool"),
            "message.inbound" | "message.outbound" => Some("message"),
            "delegation.reply" | "agent.report" => Some("report"),
            "approval.requested" | "approval.resolved" | "approval.granted" | "approval.denied" => {
                Some("approval")
            }
            "delegation.spawned" => Some("spawn"),
            "tool.denied" | "tool.failed" => Some("denied"),
            _ => None,
        };
        if let Some(kind) = kind {
            let mark = json!({"seq":event["seq"],"at":event["ts"],"kind":kind,"step_id":step_for(event, steps)});
            lanes[i]["marks"].as_array_mut().unwrap().push(mark.clone());
            if typ == "message.inbound" {
                lanes.last_mut().unwrap()["marks"]
                    .as_array_mut()
                    .unwrap()
                    .push(mark);
            }
        }
    }
    for (i, lane) in lanes.iter_mut().enumerate() {
        if let Some((start, _)) = active[i].take() {
            push_span(lane, start, end.clone().unwrap_or_default(), "working");
        }
        let marks = lane["marks"].as_array_mut().unwrap();
        if marks.iter().filter(|m| m["kind"] == "tool").count() > 200 {
            let tools: Vec<Value> = marks
                .iter()
                .filter(|m| m["kind"] == "tool")
                .cloned()
                .collect();
            let mut cursor = 0usize;
            marks.retain(|m| {
                if m["kind"] != "tool" {
                    return true;
                }
                let keep = cursor % tools.len().div_ceil(200) == 0;
                cursor += 1;
                keep
            });
        }
    }
    let duration = selected
        .iter()
        .filter_map(|e| e["ts_mono"].as_i64())
        .max()
        .unwrap_or(0)
        .saturating_sub(
            selected
                .iter()
                .filter_map(|e| e["ts_mono"].as_i64())
                .min()
                .unwrap_or(0),
        );
    let reduced = selected
        .iter()
        .filter(|e| e["type"] == "scope.narrowed")
        .count();
    let model_calls = selected
        .iter()
        .filter(|e| matches!(e["type"].as_str(), Some("model.call" | "model.request")))
        .count();
    let denied = steps
        .iter()
        .filter(|s| {
            matches!(
                s["decision"]["value"].as_str(),
                Some("denied_by_policy" | "failed")
            )
        })
        .count();
    let mut step_count = steps.len();
    if let Some((_, mono)) = last_work.iter().flatten().last() {
        let _ = mono;
    }
    if step_count == 0 {
        step_count = selected
            .iter()
            .filter(|e| e["type"] == "turn.start")
            .count();
    }
    json!({"range":{"start":start,"end":end},"stats":{"duration_ms":duration,"agents":agents.len(),"steps":step_count,"waiting_you_ms":waiting_ms,"reduced":reduced,"denied_or_failed":denied,"model_calls":model_calls},"lanes":lanes})
}

fn push_span(lane: &mut Value, start: String, end: String, state: &str) {
    let spans = lane["spans"].as_array_mut().unwrap();
    if state == "working" {
        if let Some(last) = spans.last_mut() {
            if last["state"] == "working" {
                let gap = last["end"]
                    .as_str()
                    .zip(start.get(..19))
                    .and_then(|(a, b)| {
                        let a = a.get(17..19)?.parse::<i64>().ok()?;
                        let b = b.get(17..19)?.parse::<i64>().ok()?;
                        Some((b - a).rem_euclid(60) * 1000)
                    })
                    .unwrap_or(i64::MAX);
                if gap < 2000 {
                    last["end"] = json!(end);
                    return;
                }
            }
        }
    }
    spans.push(json!({"start":start,"end":end,"state":state}));
}

fn step_for(event: &Value, steps: &[Value]) -> Value {
    let seq = event["seq"].as_i64();
    steps
        .iter()
        .find(|step| {
            step["event_seqs"]
                .as_array()
                .is_some_and(|seqs| seqs.iter().any(|s| s.as_i64() == seq))
        })
        .map(|s| s["id"].clone())
        .unwrap_or(Value::Null)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(seq: i64, session: &str, role: &str, typ: &str, mono: i64) -> Value {
        json!({"seq":seq,"ts":format!("2026-11-19T12:00:{:02}Z",seq),"ts_mono":mono,"session_id":session,"agent_role":role,"type":typ,"data":{}})
    }
    fn agents() -> Vec<Value> {
        vec![
            json!({"id":"lead-id","kind":"lead","role":"lead","colour_index":0,"initial":"L"}),
            json!({"id":"member-id","kind":"member","role":"tester","colour_index":1,"initial":"T"}),
        ]
    }

    #[test]
    fn creates_working_spans_and_merges_gaps_under_two_seconds() {
        let events = vec![
            event(1, "lead-id", "lead", "turn.start", 0),
            event(2, "lead-id", "lead", "turn.end", 1000),
            event(3, "lead-id", "lead", "turn.start", 2500),
            event(4, "lead-id", "lead", "turn.end", 4000),
        ];
        let timeline = derive(&events, &agents(), &[], None);
        assert_eq!(timeline["lanes"][0]["spans"].as_array().unwrap().len(), 1);
        assert_eq!(timeline["lanes"][0]["spans"][0]["state"], "working");
    }

    #[test]
    fn approval_wait_span_is_separate_from_work() {
        let events = vec![
            event(1, "member-id", "tester", "turn.start", 0),
            event(2, "member-id", "tester", "approval.requested", 1000),
            event(3, "member-id", "tester", "approval.resolved", 3000),
            event(4, "member-id", "tester", "turn.end", 4000),
        ];
        let timeline = derive(&events, &agents(), &[], None);
        let spans = timeline["lanes"][1]["spans"].as_array().unwrap();
        assert!(spans.iter().any(|span| span["state"] == "waiting_you"));
        assert!(spans.iter().any(|span| span["state"] == "working"));
    }

    #[test]
    fn adds_tool_message_report_and_approval_marks() {
        let events = vec![
            event(1, "lead-id", "lead", "tool.call", 0),
            event(2, "member-id", "tester", "delegation.reply", 1000),
            event(3, "lead-id", "lead", "message.inbound", 2000),
            event(4, "member-id", "tester", "approval.requested", 3000),
        ];
        let timeline = derive(&events, &agents(), &[], None);
        assert!(
            timeline["lanes"][0]["marks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|mark| mark["kind"] == "tool")
        );
        assert!(
            timeline["lanes"][0]["marks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|mark| mark["kind"] == "message")
        );
        assert!(
            timeline["lanes"][1]["marks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|mark| mark["kind"] == "report")
        );
        assert!(
            timeline["lanes"][1]["marks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|mark| mark["kind"] == "approval")
        );
    }

    #[test]
    fn caps_tool_marks_per_lane_at_two_hundred() {
        let events = (0..500)
            .map(|index| event(index, "lead-id", "lead", "tool.call", index))
            .collect::<Vec<_>>();
        let timeline = derive(&events, &agents(), &[], None);
        assert!(timeline["lanes"][0]["marks"].as_array().unwrap().len() <= 200);
    }

    #[test]
    fn reports_duration_steps_waiting_reduced_denied_and_model_counts() {
        let events = vec![
            event(1, "lead-id", "lead", "turn.start", 0),
            event(2, "lead-id", "lead", "approval.requested", 1000),
            event(3, "lead-id", "lead", "approval.resolved", 4000),
            event(4, "lead-id", "lead", "scope.narrowed", 5000),
            event(5, "lead-id", "lead", "model.call", 6000),
        ];
        let steps = vec![json!({"decision":{"value":"denied_by_policy"}})];
        let timeline = derive(&events, &agents(), &steps, None);
        assert_eq!(timeline["stats"]["duration_ms"], 6000);
        assert_eq!(timeline["stats"]["steps"], 1);
        assert!(timeline["stats"]["waiting_you_ms"].as_i64().unwrap() > 0);
        assert_eq!(timeline["stats"]["reduced"], 1);
        assert_eq!(timeline["stats"]["denied_or_failed"], 1);
        assert_eq!(timeline["stats"]["model_calls"], 1);
    }

    #[test]
    fn since_crops_lanes_and_range() {
        let events = vec![
            event(1, "lead-id", "lead", "tool.call", 0),
            event(2, "lead-id", "lead", "tool.call", 1000),
        ];
        let timeline = derive(&events, &agents(), &[], Some("2026-11-19T12:00:02Z"));
        assert_eq!(timeline["range"]["start"], "2026-11-19T12:00:02Z");
        assert_eq!(timeline["lanes"][0]["marks"].as_array().unwrap().len(), 1);
    }
}
