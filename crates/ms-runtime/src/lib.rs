use bollard::{Docker, models};
use futures_util::{StreamExt, stream::Stream};
pub use profile::{ContainerProfile, MountSpec};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod credentials;
pub mod doctor;
pub mod events;
pub mod lifecycle;
pub mod profile;
pub mod recovery;
pub mod retention;
pub mod router;
pub mod trace;

pub fn config_dir(home: &Path) -> PathBuf {
    let nested = home.join("config");
    if nested.join("microswarm.toml").is_file() {
        nested
    } else {
        home.to_path_buf()
    }
}

pub fn claim_session_busy(
    store: &ms_store::SupervisorStore,
    session_id: &str,
    active_at: &str,
) -> Result<(), String> {
    let claimed = store.connection().execute(
        "UPDATE sessions SET status='busy', last_active_at=?2 WHERE id=?1 AND status IN ('ready','busy')",
        [session_id, active_at],
    ).map_err(|error| error.to_string())?;
    if claimed == 0 {
        return Err("session is no longer ready to accept a message".into());
    }
    Ok(())
}

pub async fn wait_agent_ready(session: &mut ms_store::SessionConnections) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let ready: bool = session
            .events()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE type='agent.ready')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if ready {
            return Ok(());
        }
        let inbound: i64 = session
            .inbound()
            .query_row("SELECT COUNT(*) FROM inbound", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        let events: String = session
            .events()
            .query_row(
                "SELECT COALESCE(group_concat(type, ','),'') FROM events",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if events.contains("agent.exit") {
            return Err(format!(
                "agent exited before ready (inbound rows={inbound}, events={events})"
            ));
        }
        if std::time::Instant::now() >= deadline {
            return Err("agent did not become ready".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub trait CredentialProvider: Send + Sync + 'static {
    fn resolve(&self, name: &str) -> Option<String>;
}

pub struct FileCredentials {
    pub home: PathBuf,
}

impl CredentialProvider for FileCredentials {
    fn resolve(&self, name: &str) -> Option<String> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return None;
        }
        fs::read_to_string(self.home.join("credentials").join(name)).ok()
    }
}

pub fn utc_now() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (elapsed.as_secs() / 86_400) as i64;
    let sec = elapsed.as_secs() % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        sec / 3600,
        (sec % 3600) / 60,
        sec % 60
    )
}

pub fn utc_now_precise() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}.{:03}Z",
        utc_now().trim_end_matches('Z'),
        elapsed.subsec_millis()
    )
}

pub fn new_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    if timestamp > 0xFFFF_FFFF_FFFF {
        return Err("current time is outside the ULID timestamp range".into());
    }
    bytes[..6].copy_from_slice(&timestamp.to_be_bytes()[10..]);
    use std::io::Read;
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes[6..]))
        .map_err(|error| error.to_string())?;
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut value = u128::from_be_bytes(bytes);
    let mut encoded = [0u8; 26];
    for character in encoded.iter_mut().rev() {
        *character = ALPHABET[(value & 0x1f) as usize];
        value >>= 5;
    }
    String::from_utf8(encoded.to_vec()).map_err(|error| error.to_string())
}

#[derive(Clone)]
pub struct ContainerRuntime {
    docker: Docker,
    rootless: bool,
}

impl ContainerRuntime {
    pub fn connect(socket: &str) -> Result<Self, String> {
        let docker = Docker::connect_with_host(socket)
            .map_err(|error| format!("cannot connect to container runtime: {error}"))?;
        let rootless = socket
            .strip_prefix("unix://")
            .and_then(|path| std::fs::metadata(path).ok())
            .is_some_and(|metadata| metadata.uid() != 0);
        Ok(Self { docker, rootless })
    }

    pub fn container_user(&self, session_dir: &Path) -> Result<String, String> {
        if self.rootless {
            return Ok("0:0".into());
        }
        let owner = std::fs::metadata(session_dir)
            .map_err(|error| format!("cannot inspect session directory owner: {error}"))?;
        Ok(format!("{}:{}", owner.uid(), owner.gid()))
    }

    pub async fn ping(&self) -> Result<(), String> {
        self.docker
            .ping()
            .await
            .map(|_| ())
            .map_err(|error| format!("container runtime ping failed: {error}"))
    }

    pub async fn image_exists(&self, image: &str) -> bool {
        self.docker.inspect_image(image).await.is_ok()
    }

    pub async fn create_network(&self, name: &str) -> Result<String, String> {
        let network = self
            .docker
            .create_network(models::NetworkCreateRequest {
                name: name.to_owned(),
                internal: Some(true),
                labels: Some(HashMap::from([("ms.managed".into(), "true".into())])),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("cannot create session network: {error}"))?;
        Ok(network.id)
    }

    pub async fn network_gateway(&self, network_id: &str) -> Result<String, String> {
        let network = self
            .docker
            .inspect_network(network_id, None)
            .await
            .map_err(|error| format!("cannot inspect session network: {error}"))?;
        network
            .ipam
            .and_then(|ipam| ipam.config)
            .and_then(|configs| configs.into_iter().find_map(|config| config.gateway))
            .ok_or_else(|| "session network has no IPv4 gateway".to_owned())
    }

    pub async fn create_container(
        &self,
        profile: &ContainerProfile,
        env: &[String],
    ) -> Result<String, String> {
        let name = format!("ms-agent-{}", profile.session_id);
        let options = bollard::query_parameters::CreateContainerOptionsBuilder::default()
            .name(&name)
            .build();
        let response = self
            .docker
            .create_container(Some(options), profile.container_config(env))
            .await
            .map_err(|error| format!("cannot create agent container: {error}"))?;
        Ok(response.id)
    }

    pub async fn start(&self, container_id: &str) -> Result<(), String> {
        self.docker
            .start_container(container_id, None)
            .await
            .map_err(|error| format!("cannot start agent container: {error}"))
    }

    pub async fn remove_stopped_container(&self, container_id: &str) -> Result<(), String> {
        self.docker
            .remove_container(container_id, None)
            .await
            .map_err(|error| format!("cannot remove stopped agent container: {error}"))
    }

    pub async fn stop(&self, container_id: &str, grace: Duration) -> Result<(), String> {
        let seconds = i32::try_from(grace.as_secs()).unwrap_or(i32::MAX).max(1);
        let options = bollard::query_parameters::StopContainerOptionsBuilder::default()
            .t(seconds)
            .build();
        self.docker
            .stop_container(container_id, Some(options))
            .await
            .map_err(|error| format!("cannot stop agent container: {error}"))
    }

    pub async fn is_running(&self, container_id: &str) -> Result<bool, String> {
        let inspected = match self.docker.inspect_container(container_id, None).await {
            Ok(inspected) => inspected,
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => return Ok(false),
            Err(error) => return Err(format!("cannot inspect agent container: {error}")),
        };
        Ok(inspected
            .state
            .and_then(|state| state.running)
            .unwrap_or(false))
    }

    pub async fn oom_killed(&self, container_id: &str) -> Result<bool, String> {
        let inspected = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|error| format!("cannot inspect agent exit state: {error}"))?;
        Ok(inspected
            .state
            .and_then(|state| state.oom_killed)
            .unwrap_or(false))
    }

    pub async fn stopped_exit(&self, container_id: &str) -> Result<Option<i32>, String> {
        let inspected = match self.docker.inspect_container(container_id, None).await {
            Ok(inspected) => inspected,
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => return Ok(Some(-1)),
            Err(error) => return Err(format!("cannot inspect agent container: {error}")),
        };
        let state = inspected.state.unwrap_or_default();
        let started = state
            .started_at
            .as_deref()
            .is_some_and(|value| !value.starts_with("0001-") && !value.is_empty());
        Ok((state.running == Some(false) && started)
            .then_some(state.exit_code.unwrap_or(-1) as i32))
    }

    pub async fn stopped_sessions(
        &self,
        database: &Path,
    ) -> Result<Vec<ContainerExitEvent>, String> {
        let store = ms_store::SupervisorStore::open_readonly(database)
            .map_err(|error| error.to_string())?;
        let sessions = {
            let mut query=store.connection().prepare("SELECT id,container_id FROM sessions WHERE status IN ('starting','ready','busy') AND container_id IS NOT NULL").map_err(|error|error.to_string())?;
            query
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        };
        let mut exits = Vec::new();
        for (session_id, container_id) in sessions {
            if let Some(exit_code) = self.stopped_exit(&container_id).await? {
                exits.push(ContainerExitEvent {
                    session_id,
                    container_id,
                    exit_code: Some(exit_code),
                });
            }
        }
        Ok(exits)
    }

    pub async fn managed_containers(&self) -> Result<Vec<ManagedContainer>, String> {
        let filters = HashMap::from([("label", vec!["ms.managed=true"])]);
        let options = bollard::query_parameters::ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();
        self.docker
            .list_containers(Some(options))
            .await
            .map_err(|error| format!("cannot list managed containers: {error}"))?
            .into_iter()
            .map(|container| {
                let labels = container.labels.unwrap_or_default();
                Ok(ManagedContainer {
                    id: container.id.ok_or("managed container has no id")?,
                    session_id: labels.get("ms.session").cloned(),
                    running: container.state == Some(models::ContainerSummaryStateEnum::RUNNING),
                })
            })
            .collect()
    }

    pub async fn environment(&self, container_id: &str) -> Result<Vec<String>, String> {
        self.docker
            .inspect_container(container_id, None)
            .await
            .map_err(|error| format!("cannot inspect agent environment: {error}"))?
            .config
            .and_then(|config| config.env)
            .ok_or_else(|| "agent container has no environment".into())
    }

    pub async fn kill(&self, container_id: &str) -> Result<(), String> {
        self.docker
            .kill_container(container_id, None)
            .await
            .map_err(|error| format!("cannot kill agent container: {error}"))
    }

    pub async fn signal(&self, container_id: &str, signal: &str) -> Result<(), String> {
        let options = bollard::query_parameters::KillContainerOptionsBuilder::default()
            .signal(signal)
            .build();
        self.docker
            .kill_container(container_id, Some(options))
            .await
            .map_err(|error| format!("cannot signal agent container: {error}"))
    }

    pub async fn container_logs(&self, container_id: &str) -> Result<String, String> {
        let options = bollard::query_parameters::LogsOptionsBuilder::default()
            .stdout(true)
            .stderr(true)
            .tail("100")
            .build();
        let mut stream = self.docker.logs(container_id, Some(options));
        let mut output = String::new();
        while let Some(line) = stream.next().await {
            let line = line.map_err(|error| format!("cannot read container logs: {error}"))?;
            output.push_str(&line.to_string());
        }
        Ok(output)
    }

    pub async fn container_state(&self, container_id: &str) -> Result<String, String> {
        let inspected = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|error| format!("cannot inspect agent container: {error}"))?;
        let state = inspected.state.unwrap_or_default();
        Ok(format!(
            "path={:?} args={:?} status={:?} running={:?} exit={:?} error={:?}",
            inspected.path,
            inspected.args,
            state.status,
            state.running,
            state.exit_code,
            state.error
        ))
    }

    pub async fn watch_exits<F, Fut>(&self, mut handle_exit: F) -> Result<(), String>
    where
        F: FnMut(ContainerExitEvent) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let mut events = Box::pin(self.exit_events());
        while let Some(event) = events.next().await {
            handle_exit(event?).await?;
        }
        Ok(())
    }

    fn exit_events(&self) -> impl Stream<Item = Result<ContainerExitEvent, String>> + '_ {
        let filters = HashMap::from([
            ("type", vec!["container"]),
            ("label", vec!["ms.managed=true"]),
        ]);
        self.docker
            .events(Some(
                bollard::query_parameters::EventsOptionsBuilder::default()
                    .filters(&filters)
                    .build(),
            ))
            .filter_map(|result| async move {
                match result {
                    Ok(event) => exit_event_from(&event).map(Ok),
                    Err(error) => Some(Err(format!("runtime event stream failed: {error}"))),
                }
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerExitEvent {
    pub container_id: String,
    pub session_id: String,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedContainer {
    pub id: String,
    pub session_id: Option<String>,
    pub running: bool,
}

pub fn exit_event_from(event: &models::EventMessage) -> Option<ContainerExitEvent> {
    if event.typ != Some(models::EventMessageTypeEnum::CONTAINER)
        || event.action.as_deref() != Some("die")
    {
        return None;
    }
    let actor = event.actor.as_ref()?;
    let attributes = actor.attributes.as_ref()?;
    if attributes.get("ms.managed").map(String::as_str) != Some("true") {
        return None;
    }
    let container_id = actor.id.as_ref()?.clone();
    let session_id = attributes.get("ms.session")?.clone();
    let exit_code = attributes
        .get("exitCode")
        .and_then(|value| value.parse::<i32>().ok());
    Some(ContainerExitEvent {
        container_id,
        session_id,
        exit_code,
    })
}
