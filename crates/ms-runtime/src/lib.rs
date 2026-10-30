use bollard::{Docker, models};
use futures_util::{StreamExt, stream::Stream};
use ms_proto::{MountMode, Scope};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod lifecycle;
pub mod recovery;
pub mod router;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

#[derive(Clone, Debug)]
pub struct ContainerProfile {
    pub image: String,
    pub session_id: String,
    pub read_only_rootfs: bool,
    pub no_new_privileges: bool,
    pub cap_drop: Vec<String>,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    pub network: String,
    pub host_gateway: String,
    pub container_user: String,
    pub tmpfs: BTreeMap<String, String>,
    pub mounts: Vec<MountSpec>,
}

impl ContainerProfile {
    pub fn for_session(
        scope: &Scope,
        image: &str,
        session_dir: &Path,
        network: &str,
        host_gateway: &str,
        container_user: &str,
    ) -> Result<Self, String> {
        if image.trim().is_empty() || network.trim().is_empty() {
            return Err("container image and network must not be empty".into());
        }
        let session_dir = std::fs::canonicalize(session_dir)
            .map_err(|error| format!("cannot resolve session directory: {error}"))?;
        let mut mounts = vec![
            MountSpec {
                source: std::fs::canonicalize(session_dir.join("control"))
                    .map_err(|error| format!("cannot resolve control directory: {error}"))?
                    .display()
                    .to_string(),
                target: "/session/control".into(),
                read_only: true,
            },
            MountSpec {
                source: std::fs::canonicalize(session_dir.join("out"))
                    .map_err(|error| format!("cannot resolve out directory: {error}"))?
                    .display()
                    .to_string(),
                target: "/session/out".into(),
                read_only: false,
            },
        ];
        for mount in &scope.mounts {
            let source = std::fs::canonicalize(&mount.host_path).map_err(|error| {
                format!(
                    "cannot resolve scoped mount {}: {error}",
                    mount.host_path.display()
                )
            })?;
            let target = mount.container_path.to_string_lossy().into_owned();
            if !target.starts_with("/work")
                && !target.starts_with("/skills")
                && !target.starts_with("/data")
            {
                return Err(format!("invalid scoped mount target {target:?}"));
            }
            mounts.push(MountSpec {
                source: source.display().to_string(),
                target,
                read_only: mount.mode == MountMode::ReadOnly,
            });
        }
        Ok(Self {
            image: image.to_owned(),
            session_id: scope.session_id.clone(),
            read_only_rootfs: true,
            no_new_privileges: true,
            cap_drop: vec!["ALL".into()],
            memory_bytes: i64::from(scope.limits.memory_mb)
                .checked_mul(1024 * 1024)
                .ok_or_else(|| "memory limit overflows bytes".to_owned())?,
            pids_limit: i64::from(scope.limits.pids),
            network: network.to_owned(),
            host_gateway: host_gateway.to_owned(),
            container_user: container_user.to_owned(),
            tmpfs: BTreeMap::from([
                (
                    "/tmp".into(),
                    format!("rw,noexec,nosuid,size={}m", scope.limits.tmp_mb),
                ),
                ("/run".into(), "rw,noexec,nosuid,size=16m".into()),
            ]),
            mounts,
        })
    }

    pub fn container_config(&self, extra_env: &[String]) -> models::ContainerCreateBody {
        let mounts = self
            .mounts
            .iter()
            .map(|mount| models::Mount {
                target: Some(mount.target.clone()),
                source: Some(mount.source.clone()),
                typ: Some(models::MountType::BIND),
                read_only: Some(mount.read_only),
                ..Default::default()
            })
            .collect();
        let labels = HashMap::from([
            ("ms.managed".into(), "true".into()),
            ("ms.session".into(), self.session_id.clone()),
        ]);
        let host_config = models::HostConfig {
            readonly_rootfs: Some(self.read_only_rootfs),
            tmpfs: Some(self.tmpfs.clone().into_iter().collect()),
            cap_drop: Some(self.cap_drop.clone()),
            security_opt: Some(vec!["no-new-privileges:true".into()]),
            memory: Some(self.memory_bytes),
            pids_limit: Some(self.pids_limit),
            network_mode: Some(self.network.clone()),
            mounts: Some(mounts),
            extra_hosts: Some(vec![
                format!("host.containers.internal:{}", self.host_gateway),
                format!("host.docker.internal:{}", self.host_gateway),
            ]),
            ..Default::default()
        };
        models::ContainerCreateBody {
            image: Some(self.image.clone()),
            entrypoint: Some(vec!["/microswarm-agent".into()]),
            working_dir: Some("/session".into()),
            user: Some(self.container_user.clone()),
            env: Some(extra_env.to_vec()),
            labels: Some(labels),
            host_config: Some(host_config),
            ..Default::default()
        }
    }
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
