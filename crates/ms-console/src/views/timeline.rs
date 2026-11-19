use serde_json::{Value, json};

pub fn derive(
    _events: &[Value],
    _agents: &[Value],
    _steps: &[Value],
    _since: Option<&str>,
) -> Value {
    json!({"range":{"start":null,"end":null},"stats":{"duration_ms":0,"agents":0,"steps":0,"waiting_you_ms":0,"reduced":0,"denied_or_failed":0,"model_calls":0},"lanes":[]})
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
    #[ignore = "red: T041 — lane spans and short-gap merging are missing"]
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
    #[ignore = "red: T042 — waiting-for-you spans do not supersede work"]
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
    #[ignore = "red: T043 — event marks are not mapped to lanes"]
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
    #[ignore = "red: T044 — tool ticks are not capped at 200"]
    fn caps_tool_marks_per_lane_at_two_hundred() {
        let events = (0..500)
            .map(|index| event(index, "lead-id", "lead", "tool.call", index))
            .collect::<Vec<_>>();
        let timeline = derive(&events, &agents(), &[], None);
        assert!(timeline["lanes"][0]["marks"].as_array().unwrap().len() <= 200);
    }

    #[test]
    #[ignore = "red: T045 — timeline summary stats are missing"]
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
    #[ignore = "red: T046 — since does not crop the timeline range"]
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
