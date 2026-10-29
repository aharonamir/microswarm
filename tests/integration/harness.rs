#![allow(dead_code)]

#[path = "../fixtures/fake_model.rs"]
mod fake_model;

use fake_model::FakeModelServer;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    pub fn start_or_skip() -> Option<Self> {
        Self::start_or_skip_with_approval_tool(false)
    }

    pub fn start_approval_test_or_skip() -> Option<Self> {
        Self::start_or_skip_with_approval_tool(true)
    }

    fn start_or_skip_with_approval_tool(approval_tool: bool) -> Option<Self> {
        if let Err(error) = runtime_prerequisite_error() {
            runtime_unavailable(
                &error,
                std::env::var("MS_REQUIRE_RUNTIME").as_deref() == Ok("1"),
            );
            return None;
        }
        Some(
            Self::start_with_approval_tool(approval_tool)
                .expect("integration harness should start when a runtime is reachable"),
        )
    }

    pub fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_approval_tool(false)
    }

    pub fn start_approval_test() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_approval_tool(true)
    }

    fn start_with_approval_tool(approval_tool: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let home = unique_home()?;
        fs::create_dir_all(home.join("config"))?;
        fs::create_dir_all(home.join("sessions"))?;
        fs::create_dir_all(home.join("work"))?;
        create_synthetic_model_credential(&home)?;
        let model = FakeModelServer::start_with_approval_tool(approval_tool)?;
        let runtime = detect_runtime()?;
        let agent_image = std::env::var("MS_TEST_AGENT_IMAGE")
            .unwrap_or_else(|_| "microswarm-agent:dev".to_owned());
        verify_agent_image(runtime, &agent_image)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::NotFound, error))?;
        write_test_config(&home, &model, runtime, &agent_image, approval_tool)?;
        let mut supervisor = Command::new(env!("CARGO_BIN_EXE_microswarmd"))
            .env("MS_HOME", &home)
            .env("MS_MODEL_BASE_URL", model.base_url())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let socket = home.join("supervisor.sock");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            if let Some(status) = supervisor.try_wait()? {
                let mut stderr = String::new();
                if let Some(mut pipe) = supervisor.stderr.take() {
                    pipe.read_to_string(&mut stderr)?;
                }
                return Err(
                    format!("microswarmd exited during startup ({status}): {stderr}").into(),
                );
            }
            if Instant::now() >= deadline {
                return Err(
                    "microswarmd did not create supervisor.sock within five seconds".into(),
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(Self {
            home,
            model,
            runtime,
            supervisor: Some(supervisor),
        })
    }

    pub fn start_chat(
        &self,
        session_id: Option<&str>,
    ) -> Result<InteractiveChat, Box<dyn std::error::Error>> {
        let mut child = Command::new(cli_binary()?)
            .env("MS_HOME", &self.home)
            .arg("chat")
            .args(session_id.into_iter().flat_map(|id| ["--session", id]))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });
        Ok(InteractiveChat {
            child,
            stdin: Some(stdin),
            lines,
            output: String::new(),
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

    pub fn run_cli(&self, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
        Ok(Command::new(cli_binary()?)
            .env("MS_HOME", &self.home)
            .args(args)
            .output()?)
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

fn runtime_unavailable(reason: &str, required: bool) {
    let message = format!("skipping runtime integration test: {reason}");
    eprintln!("{message}");
    assert!(!required, "MS_REQUIRE_RUNTIME=1: {message}");
}

#[cfg(test)]
mod runtime_requirement_tests {
    use super::{detect_runtime, unique_home, verify_agent_image};

    #[test]
    fn missing_agent_image_is_a_runtime_prerequisite_failure() {
        let runtime = match detect_runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                super::runtime_unavailable(
                    &error.to_string(),
                    std::env::var("MS_REQUIRE_RUNTIME").as_deref() == Ok("1"),
                );
                return;
            }
        };
        let image = unique_home()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let error = verify_agent_image(runtime, &image).unwrap_err();
        assert!(error.contains("not present"));
    }

    #[test]
    fn required_runtime_turns_skip_into_failure() {
        let result = std::panic::catch_unwind(|| super::runtime_unavailable("no socket", true));
        assert!(result.is_err());
    }

    #[test]
    fn optional_runtime_prints_a_skip_instead_of_failing() {
        super::runtime_unavailable("no socket", false);
    }
}

impl Drop for IntegrationHarness {
    fn drop(&mut self) {
        let runtime_name = match self.runtime {
            Runtime::Docker => "docker",
            Runtime::Podman => "podman",
        };
        if let Ok(sessions) = self.session_dirs() {
            for session in sessions {
                let Some(session_id) = session.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if let Ok(containers) = self.containers_for(session_id) {
                    for container in containers {
                        let _ = Command::new(runtime_name)
                            .args(["rm", "--force", &container])
                            .output();
                    }
                }
                let _ = Command::new(runtime_name)
                    .args(["network", "rm", &format!("ms-net-{session_id}")])
                    .output();
            }
        }
        if let Some(mut child) = self.supervisor.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if std::env::var("MS_KEEP_TEST_HOME").as_deref() == Ok("1") {
            eprintln!("preserving integration test home: {}", self.home.display());
        } else {
            let _ = fs::remove_dir_all(&self.home);
        }
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

fn create_synthetic_model_credential(home: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let directory = home.join("credentials");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let value = format!("synthetic-local-test-{}-{nonce}", std::process::id());
    let path = directory.join("fixture-model-key");
    fs::write(&path, value)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn detect_runtime() -> Result<Runtime, std::io::Error> {
    let candidates = match std::env::var("MS_TEST_RUNTIME").as_deref() {
        Ok("docker") => vec![("docker", "{{.ServerVersion}}", Runtime::Docker)],
        Ok("podman") => vec![("podman", "{{.Host.Security.Rootless}}", Runtime::Podman)],
        Ok(value) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("MS_TEST_RUNTIME must be docker or podman, got {value:?}"),
            ));
        }
        Err(_) => vec![
            ("docker", "{{.ServerVersion}}", Runtime::Docker),
            ("podman", "{{.Host.Security.Rootless}}", Runtime::Podman),
        ],
    };
    for (name, info_format, runtime) in candidates {
        if Command::new(name)
            .args(["info", "--format", info_format])
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

fn runtime_prerequisite_error() -> Result<(), String> {
    let runtime = detect_runtime().map_err(|error| error.to_string())?;
    let image =
        std::env::var("MS_TEST_AGENT_IMAGE").unwrap_or_else(|_| "microswarm-agent:dev".to_owned());
    verify_agent_image(runtime, &image)
}

fn verify_agent_image(runtime: Runtime, image: &str) -> Result<(), String> {
    let (program, args) = match runtime {
        Runtime::Docker => ("docker", vec!["image", "inspect", image]),
        Runtime::Podman => ("podman", vec!["image", "exists", image]),
    };
    let result = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("cannot check {runtime:?} image {image:?}: {error}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{runtime:?} agent image {image:?} is not present; build it before running runtime integration tests"
        ))
    }
}

fn write_test_config(
    home: &Path,
    model: &FakeModelServer,
    runtime: Runtime,
    image: &str,
    approval_tool: bool,
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
    let scope_tools = if approval_tool {
        "[{ server = \"first-party\", tools = [\"fs.write\"], policy = \"ask\" }]"
    } else {
        "[]"
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
mounts = [{{ host = {:?}, path = "/work", mode = "rw" }}]
credentials = ["fixture-model-key"]
tools = {scope_tools}

[[channels]]
kind = "cli"
"#,
        model.base_url(),
        home.join("work").display().to_string()
    );
    let dir = home.join("config");
    fs::write(dir.join("microswarm.toml"), config)?;
    fs::write(dir.join("policy.toml"), "")?;
    fs::write(dir.join("ceiling.toml"), "")?;
    let tools = if approval_tool {
        "[[servers]]\nid = \"first-party\"\ntransport = \"stdio\"\ncommand = \"/microswarm-tools\"\n"
    } else {
        ""
    };
    fs::write(dir.join("tools.toml"), tools)?;
    Ok(())
}

pub struct InteractiveChat {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    output: String,
}

impl InteractiveChat {
    pub fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        let stdin = self.stdin.as_mut().expect("chat input is still open");
        writeln!(stdin, "{line}")?;
        stdin.flush()
    }

    pub fn close_input(&mut self) {
        self.stdin.take();
    }

    pub fn wait_for(&mut self, needle: &str, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        while !self.output.contains(needle) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!(
                    "timed out waiting for {needle:?}; output was: {}",
                    self.output
                ));
            }
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    self.output.push_str(&line);
                    self.output.push('\n');
                }
                Err(error) => {
                    return Err(format!(
                        "chat output ended before {needle:?}: {error}; output was: {}",
                        self.output
                    ));
                }
            }
        }
        Ok(self.output.clone())
    }

    pub fn wait(mut self) -> std::io::Result<std::process::ExitStatus> {
        self.close_input();
        self.child.wait()
    }
}

impl Drop for InteractiveChat {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
    let Some(harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
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
