use ms_proto::Permission;
use ms_supervisor::{
    config::{Language, load_config_dir},
    i18n::{MessageKey, all_message_keys, translate},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "microswarm-config-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_config(dir: &Path, file: &str, body: &str) {
    fs::write(dir.join(file), body).unwrap();
}

#[test]
fn all_supervisor_messages_have_english_and_chinese_text() {
    for &key in all_message_keys() {
        assert!(!translate(Language::En, key).trim().is_empty());
        assert!(!translate(Language::Zh, key).trim().is_empty());
    }
    assert_ne!(
        translate(Language::En, MessageKey::ApprovalRequested),
        translate(Language::Zh, MessageKey::ApprovalRequested)
    );
}

#[test]
fn config_loader_reads_all_four_files_and_validates_policy_and_server_transport() {
    let dir = temp_dir();
    write_config(
        &dir,
        "microswarm.toml",
        r#"
[home]
retention_days = 30
lang = "zh"
[runtime]
socket = "unix:///run/user/1000/podman/podman.sock"
image = "microswarm-agent:dev"
[limits]
max_sessions = 200
max_members_per_lead = 8
member_idle_s = 900
lead_idle_s = 86400
grace_ms = 5000
approval_timeout_s = 600
restart_max = 3
[model]
provider = "openai-compatible"
base_url = "https://model.example/v1"
model = "fake-model"
credential = "model-key"
[lead.scope]
mounts = [{ host = "/tmp", path = "/work", mode = "rw" }]
credentials = ["model-key"]
tools = [{ server = "fake", tools = "*", policy = "ask" }]
delegation = { enabled = true, max_members = 4 }
[[channels]]
kind = "cli"
"#,
    );
    write_config(
        &dir,
        "policy.toml",
        r#"
[spawn]
default = "ask"
[spawn.roles]
reviewer = "allow"
[tools.defaults]
"fake/test" = "ask"
"#,
    );
    write_config(
        &dir,
        "ceiling.toml",
        r#"
paths = ["/etc/shadow"]
credentials = ["root-key"]
tools = ["fake/admin"]
"#,
    );
    write_config(
        &dir,
        "tools.toml",
        r#"
[[servers]]
id = "fake"
transport = "streamable_http"
url = "https://tools.example/mcp"
credential = "model-key"
"#,
    );

    let config = load_config_dir(&dir).unwrap();
    assert_eq!(config.microswarm.home.lang, Language::Zh);
    assert_eq!(
        config.policy.spawn.roles["reviewer"],
        ms_supervisor::config::SpawnPolicy::Allow
    );
    assert_eq!(config.policy.tools.defaults["fake/test"], Permission::Ask);
    assert_eq!(config.ceiling.credentials, ["root-key".to_owned()].into());
    assert_eq!(config.tools.servers[0].id, "fake");
    assert_eq!(
        config.tools.servers[0].credential.as_deref(),
        Some("model-key")
    );

    write_config(
        &dir,
        "tools.toml",
        "[[servers]]\nid='fake'\ntransport='streamable_http'\nurl='https://tools.example/mcp'\ncredential='other-key'\n",
    );
    assert!(load_config_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn config_validation_rejects_unknown_language_and_transport_mismatch() {
    let dir = temp_dir();
    write_config(&dir, "microswarm.toml", "[home]\nlang='fr'\n");
    write_config(&dir, "policy.toml", "[spawn]\ndefault='ask'\n");
    write_config(&dir, "ceiling.toml", "paths=[]\ncredentials=[]\ntools=[]\n");
    write_config(
        &dir,
        "tools.toml",
        "[[servers]]\nid='bad'\ntransport='stdio'\nurl='https://example.test'\n",
    );
    assert!(load_config_dir(&dir).is_err());

    write_config(
        &dir,
        "microswarm.toml",
        "[runtime]\nsocket='unix:///tmp/podman.sock'\nimage='agent:dev'\n[lead.scope]\n",
    );
    write_config(
        &dir,
        "tools.toml",
        "[[servers]]\nid='bad'\ntransport='stdio'\nurl='https://example.test'\n",
    );
    assert!(load_config_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}
