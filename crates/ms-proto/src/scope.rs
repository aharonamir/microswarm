use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum MountMode {
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

impl MountMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "ro",
            Self::ReadWrite => "rw",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mount {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub mode: MountMode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolNames {
    All(String),
    Selected(BTreeSet<String>),
}

impl ToolNames {
    pub fn all() -> Self {
        Self::All("*".to_owned())
    }

    pub fn contains(&self, tool: &str) -> bool {
        match self {
            Self::All(value) => value == "*",
            Self::Selected(names) => names.contains(tool),
        }
    }

    fn intersect(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::All(a), Self::All(b)) if a == "*" && b == "*" => Self::all(),
            (Self::All(a), Self::Selected(names)) if a == "*" => Self::Selected(names.clone()),
            (Self::Selected(names), Self::All(b)) if b == "*" => Self::Selected(names.clone()),
            (Self::Selected(left), Self::Selected(right)) => {
                Self::Selected(left.intersection(right).cloned().collect())
            }
            _ => Self::Selected(BTreeSet::new()),
        }
    }

    fn names(&self) -> Option<&BTreeSet<String>> {
        match self {
            Self::All(value) if value == "*" => None,
            Self::All(_) => Some(&EMPTY_NAMES),
            Self::Selected(names) => Some(names),
        }
    }
}

static EMPTY_NAMES: BTreeSet<String> = BTreeSet::new();

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolGrant {
    pub server: String,
    pub tools: ToolNames,
    pub policy: Permission,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Permission>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedFrom {
    Config,
    TeamSkill,
    Delegation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrowedReason {
    NotInLeadScope,
    Ceiling,
    ModeDowngraded,
    PolicyTightened,
    LimitCapped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrowedDimension {
    Mount,
    Credential,
    Tool,
    Mode,
    Policy,
    Limit,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NarrowedItem {
    pub dimension: NarrowedDimension,
    pub item: String,
    pub reason: NarrowedReason,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub memory_mb: u32,
    pub pids: u32,
    pub tmp_mb: u32,
    pub max_turns: u32,
    pub max_tokens: u64,
}

impl Limits {
    fn min(&self, other: &Self) -> Self {
        Self {
            memory_mb: self.memory_mb.min(other.memory_mb),
            pids: self.pids.min(other.pids),
            tmp_mb: self.tmp_mb.min(other.tmp_mb),
            max_turns: self.max_turns.min(other.max_turns),
            max_tokens: self.max_tokens.min(other.max_tokens),
        }
    }

    fn is_no_greater_than(&self, other: &Self) -> bool {
        self.memory_mb <= other.memory_mb
            && self.pids <= other.pids
            && self.tmp_mb <= other.tmp_mb
            && self.max_turns <= other.max_turns
            && self.max_tokens <= other.max_tokens
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub version: u8,
    pub session_id: String,
    pub derived_from: DerivedFrom,
    pub source_ref: String,
    pub mounts: Vec<Mount>,
    pub credentials: Vec<String>,
    pub tools: Vec<ToolGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<DelegationScope>,
    pub limits: Limits,
    pub narrowed: Vec<NarrowedItem>,
    pub hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DelegationScope {
    pub enabled: bool,
    pub max_members: u8,
}

impl Scope {
    pub fn canonical_hash(&self) -> String {
        let mut value = serde_json::to_value(self).expect("Scope serialization is infallible");
        if let Some(object) = value.as_object_mut() {
            object.remove("hash");
        }
        let canonical = serde_json::to_vec(&value).expect("JSON value serialization is infallible");
        let digest = Sha256::digest(canonical);
        format!("sha256:{digest:x}")
    }

    pub fn update_hash(&mut self) {
        self.hash = self.canonical_hash();
    }

    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.credentials
            .iter()
            .all(|credential| other.credentials.contains(credential))
            && self.mounts.iter().all(|mount| {
                other.mounts.iter().any(|parent| {
                    mount.host_path.starts_with(&parent.host_path)
                        && mount.mode <= parent.mode
                        && mount.container_path.starts_with(&parent.container_path)
                })
            })
            && self.tools.iter().all(|grant| {
                other.tools.iter().any(|parent| {
                    if grant.server != parent.server {
                        return false;
                    }
                    let names_fit = match (grant.tools.names(), parent.tools.names()) {
                        (Some(child), Some(parent_names)) => child.is_subset(parent_names),
                        (Some(_), None) | (None, None) => true,
                        (None, Some(_)) => false,
                    };
                    if !names_fit || grant.policy < parent.policy {
                        return false;
                    }
                    let relevant_tools: BTreeSet<&str> = grant.tools.names().map_or_else(
                        || {
                            grant
                                .overrides
                                .keys()
                                .chain(parent.overrides.keys())
                                .map(String::as_str)
                                .collect()
                        },
                        |names| names.iter().map(String::as_str).collect(),
                    );
                    relevant_tools.into_iter().all(|tool| {
                        let child_policy =
                            grant.overrides.get(tool).copied().unwrap_or(grant.policy);
                        let parent_policy =
                            parent.overrides.get(tool).copied().unwrap_or(parent.policy);
                        child_policy >= parent_policy
                    })
                })
            })
            && self.limits.is_no_greater_than(&other.limits)
    }

    pub fn is_outside_ceiling(&self, ceiling: &ScopeCeiling) -> bool {
        self.mounts.iter().all(|mount| {
            !ceiling
                .paths
                .iter()
                .any(|blocked| path_overlaps(&mount.host_path, blocked))
        }) && self
            .credentials
            .iter()
            .all(|credential| !ceiling.credentials.contains(credential))
            && self.tools.iter().all(|grant| {
                if grant.tools.names().is_none() {
                    !ceiling
                        .tools
                        .iter()
                        .any(|item| item.starts_with(&format!("{}/", grant.server)))
                } else {
                    grant.tools.names().is_some_and(|names| {
                        names.iter().all(|tool| {
                            !ceiling
                                .tools
                                .contains(&format!("{}/{}", grant.server, tool))
                                && !ceiling.tools.contains(&format!("{}/*", grant.server))
                        })
                    })
                }
            })
            && ceiling
                .limits
                .as_ref()
                .is_none_or(|limits| self.limits.is_no_greater_than(limits))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScopeCeiling {
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    #[serde(default)]
    pub credentials: BTreeSet<String>,
    #[serde(default)]
    pub tools: BTreeSet<String>,
    #[serde(default)]
    pub limits: Option<Limits>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScopeError {
    InvalidPath(PathBuf),
    InvalidWildcard(String),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(
                formatter,
                "cannot canonicalize scope path {}",
                path.display()
            ),
            Self::InvalidWildcard(value) => {
                write!(formatter, "invalid wildcard tool selector {value:?}")
            }
        }
    }
}

impl std::error::Error for ScopeError {}

pub fn intersect_scope(
    request: &Scope,
    lead: &Scope,
    ceiling: &ScopeCeiling,
) -> Result<Scope, ScopeError> {
    let blocked_paths = ceiling
        .paths
        .iter()
        .map(|path| canonicalize(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = request.clone();
    result.mounts.clear();
    result.credentials.clear();
    result.tools.clear();
    result.narrowed.clear();
    for credential in &request.credentials {
        let reason = if !lead.credentials.contains(credential) {
            Some(NarrowedReason::NotInLeadScope)
        } else if ceiling.credentials.contains(credential) {
            Some(NarrowedReason::Ceiling)
        } else {
            None
        };
        if let Some(reason) = reason {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Credential,
                item: credential.clone(),
                reason,
            });
        } else {
            result.credentials.push(credential.clone());
        }
    }

    for requested in &request.mounts {
        let host = canonicalize(&requested.host_path)?;
        if blocked_paths
            .iter()
            .any(|blocked| path_overlaps(&host, blocked))
        {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Mount,
                item: host.display().to_string(),
                reason: NarrowedReason::Ceiling,
            });
            continue;
        }
        let mut admitted = None;
        for parent in &lead.mounts {
            let parent_host = canonicalize(&parent.host_path)?;
            if !host.starts_with(&parent_host) {
                continue;
            }
            let relative = host
                .strip_prefix(&parent_host)
                .map_err(|_| ScopeError::InvalidPath(host.clone()))?;
            let projected = normalize_join(&parent.container_path, relative);
            let requested_container = normalize_path(&requested.container_path);
            if requested_container != projected {
                continue;
            }
            let mode = requested.mode.min(parent.mode);
            if mode != requested.mode {
                result.narrowed.push(NarrowedItem {
                    dimension: NarrowedDimension::Mode,
                    item: host.display().to_string(),
                    reason: NarrowedReason::ModeDowngraded,
                });
            }
            admitted = Some(Mount {
                host_path: host.clone(),
                container_path: projected,
                mode,
            });
            break;
        }
        if let Some(mount) = admitted {
            result.mounts.push(mount);
        } else {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Mount,
                item: host.display().to_string(),
                reason: NarrowedReason::NotInLeadScope,
            });
        }
    }

    for request_grant in &request.tools {
        let Some(lead_grant) = lead
            .tools
            .iter()
            .find(|grant| grant.server == request_grant.server)
        else {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Tool,
                item: request_grant.server.clone(),
                reason: NarrowedReason::NotInLeadScope,
            });
            continue;
        };
        let mut names = request_grant.tools.intersect(&lead_grant.tools);
        let wildcard_blocked = ceiling
            .tools
            .contains(&format!("{}/*", request_grant.server));
        let server_blocked = wildcard_blocked
            || names.names().is_none()
                && ceiling
                    .tools
                    .iter()
                    .any(|item| item.starts_with(&format!("{}/", request_grant.server)));
        match (&mut names, server_blocked) {
            (_, true) => names = ToolNames::Selected(BTreeSet::new()),
            (ToolNames::All(value), false) if value != "*" => {
                return Err(ScopeError::InvalidWildcard(value.clone()));
            }
            (ToolNames::Selected(selected), false) => {
                selected.retain(|tool| {
                    !ceiling
                        .tools
                        .contains(&format!("{}/{}", request_grant.server, tool))
                });
            }
            _ => {}
        }
        if server_blocked {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Tool,
                item: format!("{}/*", request_grant.server),
                reason: NarrowedReason::Ceiling,
            });
        } else if request_grant.tools.names().is_none() && lead_grant.tools.names().is_some() {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Tool,
                item: format!("{}/*", request_grant.server),
                reason: NarrowedReason::NotInLeadScope,
            });
        } else if let (Some(requested), Some(granted)) =
            (request_grant.tools.names(), lead_grant.tools.names())
        {
            for tool in requested.difference(granted) {
                result.narrowed.push(NarrowedItem {
                    dimension: NarrowedDimension::Tool,
                    item: format!("{}/{}", request_grant.server, tool),
                    reason: NarrowedReason::NotInLeadScope,
                });
            }
            for tool in requested.intersection(granted) {
                if ceiling
                    .tools
                    .contains(&format!("{}/{}", request_grant.server, tool))
                {
                    result.narrowed.push(NarrowedItem {
                        dimension: NarrowedDimension::Tool,
                        item: format!("{}/{}", request_grant.server, tool),
                        reason: NarrowedReason::Ceiling,
                    });
                }
            }
        }
        if names.names().is_some_and(BTreeSet::is_empty) {
            continue;
        }

        let base = request_grant.policy.max(lead_grant.policy);
        if base > request_grant.policy {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Policy,
                item: request_grant.server.clone(),
                reason: NarrowedReason::PolicyTightened,
            });
        }
        let mut overrides = BTreeMap::new();
        let override_names: BTreeSet<String> = match names.names() {
            Some(selected) => selected.clone(),
            None => request_grant
                .overrides
                .keys()
                .chain(lead_grant.overrides.keys())
                .cloned()
                .collect(),
        };
        for tool in &override_names {
            let requested_policy = request_grant
                .overrides
                .get(tool)
                .copied()
                .unwrap_or(request_grant.policy);
            let lead_policy = lead_grant
                .overrides
                .get(tool)
                .copied()
                .unwrap_or(lead_grant.policy);
            let effective = requested_policy.max(lead_policy).max(base);
            if effective != base {
                overrides.insert(tool.clone(), effective);
            }
            if effective > requested_policy {
                result.narrowed.push(NarrowedItem {
                    dimension: NarrowedDimension::Policy,
                    item: format!("{}/{}", request_grant.server, tool),
                    reason: NarrowedReason::PolicyTightened,
                });
            }
        }
        if names.names().is_none() && request_grant.policy < base {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Policy,
                item: format!("{}/*", request_grant.server),
                reason: NarrowedReason::PolicyTightened,
            });
        }
        result.tools.push(ToolGrant {
            server: request_grant.server.clone(),
            tools: names,
            policy: base,
            overrides,
        });
    }

    if result.derived_from != DerivedFrom::Config {
        result.delegation = None;
    }

    result.limits = result.limits.min(&lead.limits);
    if let Some(maximum) = &ceiling.limits {
        result.limits = result.limits.min(maximum);
    }
    for (name, was_capped) in [
        (
            "memory_mb",
            result.limits.memory_mb < request.limits.memory_mb,
        ),
        ("pids", result.limits.pids < request.limits.pids),
        ("tmp_mb", result.limits.tmp_mb < request.limits.tmp_mb),
        (
            "max_turns",
            result.limits.max_turns < request.limits.max_turns,
        ),
        (
            "max_tokens",
            result.limits.max_tokens < request.limits.max_tokens,
        ),
    ] {
        if was_capped {
            result.narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Limit,
                item: name.to_owned(),
                reason: NarrowedReason::LimitCapped,
            });
        }
    }
    result.update_hash();
    Ok(result)
}

fn canonicalize(path: &Path) -> Result<PathBuf, ScopeError> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Ok(canonical);
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ScopeError::InvalidPath(path.to_owned()))?
            .join(path)
    };
    let normalized = normalize_path(&absolute);
    let mut ancestor = normalized.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(ancestor) {
            Ok(mut resolved) => {
                for component in missing.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(normalize_path(&resolved));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(ancestor).is_ok() {
                    return Err(ScopeError::InvalidPath(path.to_owned()));
                }
            }
            Err(_) => return Err(ScopeError::InvalidPath(path.to_owned())),
        }
        let name = ancestor
            .file_name()
            .ok_or_else(|| ScopeError::InvalidPath(path.to_owned()))?;
        missing.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| ScopeError::InvalidPath(path.to_owned()))?;
    }
}

fn path_overlaps(left: &Path, right: &Path) -> bool {
    let left = canonicalize(left).unwrap_or_else(|_| normalize_path(left));
    let right = canonicalize(right).unwrap_or_else(|_| normalize_path(right));
    left.starts_with(&right) || right.starts_with(&left)
}

fn normalize_join(base: &Path, relative: &Path) -> PathBuf {
    normalize_path(&base.join(relative))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
