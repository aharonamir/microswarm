use crate::config::ConfigSet;
use ms_proto::{
    DerivedFrom, Limits, Mount, MountMode, NarrowedDimension, NarrowedItem, NarrowedReason,
    Permission, Scope, ScopeCeiling, ToolGrant, ToolNames, intersect_scope,
};
use ms_store::{SessionRecord, SupervisorStore};
use serde_json::Value;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn build_lead_scope(config: &ConfigSet, session_id: &str) -> Result<Scope, String> {
    if session_id.trim().is_empty() {
        return Err("session id must not be empty".into());
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is missing while expanding configured mounts".to_owned())?;
    let mounts = config
        .microswarm
        .lead
        .scope
        .mounts
        .iter()
        .map(|mount| {
            let host_path = if mount.host.starts_with("~/") {
                home.join(mount.host.strip_prefix("~/").expect("prefix checked"))
            } else {
                mount.host.clone()
            };
            let host_path = fs::canonicalize(&host_path).map_err(|error| {
                format!(
                    "cannot resolve configured mount {}: {error}",
                    host_path.display()
                )
            })?;
            Ok(Mount {
                host_path,
                container_path: mount.path.clone(),
                mode: mount.mode,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let lead = &config.microswarm.lead.scope;
    let mut candidate = Scope {
        version: 1,
        session_id: session_id.to_owned(),
        derived_from: DerivedFrom::Config,
        source_ref: "config".into(),
        mounts,
        credentials: lead.credentials.clone(),
        tools: lead
            .tools
            .iter()
            .map(|tool| ToolGrant {
                server: tool.server.clone(),
                tools: tool.tools.clone(),
                policy: tool.policy,
                overrides: tool.overrides.clone(),
            })
            .collect(),
        delegation: lead
            .delegation
            .as_ref()
            .map(|delegation| ms_proto::DelegationScope {
                enabled: delegation.enabled,
                max_members: delegation.max_members,
            }),
        limits: Limits {
            memory_mb: config.microswarm.limits.memory_mb,
            pids: config.microswarm.limits.pids,
            tmp_mb: config.microswarm.limits.tmp_mb,
            max_turns: config.microswarm.limits.max_turns,
            max_tokens: config.microswarm.limits.max_tokens,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    candidate.update_hash();
    let ceiling = ScopeCeiling {
        paths: config.ceiling.paths.clone(),
        credentials: config.ceiling.credentials.clone(),
        tools: config.ceiling.tools.clone(),
        limits: None,
    };
    let mut scope = intersect_scope(&candidate, &candidate, &ceiling)
        .map_err(|error| format!("cannot intersect lead scope with ceiling: {error}"))?;
    scope.update_hash();
    Ok(scope)
}

pub fn build_member_scope(
    lead: &Scope,
    role_template: &Value,
    request: &Value,
    session_id: &str,
    delegation_id: &str,
    ceiling: &ScopeCeiling,
) -> Result<Scope, String> {
    let template_object = role_template
        .as_object()
        .ok_or("team skill role scope must be an object")?;
    let request_object = request
        .as_object()
        .ok_or("delegation scope request must be an object")?;
    let mut narrowed = Vec::new();
    let template_mounts = template_object
        .get("mounts")
        .map(|value| parse_mount_requests(value, lead, &mut narrowed))
        .transpose()?
        .unwrap_or_default();
    let requested_mounts = match request_object.get("mounts") {
        Some(value) => parse_mount_requests(value, lead, &mut narrowed)?,
        None => template_mounts.clone(),
    };
    let template_credentials =
        parse_string_list(template_object.get("credentials"), "credentials")?;
    let requested_credentials = match request_object.get("credentials") {
        Some(value) => parse_string_values(value, "credentials")?,
        None => template_credentials.clone(),
    };
    let template_tools = parse_tool_requests(template_object.get("tools"))?;
    let requested_tools = match request_object.get("tools") {
        Some(value) => parse_tool_requests(Some(value))?,
        None => template_tools.clone(),
    };
    let limits = scope_limits(&lead.limits, template_object.get("limits"))?;
    let mut candidate = Scope {
        version: lead.version,
        session_id: session_id.to_owned(),
        derived_from: DerivedFrom::Delegation,
        source_ref: delegation_id.to_owned(),
        mounts: requested_mounts,
        credentials: requested_credentials,
        tools: requested_tools,
        delegation: None,
        limits: limits.clone(),
        narrowed: Vec::new(),
        hash: String::new(),
    };
    let template = Scope {
        version: lead.version,
        session_id: session_id.to_owned(),
        derived_from: DerivedFrom::TeamSkill,
        source_ref: "team_skill".into(),
        mounts: template_mounts,
        credentials: template_credentials,
        tools: template_tools,
        delegation: None,
        limits,
        narrowed: Vec::new(),
        hash: String::new(),
    };
    candidate.update_hash();
    let template_intersection = intersect_scope(&candidate, &template, &ScopeCeiling::default())
        .map_err(|error| format!("cannot intersect member request with role template: {error}"))?;
    let mut result = intersect_scope(&template_intersection, lead, ceiling)
        .map_err(|error| format!("cannot intersect member scope with lead and ceiling: {error}"))?;
    result.narrowed.extend(narrowed);
    if !request_is_empty(request)
        && result.mounts.is_empty()
        && result.credentials.is_empty()
        && result.tools.is_empty()
    {
        return Err(
            "scope_empty: requested grants do not intersect role, lead, and ceiling".into(),
        );
    }
    result.update_hash();
    Ok(result)
}

fn request_is_empty(request: &Value) -> bool {
    ["mounts", "credentials", "tools"].into_iter().all(|field| {
        request
            .get(field)
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    })
}

fn parse_mount_requests(
    value: &Value,
    lead: &Scope,
    narrowed: &mut Vec<NarrowedItem>,
) -> Result<Vec<Mount>, String> {
    let values = value.as_array().ok_or("scope mounts must be an array")?;
    let mut result = Vec::new();
    for value in values {
        let path = value
            .get("path")
            .and_then(Value::as_str)
            .ok_or("scope mount path must be a string")?;
        let path = normalize_container_path(path)?;
        let mode = match value.get("mode").and_then(Value::as_str) {
            Some("ro") => MountMode::ReadOnly,
            Some("rw") => MountMode::ReadWrite,
            _ => return Err("scope mount mode must be ro or rw".into()),
        };
        let Some((parent, relative)) = lead
            .mounts
            .iter()
            .filter_map(|parent| {
                let parent_path =
                    normalize_container_path(&parent.container_path.to_string_lossy()).ok()?;
                path.strip_prefix(&parent_path)
                    .ok()
                    .map(|relative| (parent, relative.to_path_buf()))
            })
            .max_by_key(|(parent, _)| parent.container_path.components().count())
        else {
            narrowed.push(NarrowedItem {
                dimension: NarrowedDimension::Mount,
                item: path.display().to_string(),
                reason: NarrowedReason::NotInLeadScope,
            });
            continue;
        };
        result.push(Mount {
            host_path: parent.host_path.join(relative),
            container_path: path,
            mode,
        });
    }
    Ok(result)
}

fn normalize_container_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err("scope mount path must be absolute".into());
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => normalized.push(value),
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(_) => return Err("invalid scope mount prefix".into()),
        }
    }
    Ok(normalized)
}

fn parse_string_list(value: Option<&Value>, name: &str) -> Result<Vec<String>, String> {
    match value {
        Some(value) => parse_string_values(value, name),
        None => Ok(Vec::new()),
    }
}

fn parse_string_values(value: &Value, name: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("scope {name} must be an array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("scope {name} entries must be nonempty strings"))
        })
        .collect()
}

fn parse_tool_requests(value: Option<&Value>) -> Result<Vec<ToolGrant>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or("scope tools must be an array")?
        .iter()
        .map(|grant| {
            let server = grant
                .get("server")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or("scope tool grant requires server")?;
            let tools = match grant.get("tools") {
                None => ToolNames::all(),
                Some(Value::Array(names)) => ToolNames::Selected(
                    names
                        .iter()
                        .map(|name| {
                            name.as_str()
                                .filter(|name| !name.trim().is_empty())
                                .map(str::to_owned)
                                .ok_or("scope tool names must be nonempty strings")
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err("scope tool names must be an array".into()),
            };
            Ok(ToolGrant {
                server: server.into(),
                tools,
                policy: Permission::Allow,
                overrides: Default::default(),
            })
        })
        .collect()
}

fn scope_limits(lead: &Limits, requested: Option<&Value>) -> Result<Limits, String> {
    let mut limits = lead.clone();
    if let Some(requested) = requested {
        let object = requested
            .as_object()
            .ok_or("role scope limits must be an object")?;
        for (key, value) in object {
            let number = value
                .as_u64()
                .ok_or_else(|| format!("role limit {key} must be an integer"))?;
            match key.as_str() {
                "memory_mb" => {
                    limits.memory_mb = number.try_into().map_err(|_| "memory_mb is too large")?
                }
                "pids" => limits.pids = number.try_into().map_err(|_| "pids is too large")?,
                "tmp_mb" => limits.tmp_mb = number.try_into().map_err(|_| "tmp_mb is too large")?,
                "max_turns" => {
                    limits.max_turns = number.try_into().map_err(|_| "max_turns is too large")?
                }
                "max_tokens" => limits.max_tokens = number,
                _ => return Err(format!("unknown role limit {key:?}")),
            }
        }
    }
    Ok(limits)
}

pub fn install_lead_scope(
    store: &mut SupervisorStore,
    record: SessionRecord,
    session_dir: &Path,
    scope: &Scope,
) -> Result<(), String> {
    if record.kind != "lead" {
        return Err("lead scope can only be installed for a lead session".into());
    }
    install_authoritative_scope(store, record, session_dir, scope)
}

pub fn install_member_scope(
    store: &mut SupervisorStore,
    record: SessionRecord,
    session_dir: &Path,
    scope: &Scope,
) -> Result<(), String> {
    if record.kind != "member" || record.parent_id.is_none() {
        return Err("member scope requires a member session with a lead parent".into());
    }
    install_authoritative_scope(store, record, session_dir, scope)
}

fn install_authoritative_scope(
    store: &mut SupervisorStore,
    mut record: SessionRecord,
    session_dir: &Path,
    scope: &Scope,
) -> Result<(), String> {
    if record.id != scope.session_id || scope.hash != scope.canonical_hash() {
        return Err("session record and canonical scope identity/hash must match".into());
    }
    record.scope_hash = Some(scope.hash.clone());
    record.scope = Some(
        serde_json::to_value(scope).map_err(|error| format!("cannot serialize scope: {error}"))?,
    );
    store
        .insert_session(&record)
        .map_err(|error| format!("cannot persist authoritative scope: {error}"))?;
    write_scope_file(session_dir, scope)
}

fn write_scope_file(session_dir: &Path, scope: &Scope) -> Result<(), String> {
    let control_dir = session_dir.join("control");
    if !control_dir.is_dir() {
        return Err("session control directory must exist before writing scope".into());
    }
    let destination = control_dir.join("scope.json");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = control_dir.join(format!("scope.json.tmp-{}-{nonce}", std::process::id()));
    let bytes =
        serde_json::to_vec(scope).map_err(|error| format!("cannot serialize scope: {error}"))?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| format!("cannot create temporary scope file: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("cannot write temporary scope file: {error}"))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o444))
            .map_err(|error| format!("cannot set scope file permissions: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot fsync scope file: {error}"))?;
        fs::rename(&temporary, &destination)
            .map_err(|error| format!("cannot atomically install scope file: {error}"))?;
        File::open(&control_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("cannot fsync control directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
