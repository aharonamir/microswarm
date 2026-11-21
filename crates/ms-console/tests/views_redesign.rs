mod timeline {
    use ms_console::views::timeline::*;
    use serde_json::{Value, json};
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
}

mod team {
    use ms_console::views::team::*;
    use serde_json::{Value, json};
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

        #[test]
        fn caps_group_rows_and_reports_the_more_count() {
            let mut events = vec![ev(1, "turn.start", "lead", "lead", json!({}))];
            for index in 0..5 {
                events.push(ev(index + 2, "delegation.spawned", "lead", "lead", json!({"member_id":format!("m{index}"),"delegation_id":format!("d{index}"),"role":"tester","task":"task"})));
            }
            let team = derive(&events, &[], &[]);
            assert_eq!(team["groups"][0]["members"].as_array().unwrap().len(), 4);
            assert_eq!(team["groups"][0]["more"], 1);
        }
    }
}

mod reach {
    use ms_console::views::reach::*;
    use serde_json::{Value, json};
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
            let scope = json!({"version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"delegation","source_ref":"team","mounts":[{"host_path":"/tmp","container_path":"/a","mode":"ro"}],"credentials":[],"tools":[],"delegation":null,"limits":{"memory_mb":1,"pids":1,"tmp_mb":1,"max_turns":1,"max_tokens":1},"narrowed":narrowed,"hash":"x"});
            let view = json!({"granted":scope,"lead_scope":{"mounts":[{"container_path":"/a","mode":"rw"}]},"rows":[{"dimension":"mounts","requested":[{"container_path":"/a","mode":"rw"}],"lead":["/a","/b"],"ceiling":[]} ]});
            let reach = derive(&view, false).unwrap();
            assert_eq!(reach["reduced"].as_array().unwrap().len(), 5);
            assert_eq!(
                reach["reduced"][0]["text"]["key"],
                "reach.reason.not_in_lead_scope"
            );
            assert_eq!(reach["derivation"]["requested"][0]["text"], "/a");
            assert_eq!(reach["derivation"]["requested"][0]["lowered"], true);
        }
        #[test]
        fn lead_has_no_derivation_or_reductions() {
            let scope = json!({"version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config","source_ref":"cfg","mounts":[],"credentials":[],"tools":[],"delegation":null,"limits":{"memory_mb":1,"pids":1,"tmp_mb":1,"max_turns":1,"max_tokens":1},"narrowed":[],"hash":"x"});
            let result = derive(&json!({"granted":scope}), true).unwrap();
            assert!(result["derivation"].is_null());
            assert_eq!(result["reduced"], json!([]));
        }
    }
}

mod home {
    use ms_console::views::home::*;
    use serde_json::json;
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn builds_needs_you_running_and_ended_sections() {
            let sessions = vec![
                json!({"id":"s1","title":"Work","in_flight":true,"status_words":{"tone":"working"},"created_at":"now","agents":[]}),
                json!({"id":"s2","title":"Done","closed_at":"then","status_words":{"key":"status.finished"},"agents":[]}),
            ];
            let approvals = vec![
                json!({"id":"a1","code":"ABCD","state":"pending","root_session_id":"s1","expires_at":"later"}),
            ];
            let home = derive(&sessions, &approvals, Some("start"));
            assert_eq!(home["summary"]["needs_you"], 1);
            assert_eq!(home["summary"]["running"], 1);
            assert_eq!(home["ended_today"][0]["session_id"], "s2");
            assert_eq!(home["system"]["supervisor_started_at"], "start");
        }
    }
}

mod phases {
    use ms_console::views::phases::*;
    use serde_json::{Value, json};
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
}

mod steps {
    use ms_console::views::steps::*;
    use serde_json::{Value, json};
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
        fn failed_allowed_tool_is_a_problem() {
            let events = vec![
                tool(1, "failed"),
                event(
                    2,
                    "permit.decision",
                    json!({"call_id":"failed","decision":"allow"}),
                ),
                event(3, "tool.result", json!({"call_id":"failed","ok":false})),
            ];
            let result = derive(&events, &agents(), None, None, 20, "problems");
            assert_eq!(result["steps"][0]["decision"]["value"], "failed");
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
}
