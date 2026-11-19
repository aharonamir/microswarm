pub mod phases;
pub mod sentences;
pub mod steps;
pub mod timeline;

use ms_runtime::trace::TraceData;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn agents(trace: &TraceData) -> Vec<Value> {
    let mut used_initials = BTreeMap::<String, usize>::new();
    trace
        .sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let initial = session.role.chars().next().unwrap_or('M').to_uppercase().to_string();
            let count = used_initials.entry(initial.clone()).or_default();
            *count += 1;
            let label = if *count > 1 {
                session.role.chars().take(2).collect::<String>()
            } else {
                initial
            };
            let colour = if index == 0 { 0 } else { ((index - 1) % 6) + 1 };
            json!({"id":session.id,"kind":if session.parent.is_some(){"member"}else{"lead"},"role":session.role,"colour_index":colour,"initial":label})
        })
        .collect()
}

pub fn agent_by_id(agents: &[Value], id: &str) -> Value {
    agents
        .iter()
        .find(|agent| agent["id"] == id)
        .cloned()
        .unwrap_or_else(
            || json!({"id":id,"kind":"member","role":"member","colour_index":1,"initial":"M"}),
        )
}
