use serde_json::{Value, json};

pub fn derive(
    _events: &[Value],
    _agents: &[Value],
    _before_seq: Option<i64>,
    _after_seq: Option<i64>,
    _limit: usize,
    _filter: &str,
) -> Value {
    json!({"steps":[],"phases":[],"next_before_seq":null,"last_seq":0})
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
    #[ignore = "red: T021 — tool call, decision, and result are not paired"]
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
    #[ignore = "red: T022 — identical tool calls are not paired by their call ids"]
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
    #[ignore = "red: T023 — unfinished tool calls are lost when an agent exits"]
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
    #[ignore = "red: T024 — spawn scope reductions are not attached"]
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
    #[ignore = "red: T025 — step filters are not implemented"]
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
    #[ignore = "red: T026 — step pages do not honor before_seq and after_seq"]
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
