use serde_json::{Value, json};

pub fn derive(_events: &[Value], _steps: &[Value]) -> Vec<Value> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: i64, typ: &str, role: &str, data: Value) -> Value {
        json!({"seq":seq,"ts":format!("2026-11-19T12:00:{:02}Z",seq),"ts_mono":seq*1000,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","agent_role":role,"type":typ,"data":data})
    }

    #[test]
    #[ignore = "red: T031 — operator messages do not start planning or work phases"]
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
    #[ignore = "red: T032 — parallel member start and completion phases are not derived"]
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
    #[ignore = "red: T033 — approvals do not create waiting-for-you phases"]
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
    #[ignore = "red: T034 — missing lifecycle events have no degraded phase"]
    fn missing_events_still_produce_a_degraded_phase() {
        let phases = derive(&[], &[]);
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0]["title_key"], "phase.lead_works");
    }
}
