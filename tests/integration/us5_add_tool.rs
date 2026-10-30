#[path = "us3_support.rs"]
mod support;

use std::{fs, path::Path};

#[test]
fn added_stdio_tool_is_available_without_rebuilding() {
    let Some(harness) = support::start() else {
        return;
    };
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/us5_stdio_mcp.py");
    fs::copy(script, harness.home.join("work/weather_mcp.py")).unwrap();
    let command = "python3 /work/weather_mcp.py";
    let add = harness
        .run_cli(&["tools", "add", "fixture-python", "--stdio", command])
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let microswarm = harness.home.join("config/microswarm.toml");
    let mut settings = fs::read_to_string(&microswarm).unwrap();
    settings = settings.replace(
        "{ server = \"microswarm-tools\", tools = \"*\", policy = \"allow\" }",
        "{ server = \"microswarm-tools\", tools = \"*\", policy = \"allow\" }, { server = \"fixture-python\", tools = \"*\", policy = \"allow\" }",
    );
    fs::write(microswarm, settings).unwrap();
    let output = harness.chat(None, &["call fixture-python/weather"]);
    let output = output.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("27 C and clear"),
        "agent should return the Python MCP tool result: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(harness.home.join("work/weather.called").is_file());
}
