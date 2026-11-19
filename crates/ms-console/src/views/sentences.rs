use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct StepSentence {
    pub key: String,
    pub args: Value,
}

pub fn sentence(_kind: &str, _data: &Value) -> StepSentence {
    StepSentence {
        key: "step.other".into(),
        args: Value::Object(serde_json::Map::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sent(kind: &str, data: Value, expected: &str) {
        assert_eq!(sentence(kind, &data).key, expected);
    }

    #[test]
    #[ignore = "red: T011 — event sentence mapping is not implemented"]
    fn sentences_cover_the_step_sentence_contract() {
        for (tool, key, args) in [
            ("fs.read", "step.read", json!({"path":"/work/a.rs"})),
            ("read_file", "step.read", json!({"path":"/work/a.rs"})),
            ("fs.list", "step.list", json!({"path":"/work","count":3})),
            ("fs.write", "step.write", json!({"path":"/work/a.rs"})),
            ("write_file", "step.write", json!({"path":"/work/a.rs"})),
            ("fs.edit", "step.edit", json!({"path":"/work/a.rs"})),
            (
                "grep",
                "step.search",
                json!({"pattern":"expires_at","count":6}),
            ),
            (
                "search_files",
                "step.search",
                json!({"pattern":"expires_at","count":6}),
            ),
            ("shell.run", "step.shell", json!({"command":"cargo test"})),
            (
                "web.fetch",
                "step.fetch",
                json!({"url":"https://example.com"}),
            ),
            (
                "web.search",
                "step.web_search",
                json!({"query":"refresh token"}),
            ),
            ("unknown.tool", "step.tool", json!({})),
        ] {
            let data = json!({"server":"test","tool":tool,"args":args});
            assert_eq!(sentence("tool.call", &data).key, key, "{tool}");
        }
        sent("delegation.request", json!({"role":"tester"}), "step.spawn");
        sent(
            "delegation.denied",
            json!({"role":"tester","reason":"limit"}),
            "step.spawn.denied",
        );
        sent("delegation.send", json!({"role":"tester"}), "step.send");
        sent(
            "delegation.reply",
            json!({"status":"done","summary":"all pass"}),
            "step.report",
        );
        sent(
            "delegation.dismissed",
            json!({"role":"tester"}),
            "step.dismiss",
        );
        sent("delegation.timeout", json!({"timeout":30}), "step.timeout");
        sent("message.inbound", json!({"text":"hello"}), "step.you_said");
        sent("turn.end", json!({"reply":true}), "step.replied");
        sent(
            "approval.requested",
            json!({"role":"tester","what":"a command"}),
            "step.approval.asked",
        );
        sent(
            "approval.resolved",
            json!({"decision":"approved","channel":"web"}),
            "step.approval.decided",
        );
        sent("approval.expired", json!({}), "step.approval.expired");
        sent("agent.exit", json!({"reason":"safety"}), "step.exit.safety");
        sent("unrecognized", json!({}), "step.other");
    }

    #[test]
    #[ignore = "red: T012 — sentence arguments are not yet bounded and sanitized"]
    fn sentence_arguments_are_single_line_bounded_and_paths_are_shortened() {
        let result = sentence(
            "tool.call",
            &json!({"server":"fs","tool":"fs.read","args":{"path":"/work/src/auth/refresh.rs"}}),
        );
        assert_eq!(result.args["path"], "refresh.rs");
        let hostile = sentence("message.inbound", &json!({"text":"hello\n<script>"}));
        assert!(!hostile.args["text"].as_str().unwrap().contains('\n'));
        assert!(hostile.args["text"].as_str().unwrap().chars().count() <= 120);
    }

    #[test]
    #[ignore = "red: T013 — pending tool decisions do not select the ask sentence"]
    fn an_asked_shell_call_uses_the_pending_sentence() {
        let result = sentence(
            "tool.call",
            &json!({"server":"shell","tool":"shell.run","args":{"command":"cargo test"},"decision":"ask"}),
        );
        assert_eq!(result.key, "step.shell.ask");
    }
}
