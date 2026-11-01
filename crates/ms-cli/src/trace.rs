use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

pub fn run(home: &Path, args: &[String]) -> Result<(), String> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with('-'))
        .ok_or("trace requires a session id")?;
    let json_output = args.iter().any(|arg| arg == "--json");
    let filters = args
        .windows(2)
        .find(|pair| pair[0] == "--events")
        .map(|pair| {
            pair[1]
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
        });
    let data = ms_runtime::trace::build(home, id, filters.as_ref())?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&data.events).map_err(|e| e.to_string())?
        );
    } else {
        render_text(&data.sessions, &data.events);
    }
    Ok(())
}

fn render_text(sessions: &[ms_runtime::trace::TraceSession], events: &[Value]) {
    for (index, session) in sessions.iter().enumerate() {
        let scope = session
            .scope
            .as_ref()
            .map(scope_summary)
            .unwrap_or_else(|| "scope unavailable".into());
        let branch = if session.parent.is_some() {
            if index + 1 == sessions.len() {
                "└─"
            } else {
                "├─"
            }
        } else {
            ""
        };
        println!(
            "{branch} {} {} {}  {scope}",
            session.id, session.role, session.status
        );
        let rows = events
            .iter()
            .filter(|event| event["session_id"] == session.id);
        for event in rows {
            println!(
                "{}  {} {}",
                if session.parent.is_some() {
                    "   │"
                } else {
                    "├─"
                },
                event["seq"],
                format_event(event)
            );
        }
    }
}

fn scope_summary(scope: &Value) -> String {
    let mounts = scope
        .get("mounts")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let tools = scope
        .get("tools")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let creds = scope
        .get("credentials")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!("scope: {mounts} mounts · {tools} tools · {creds} creds")
}

fn format_event(event: &Value) -> String {
    let kind = event["type"].as_str().unwrap_or("event");
    let data = &event["data"];
    match kind {
        "tool.call" => format!(
            "tool.call {} {}",
            data["server"].as_str().unwrap_or(""),
            data["tool"].as_str().unwrap_or("")
        ),
        "model.call" => format!(
            "model.call {}ms in {} / out {}",
            data["duration_ms"].as_i64().unwrap_or(0),
            data["input_tokens"].as_i64().unwrap_or(0),
            data["output_tokens"].as_i64().unwrap_or(0)
        ),
        _ => kind.to_owned(),
    }
}
