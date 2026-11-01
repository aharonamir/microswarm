#[path = "us3_support.rs"]
mod support;

use std::{fs, path::Path};

#[test]
fn added_stdio_tool_is_available_without_rebuilding() {
    let Some(harness) = support::start() else {
        return;
    };
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/us5_stdio_mcp.sh");
    fs::copy(script, harness.home.join("work/weather_mcp.sh")).unwrap();
    let command = "/bin/sh /work/weather_mcp.sh";
    let add = harness
        .run_cli(&["tools", "add", "fixture-shell", "--stdio", command])
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
        "{ server = \"microswarm-tools\", tools = \"*\", policy = \"allow\" }, { server = \"fixture-shell\", tools = \"*\", policy = \"allow\" }",
    );
    fs::write(microswarm, settings).unwrap();
    let output = harness.chat(None, &["call fixture-shell/weather"]);
    let output = output.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("27 C and clear"),
        "agent should return the shell MCP tool result: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
