use serde_json::{Value, json};

pub fn derive(
    sessions: &[Value],
    approvals: &[Value],
    supervisor_started_at: Option<&str>,
) -> Value {
    let agents = |session: &Value| session.get("agents").cloned().unwrap_or_else(|| json!([]));
    let running: Vec<Value> = sessions.iter().filter(|s| s["status_words"]["tone"] == "working" || s["in_flight"] == true).map(|s| json!({
        "session_id":s["id"],"title":s["title"],"channel":s["channel"],"status":s["status_words"],"agents":agents(s),"latest":{"who":"lead","text_or_sentence":s["latest"]},"started_at":s["created_at"]
    })).collect();
    let needs_you: Vec<Value> = approvals.iter().filter(|a|a["state"]=="pending").map(|a|json!({
        "approval_id":a["id"],"code":a["code"],"session_id":a["root_session_id"],"session_title":a["session_title"],"agent":a["agent"],"sentence":a["sentence"],"runs_in":a["runs_in"],"expires_at":a["expires_at"]
    })).collect();
    let ended_today: Vec<Value> = sessions.iter().filter(|s|s["closed_at"].is_string()).map(|s|json!({
        "session_id":s["id"],"title":s["title"],"channel":s["channel"],"outcome_key":s["status_words"]["key"],"ended_at":s["closed_at"],"agents":agents(s),"last_line":s["last_line"]
    })).collect();
    json!({"summary":{"needs_you":needs_you.len(),"running":running.len()},"needs_you":needs_you,"running":running,"ended_today":ended_today,"system":{"containers":0,"approvals_today":approvals.len(),"supervisor_started_at":supervisor_started_at}})
}

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
