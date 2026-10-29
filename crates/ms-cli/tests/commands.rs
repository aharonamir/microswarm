use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn home() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ms-cli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    path
}

#[test]
fn init_help_and_empty_sessions_are_available() {
    let dir = home();
    let init = Command::new(env!("CARGO_BIN_EXE_microswarm"))
        .args(["init", "--home"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let config = fs::read_to_string(dir.join("microswarm.toml")).unwrap();
    assert!(config.contains("socket = \"unix:///var/run/docker.sock\""));
    assert!(dir.join("microswarm.toml").is_file());
    assert!(dir.join("policy.toml").is_file());
    assert!(dir.join("ceiling.toml").is_file());
    assert!(dir.join("tools.toml").is_file());
    let sessions = Command::new(env!("CARGO_BIN_EXE_microswarm"))
        .args(["sessions", "--home"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(sessions.status.success());
    assert!(String::from_utf8_lossy(&sessions.stdout).contains("No active sessions"));
    let help = Command::new(env!("CARGO_BIN_EXE_microswarm"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&help.stdout).contains("creds"));
    fs::remove_dir_all(dir).unwrap();
}
