#[path = "../fixtures/fake_model.rs"]
mod fake_model;

use fake_model::FakeModelServer;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
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
        let agent_image = std::env::var("MS_TEST_AGENT_IMAGE")
            .unwrap_or_else(|_| "microswarm-agent:dev".to_owned());
        write_test_config(&home, &model, runtime, &agent_image)?;
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

    pub fn chat(
        &self,
        session_id: Option<&str>,
        messages: &[&str],
    ) -> Result<Output, Box<dyn std::error::Error>> {
        let mut child = Command::new(cli_binary()?)
            .env("MS_HOME", &self.home)
            .arg("chat")
            .args(session_id.into_iter().flat_map(|id| ["--session", id]))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        {
            let stdin = child.stdin.as_mut().expect("stdin is piped");
            for message in messages {
                writeln!(stdin, "{message}")?;
            }
        }
        Ok(child.wait_with_output()?)
    }

    pub fn session_dirs(&self) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        Ok(fs::read_dir(self.home.join("sessions"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect())
    }

    pub fn containers_for(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let (program, args): (&str, Vec<String>) = match self.runtime {
            Runtime::Docker => (
                "docker",
                vec![
                    "ps".into(),
                    "--all".into(),
                    "--filter".into(),
                    format!("label=ms.session={session_id}"),
                    "--format".into(),
                    "{{.ID}}".into(),
                ],
            ),
            Runtime::Podman => (
                "podman",
                vec![
                    "ps".into(),
                    "--all".into(),
                    "--filter".into(),
                    format!("label=ms.session={session_id}"),
                    "--format".into(),
                    "{{.ID}}".into(),
                ],
            ),
        };
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            return Err(format!(
                "{program} ps failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?
            .lines()
            .map(str::to_owned)
            .collect())
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

fn write_test_config(
    home: &PathBuf,
    model: &FakeModelServer,
    runtime: Runtime,
    image: &str,
) -> std::io::Result<()> {
    let socket = match runtime {
        Runtime::Docker => std::env::var("DOCKER_HOST")
            .unwrap_or_else(|_| "unix:///var/run/docker.sock".to_owned()),
        Runtime::Podman => match std::env::var("CONTAINER_HOST") {
            Ok(socket) => socket,
            Err(_) => {
                let output = Command::new("podman")
                    .args(["info", "--format", "{{.Host.RemoteSocket.Path}}"])
                    .output()?;
                let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                if path.starts_with("unix://") {
                    path
                } else {
                    format!("unix://{path}")
                }
            }
        },
    };
    let config = format!(
        r#"[runtime]
socket = {socket:?}
image = {image:?}

[model]
provider = "openai-compatible"
base_url = {:?}
model = "fake-model"
credential = "fixture-model-key"

[lead.scope]
credentials = ["fixture-model-key"]
tools = []

[[channels]]
kind = "cli"
"#,
        model.base_url()
    );
    let dir = home.join("config");
    fs::write(dir.join("microswarm.toml"), config)?;
    fs::write(dir.join("policy.toml"), "")?;
    fs::write(dir.join("ceiling.toml"), "")?;
    fs::write(dir.join("tools.toml"), "")?;
    Ok(())
}

fn cli_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let current = std::env::current_exe()?;
    let debug_dir = current
        .parent()
        .and_then(|deps_dir| deps_dir.parent())
        .ok_or("integration test binary is not under target/<profile>/deps")?;
    let executable = if cfg!(windows) {
        debug_dir.join("microswarm.exe")
    } else {
        debug_dir.join("microswarm")
    };
    if !executable.is_file() {
        return Err(format!("CLI executable is missing: {}", executable.display()).into());
    }
    Ok(executable)
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
