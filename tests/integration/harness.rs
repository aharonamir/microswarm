#[path = "../fixtures/fake_model.rs"]
mod fake_model;

use fake_model::FakeModelServer;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub enum Runtime {
    Docker,
    Podman,
}

pub struct IntegrationHarness {
    pub home: PathBuf,
    pub model: FakeModelServer,
    pub runtime: Runtime,
    supervisor: Option<Child>,
}

impl IntegrationHarness {
    pub fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let home = unique_home()?;
        fs::create_dir_all(home.join("config"))?;
        fs::create_dir_all(home.join("sessions"))?;
        let model = FakeModelServer::start()?;
        let runtime = detect_runtime()?;
        let supervisor = Command::new(env!("CARGO_BIN_EXE_microswarmd"))
            .env("MS_HOME", &home)
            .env("MS_MODEL_BASE_URL", model.base_url())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            home,
            model,
            runtime,
            supervisor: Some(supervisor),
        })
    }
}

impl Drop for IntegrationHarness {
    fn drop(&mut self) {
        if let Some(mut child) = self.supervisor.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.home);
    }
}

fn unique_home() -> std::io::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "microswarm-test-{}-{timestamp}",
        std::process::id()
    )))
}

fn detect_runtime() -> Result<Runtime, std::io::Error> {
    for (name, runtime) in [("docker", Runtime::Docker), ("podman", Runtime::Podman)] {
        if Command::new(name)
            .args(["info", "--format", "{{.ServerVersion}}"])
            .output()
            .is_ok_and(|result| result.status.success())
        {
            return Ok(runtime);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "neither Docker nor Podman API is reachable",
    ))
}

#[test]
fn harness_starts_fake_model_and_supervisor_with_a_real_runtime() {
    let harness = IntegrationHarness::start().expect("runtime and supervisor should start");
    assert!(harness.home.join("config").is_dir());
    assert!(matches!(harness.runtime, Runtime::Docker | Runtime::Podman));
    assert!(harness.supervisor.as_ref().is_some());

    let mut stream = TcpStream::connect(
        harness
            .model
            .base_url()
            .trim_start_matches("http://")
            .trim_end_matches("/v1"),
    )
    .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(b"POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}") .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("200 OK"));
    assert!(response.contains("fixture reply"));
}
