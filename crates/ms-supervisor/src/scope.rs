use crate::config::ConfigSet;
use ms_proto::{DerivedFrom, Limits, Mount, Scope, ScopeCeiling, ToolGrant, intersect_scope};
use ms_store::{SessionRecord, SupervisorStore};
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

pub fn install_lead_scope(
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
