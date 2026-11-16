use ms_console::read;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
#[test]
fn host_doctor_rows_match_shared_doctor_checks() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join("microswarm.toml"),
        r#"[runtime]
socket = "/var/run/docker.sock"
image = "microswarm-agent:test"
[model]
provider = "test"
credential = "provider-key"
"#,
    )
    .unwrap();
    let actual = read::host(&home);
    let expected = ms_runtime::doctor::run(ms_runtime::doctor::DoctorInput {
        config_path: home.join("microswarm.toml").to_string_lossy().as_ref(),
        runtime_socket: "/var/run/docker.sock",
        agent_image: "microswarm-agent:test",
        model: "test",
        credential: "provider-key",
        search: None,
    });
    assert_eq!(actual["host"]["doctor"], serde_json::to_value(expected.iter().map(|check| serde_json::json!({"check":check.name,"result":check.result,"detail":check.detail})).collect::<Vec<_>>()).unwrap());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn configured_search_is_reported_in_doctor_and_host_tools() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-search-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(home.join("credentials")).unwrap();
    fs::write(
        home.join("credentials/web-search"),
        "synthetic-search-secret",
    )
    .unwrap();
    fs::write(
        home.join("microswarm.toml"),
        r#"[runtime]
socket = "/var/run/docker.sock"
image = "microswarm-agent:test"
[model]
provider = "test"
credential = "provider-key"
[search]
provider = "tavily"
credential = "web-search"
[lead.scope]
credentials = ["provider-key", "web-search"]
tools = [{ server = "microswarm-tools", tools = "*", policy = "ask" }]
"#,
    )
    .unwrap();

    let host = read::host(&home)["host"].clone();
    let search_check = host["doctor"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["check"] == "web search")
        .expect("doctor should report web search");
    assert_eq!(search_check["result"], "pass");
    assert!(search_check["detail"].as_str().unwrap().contains("Tavily"));
    let search_tool = host["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["id"] == "web.search")
        .expect("Host should list web.search");
    assert_eq!(search_tool["provider"], "tavily");
    assert_eq!(search_tool["enabled"], true);
    let serialized = host.to_string();
    assert!(!serialized.contains("synthetic-search-secret"));
    fs::remove_dir_all(home).unwrap();
}
