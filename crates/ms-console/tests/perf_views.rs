use serde_json::{Value, json};
use std::{hint::black_box, time::Instant};

fn agent(index: usize) -> Value {
    json!({"id":format!("agent-{index}"),"kind":if index==0{"lead"}else{"member"},"role":format!("role-{index}"),"colour_index":index,"initial":"A"})
}

#[test]
fn load_fixture_views_fit_warm_budgets() {
    let agents = (0..6).map(agent).collect::<Vec<_>>();
    let mut events = Vec::with_capacity(2000);
    events.push(json!({"seq":0,"ts":"2026-11-19T12:00:00Z","ts_mono":0,"session_id":"agent-0","agent_role":"lead","type":"turn.start","data":{}}));
    for seq in 1..2000 {
        let index = seq % 6;
        let typ = if seq % 9 == 0 {
            "delegation.spawned"
        } else if seq % 7 == 0 {
            "tool.call"
        } else {
            "message.inbound"
        };
        events.push(json!({"seq":seq,"ts":format!("2026-11-19T12:{:02}:{:02}Z",(seq/60)%60,seq%60),"ts_mono":seq as i64*10,"session_id":format!("agent-{index}"),"agent_role":format!("role-{index}"),"type":typ,"data":{"member_id":format!("agent-{index}"),"role":format!("role-{index}"),"task":"load test task"}}));
    }
    let steps =
        ms_console::views::steps::derive(&events, &agents, None, None, 200, "everything")["steps"]
            .as_array()
            .cloned()
            .unwrap_or_default();
    let _ = ms_console::views::team::derive(&events, &agents, &steps);
    let _ = ms_console::views::timeline::derive(&events, &agents, &steps, None);
    let team_start = Instant::now();
    black_box(ms_console::views::team::derive(&events, &agents, &steps));
    let team = team_start.elapsed();
    let steps_start = Instant::now();
    black_box(ms_console::views::steps::derive(
        &events,
        &agents,
        None,
        None,
        200,
        "everything",
    ));
    let steps_time = steps_start.elapsed();
    let timeline_start = Instant::now();
    black_box(ms_console::views::timeline::derive(
        &events, &agents, &steps, None,
    ));
    let timeline = timeline_start.elapsed();
    println!(
        "warm load view timings: team={}us steps={}us timeline={}us",
        team.as_micros(),
        steps_time.as_micros(),
        timeline.as_micros()
    );
    assert!(
        team < std::time::Duration::from_millis(150),
        "team view took {team:?}"
    );
    assert!(
        steps_time < std::time::Duration::from_millis(300),
        "steps page took {steps_time:?}"
    );
    assert!(
        timeline < std::time::Duration::from_millis(300),
        "timeline took {timeline:?}"
    );
}
