use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct StepSentence {
    pub key: String,
    pub args: Value,
}

pub fn sentence(kind: &str, data: &Value) -> StepSentence {
    let (key, args) = match kind {
        "tool.call" => tool_sentence(data),
        "delegation.request" | "delegation.spawned" => (
            "step.spawn",
            serde_json::json!({
                "role": text(data, "role"),
                "reach": text(data, "reach").pipe_or_else(|| reach_text(data)),
            }),
        ),
        "delegation.denied" => (
            "step.spawn.denied",
            serde_json::json!({"role":text(data,"role"),"reason":text(data,"reason").pipe_or("policy")}),
        ),
        "delegation.send" => ("step.send", serde_json::json!({"role":text(data,"role")})),
        "delegation.reply" => (
            "step.report",
            serde_json::json!({"status":text(data,"status").pipe_or("done"),"summary":text(data,"summary").pipe_or_else(||text(data,"text"))}),
        ),
        "delegation.dismiss" | "delegation.dismissed" => (
            "step.dismiss",
            serde_json::json!({"role":text(data,"role")}),
        ),
        "delegation.timeout" => (
            "step.timeout",
            serde_json::json!({"timeout":text(data,"timeout").pipe_or("the time limit")}),
        ),
        "message.inbound" => (
            "step.you_said",
            serde_json::json!({"text":bounded(text(data,"text"),120)}),
        ),
        "turn.end" => ("step.replied", serde_json::json!({})),
        "approval.requested" => (
            "step.approval.asked",
            serde_json::json!({"role":text(data,"role").pipe_or("member"),"what":approval_what(data)}),
        ),
        "approval.resolved" => (
            "step.approval.decided",
            serde_json::json!({"decision":text(data,"decision"),"channel":text(data,"channel").pipe_or("another channel")}),
        ),
        "approval.expired" => ("step.approval.expired", serde_json::json!({})),
        "agent.exit" => {
            let reason = match data["reason"].as_str().unwrap_or("safety") {
                "ok" => "ok",
                "transient" => "transient",
                "config" => "config",
                _ => "safety",
            };
            return StepSentence {
                key: format!("step.exit.{reason}"),
                args: serde_json::json!({}),
            };
        }
        _ => ("step.other", serde_json::json!({})),
    };
    StepSentence {
        key: key.into(),
        args: clean_args(args),
    }
}

fn tool_sentence(data: &Value) -> (&'static str, Value) {
    let tool = data["tool"].as_str().unwrap_or("unknown");
    let args = data
        .get("args")
        .or_else(|| data.get("arguments"))
        .unwrap_or(&Value::Null);
    match tool {
        "fs.read" | "read_file" => (
            "step.read",
            serde_json::json!({"path":short_path(text(args,"path"))}),
        ),
        "fs.list" => (
            "step.list",
            serde_json::json!({"path":short_path(text(args,"path")),"count":number(data,"count")}),
        ),
        "fs.write" | "write_file" => (
            "step.write",
            serde_json::json!({"path":short_path(text(args,"path"))}),
        ),
        "fs.edit" => (
            "step.edit",
            serde_json::json!({"path":short_path(text(args,"path"))}),
        ),
        "grep" | "search_files" => (
            "step.search",
            serde_json::json!({"pattern":text(args,"pattern"),"count":number(data,"count")}),
        ),
        "shell.run" if data["decision"] == "ask" => (
            "step.shell.ask",
            serde_json::json!({"command":text(args,"command")}),
        ),
        "shell.run" => (
            "step.shell",
            serde_json::json!({"command":text(args,"command")}),
        ),
        "web.fetch" => (
            "step.fetch",
            serde_json::json!({"host":host(text(args,"url"))}),
        ),
        "web.search" => (
            "step.web_search",
            serde_json::json!({"query":text(args,"query")}),
        ),
        _ => (
            "step.tool",
            serde_json::json!({"server":text(data,"server"),"tool":tool}),
        ),
    }
}

fn text(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default()
}

fn number(data: &Value, key: &str) -> Value {
    data.get(key)
        .and_then(Value::as_i64)
        .map(Value::from)
        .unwrap_or_else(|| Value::from(0))
}

fn bounded(input: String, limit: usize) -> String {
    let single = input
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<Vec<_>>();
    if single.len() > limit {
        let mut value = single
            .into_iter()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        value.push('…');
        value.trim().to_owned()
    } else {
        single.into_iter().collect::<String>().trim().to_owned()
    }
}

fn short_path(path: String) -> String {
    let relative = path.strip_prefix("/work/").unwrap_or(&path);
    let display = relative
        .rsplit('/')
        .next()
        .filter(|name| name.len() < relative.len())
        .unwrap_or(relative);
    bounded(display.to_owned(), 120)
}

fn host(url: String) -> String {
    let without_scheme = url.split_once("://").map(|(_, host)| host).unwrap_or(&url);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    bounded(host.to_owned(), 120)
}

fn reach_text(data: &Value) -> String {
    if let Some(path) = data["scope"]["mounts"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["container_path"].as_str())
    {
        format!("read access to `{path}`")
    } else {
        "no file access".into()
    }
}

fn approval_what(data: &Value) -> String {
    let command = data["arguments"]["command"]
        .as_str()
        .or(data["args"]["command"].as_str());
    command
        .map(|s| format!("run `{s}`"))
        .unwrap_or_else(|| "use a tool".into())
}

fn clean_args(mut args: Value) -> Value {
    if let Some(values) = args.as_object_mut() {
        for (key, value) in values.iter_mut() {
            if let Some(text) = value.as_str() {
                let limit = if key == "text" { 120 } else { 120 };
                *value = Value::String(bounded(text.to_owned(), limit));
            }
        }
    }
    args
}

trait OptionText {
    fn pipe_or(self, default: &str) -> String;
    fn pipe_or_else(self, fallback: impl FnOnce() -> String) -> String;
}

impl OptionText for String {
    fn pipe_or(self, default: &str) -> String {
        if self.is_empty() {
            default.into()
        } else {
            self
        }
    }
    fn pipe_or_else(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() { fallback() } else { self }
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
    fn an_asked_shell_call_uses_the_pending_sentence() {
        let result = sentence(
            "tool.call",
            &json!({"server":"shell","tool":"shell.run","args":{"command":"cargo test"},"decision":"ask"}),
        );
        assert_eq!(result.key, "step.shell.ask");
    }
}
