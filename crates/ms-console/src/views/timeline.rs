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
        let id = event["data"]["requesting_session_id"]
            .as_str()
            .or(event["session_id"].as_str())
            .unwrap_or_default();
        agents
            .iter()
            .position(|agent| agent["id"].as_str() == Some(id))
            .unwrap_or(0)
            .min(agents.len().saturating_sub(1))
    };
    let mut waiting_ms = 0i64;
    let mut active: Vec<Option<(String, i64, &'static str)>> = vec![None; lanes.len()];
    let mut last_work: Vec<Option<(String, i64)>> = vec![None; lanes.len()];
    for event in &selected {
        let i = lane_index(event);
        let typ = event["type"].as_str().unwrap_or_default();
        let ts = event["ts"].as_str().unwrap_or_default().to_owned();
        let mono = event["ts_mono"].as_i64().unwrap_or(0);
        match typ {
            "turn.start" => active[i] = Some((ts.clone(), mono, "working")),
            "approval.requested" => {
                if let Some((work_start, work_mono, state)) = active[i].take() {
                    push_span(&mut lanes[i], work_start, ts.clone(), state);
                    last_work[i] = Some((ts.clone(), mono.max(work_mono)));
                }
                active[i] = Some((ts.clone(), mono, "waiting_you"));
            }
            "approval.resolved" | "approval.granted" | "approval.denied" => {
                if let Some((span_start, span_mono, state)) = active[i].take() {
                    push_span(&mut lanes[i], span_start, ts.clone(), state);
                    if state == "waiting_you" {
                        waiting_ms += mono.saturating_sub(span_mono);
                    }
                }
                active[i] = Some((ts.clone(), mono, "working"));
            }
            "turn.end" => {
                if let Some((span_start, span_mono, state)) = active[i].take() {
                    push_span(&mut lanes[i], span_start, ts.clone(), state);
                    if state == "waiting_you" {
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
            "scope.narrowed" => Some("reduced"),
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
        if let Some((start, mono, state)) = active[i].take() {
            let end_mono = selected
                .iter()
                .filter_map(|event| event["ts_mono"].as_i64())
                .max()
                .unwrap_or(mono);
            push_span(lane, start, end.clone().unwrap_or_default(), state);
            if state == "waiting_you" {
                waiting_ms += end_mono.saturating_sub(mono);
            }
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
                let keep = cursor.is_multiple_of(tools.len().div_ceil(200));
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
    if state == "working"
        && let Some(last) = spans.last_mut()
        && last["state"] == "working"
    {
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
    spans.push(json!({"start":start,"end":end,"state":state}));
}

fn step_for(event: &Value, steps: &[Value]) -> Value {
    let seq = event["seq"].as_i64();
    steps
        .iter()
        .find(|step| {
            step["agent"]["id"] == event["session_id"]
                && step["event_seqs"]
                    .as_array()
                    .is_some_and(|seqs| seqs.iter().any(|s| s.as_i64() == seq))
        })
        .map(|s| s["step_id"].clone())
        .unwrap_or(Value::Null)
}
