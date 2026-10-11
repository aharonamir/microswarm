use ms_proto::{MountMode, Permission, ToolNames};
use serde::{Deserialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    En,
    Zh,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigSet {
    pub microswarm: MicroswarmConfig,
    pub policy: PolicyConfig,
    pub ceiling: CeilingConfig,
    pub tools: ToolsConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MicroswarmConfig {
    #[serde(default)]
    pub home: HomeConfig,
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    pub model: Option<ModelConfig>,
    pub lead: LeadConfig,
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HomeConfig {
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default)]
    pub lang: Language,
}

impl Default for HomeConfig {
    fn default() -> Self {
        Self {
            retention_days: default_retention_days(),
            lang: Language::default(),
        }
    }
}

const fn default_retention_days() -> u32 {
    30
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeConfig {
    pub socket: String,
    pub image: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_max_sessions")]
    pub max_sessions: u32,
    #[serde(default = "default_max_members")]
    pub max_members_per_lead: u8,
    #[serde(default = "default_member_idle")]
    pub member_idle_s: u64,
    #[serde(default = "default_lead_idle")]
    pub lead_idle_s: u64,
    #[serde(default = "default_grace")]
    pub grace_ms: u64,
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_s: u64,
    #[serde(default = "default_restart_max")]
    pub restart_max: u8,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_sessions: default_max_sessions(),
            max_members_per_lead: default_max_members(),
            member_idle_s: default_member_idle(),
            lead_idle_s: default_lead_idle(),
            grace_ms: default_grace(),
            approval_timeout_s: default_approval_timeout(),
            restart_max: default_restart_max(),
        }
    }
}

const fn default_max_sessions() -> u32 {
    200
}
const fn default_max_members() -> u8 {
    8
}
const fn default_member_idle() -> u64 {
    900
}
const fn default_lead_idle() -> u64 {
    86_400
}
const fn default_grace() -> u64 {
    5_000
}
const fn default_approval_timeout() -> u64 {
    600
}
const fn default_restart_max() -> u8 {
    3
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub credential: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LeadConfig {
    pub scope: LeadScopeConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LeadScopeConfig {
    #[serde(default)]
    pub mounts: Vec<ConfiguredMount>,
    #[serde(default)]
    pub credentials: Vec<String>,
    #[serde(default)]
    pub tools: Vec<ConfiguredTool>,
    pub delegation: Option<DelegationConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfiguredMount {
    pub host: PathBuf,
    pub path: PathBuf,
    pub mode: MountMode,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfiguredTool {
    pub server: String,
    pub tools: ToolNames,
    pub policy: Permission,
    #[serde(default)]
    pub overrides: BTreeMap<String, Permission>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DelegationConfig {
    pub enabled: bool,
    pub max_members: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChannelConfig {
    pub kind: String,
    pub token: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpawnPolicy {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PolicyConfig {
    #[serde(default)]
    pub spawn: SpawnConfig,
    #[serde(default)]
    pub tools: ToolPolicyConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpawnConfig {
    #[serde(default = "default_spawn_policy")]
    pub default: SpawnPolicy,
    #[serde(default)]
    pub roles: BTreeMap<String, SpawnPolicy>,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            default: default_spawn_policy(),
            roles: BTreeMap::new(),
        }
    }
}

const fn default_spawn_policy() -> SpawnPolicy {
    SpawnPolicy::Ask
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ToolPolicyConfig {
    #[serde(default)]
    pub defaults: BTreeMap<String, Permission>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CeilingConfig {
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    #[serde(default)]
    pub credentials: BTreeSet<String>,
    #[serde(default)]
    pub tools: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub servers: Vec<ToolServerConfig>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolTransport {
    Stdio,
    StreamableHttp,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ToolServerConfig {
    pub id: String,
    pub transport: ToolTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
}

#[derive(Debug)]
pub struct ConfigError(String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

pub fn load_config_dir(path: impl AsRef<Path>) -> Result<ConfigSet, ConfigError> {
    let path = path.as_ref();
    let config = ConfigSet {
        microswarm: read_toml(path.join("microswarm.toml"))?,
        policy: read_toml(path.join("policy.toml"))?,
        ceiling: read_toml(path.join("ceiling.toml"))?,
        tools: read_toml(path.join("tools.toml"))?,
    };
    config.validate()?;
    Ok(config)
}

fn read_toml<T: DeserializeOwned>(path: PathBuf) -> Result<T, ConfigError> {
    let source = fs::read_to_string(&path)
        .map_err(|error| ConfigError(format!("{}: {error}", path.display())))?;
    toml::from_str(&source).map_err(|error| ConfigError(format!("{}: {error}", path.display())))
}

impl ConfigSet {
    fn validate(&self) -> Result<(), ConfigError> {
        let app = &self.microswarm;
        if app.home.retention_days == 0 || app.limits.max_sessions == 0 {
            return Err(ConfigError(
                "retention_days and max_sessions must be positive".into(),
            ));
        }
        if app.limits.max_members_per_lead > 32 {
            return Err(ConfigError(
                "max_members_per_lead exceeds the schema maximum of 32".into(),
            ));
        }
        if !app.runtime.socket.starts_with("unix://") || app.runtime.image.trim().is_empty() {
            return Err(ConfigError(
                "runtime.socket must use unix:// and runtime.image must not be empty".into(),
            ));
        }
        if let Some(model) = &app.model {
            if !matches!(model.provider.as_str(), "openai-compatible" | "anthropic") {
                return Err(ConfigError(format!(
                    "unsupported model provider {:?}",
                    model.provider
                )));
            }
            validate_url(&model.base_url, "model.base_url")?;
            if !valid_name(&model.credential)
                || !app.lead.scope.credentials.contains(&model.credential)
            {
                return Err(ConfigError(
                    "model credential must be a valid name granted in lead.scope.credentials"
                        .into(),
                ));
            }
        }
        for mount in &app.lead.scope.mounts {
            if !mount.host.is_absolute() && !mount.host.starts_with("~/") {
                return Err(ConfigError(format!(
                    "mount host path must be absolute or start with ~/: {}",
                    mount.host.display()
                )));
            }
            if !(mount.path.starts_with("/work")
                || mount.path.starts_with("/skills")
                || mount.path.starts_with("/data"))
            {
                return Err(ConfigError(format!(
                    "mount container path is outside supported roots: {}",
                    mount.path.display()
                )));
            }
        }
        for credential in &app.lead.scope.credentials {
            if !valid_name(credential) {
                return Err(ConfigError(format!(
                    "invalid credential name {credential:?}"
                )));
            }
        }
        for tool in &app.lead.scope.tools {
            if let ToolNames::All(selector) = &tool.tools
                && selector != "*"
            {
                return Err(ConfigError(format!("invalid tool wildcard {selector:?}")));
            }
        }
        for channel in &app.channels {
            if !matches!(channel.kind.as_str(), "cli" | "telegram" | "webhook") {
                return Err(ConfigError(format!(
                    "unsupported channel kind {:?}",
                    channel.kind
                )));
            }
            if channel
                .token
                .as_ref()
                .is_some_and(|token| !valid_name(token))
            {
                return Err(ConfigError(
                    "channel token must be a credential name, never a secret value".into(),
                ));
            }
        }
        for path in &self.ceiling.paths {
            if !path.is_absolute()
                && !path.to_string_lossy().starts_with("$MS_HOME")
                && !path.to_string_lossy().starts_with("~/")
            {
                return Err(ConfigError(format!(
                    "ceiling path must be absolute or use a supported root: {}",
                    path.display()
                )));
            }
        }
        let mut ids = BTreeSet::new();
        for server in &self.tools.servers {
            if !valid_server_id(&server.id) || !ids.insert(server.id.as_str()) {
                return Err(ConfigError(format!(
                    "invalid or duplicate tools server id {:?}",
                    server.id
                )));
            }
            match server.transport {
                ToolTransport::Stdio
                    if server
                        .command
                        .as_deref()
                        .is_some_and(|command| !command.is_empty())
                        && server.url.is_none() => {}
                ToolTransport::StreamableHttp
                    if server.command.is_none()
                        && server
                            .url
                            .as_deref()
                            .is_some_and(|url| validate_url(url, "tools server URL").is_ok()) => {}
                _ => {
                    return Err(ConfigError(format!(
                        "tools server {:?} has fields that do not match its transport",
                        server.id
                    )));
                }
            }
        }
        Ok(())
    }
}

fn valid_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(byte))
}

fn valid_server_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(byte))
}

fn validate_url(value: &str, field: &str) -> Result<(), ConfigError> {
    let local_http = value.starts_with("http://localhost") || value.starts_with("http://127.0.0.1");
    if value.starts_with("https://") || local_http {
        Ok(())
    } else {
        Err(ConfigError(format!(
            "{field} must use HTTPS (HTTP is allowed for loopback only)"
        )))
    }
}
