use bollard::{Docker, models};
use futures_util::{StreamExt, stream::Stream};
use ms_proto::{MountMode, Scope};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    time::Duration,
};

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
    pub tmpfs: BTreeMap<String, String>,
    pub mounts: Vec<MountSpec>,
}

impl ContainerProfile {
    pub fn for_session(
        scope: &Scope,
        image: &str,
        session_dir: &Path,
        network: &str,
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
            ..Default::default()
        };
        models::ContainerCreateBody {
            image: Some(self.image.clone()),
            entrypoint: Some(vec!["/microswarm-agent".into()]),
            working_dir: Some("/session".into()),
            user: Some("65532:65532".into()),
            env: Some(extra_env.to_vec()),
            labels: Some(labels),
            host_config: Some(host_config),
            ..Default::default()
        }
    }
}

pub struct ContainerRuntime {
    docker: Docker,
}

impl ContainerRuntime {
    pub fn connect(socket: &str) -> Result<Self, String> {
        let docker = Docker::connect_with_host(socket)
            .map_err(|error| format!("cannot connect to container runtime: {error}"))?;
        Ok(Self { docker })
    }

    pub async fn ping(&self) -> Result<(), String> {
        self.docker
            .ping()
            .await
            .map(|_| ())
            .map_err(|error| format!("container runtime ping failed: {error}"))
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
