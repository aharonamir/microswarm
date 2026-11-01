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
    });
    assert_eq!(actual["host"]["doctor"], serde_json::to_value(expected.iter().map(|check| serde_json::json!({"check":check.name,"result":check.result,"detail":check.detail})).collect::<Vec<_>>()).unwrap());
    fs::remove_dir_all(home).unwrap();
}
