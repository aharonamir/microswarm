use ms_store::{SupervisorStore, TeamRoleRecord};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedTeamSkill {
    pub name: String,
    pub version: String,
    pub roles: Vec<String>,
}

#[derive(Deserialize)]
struct TeamSkillDocument {
    name: String,
    version: String,
    description: String,
    kind: String,
    roles: BTreeMap<String, RoleDocument>,
}

#[derive(Deserialize)]
struct RoleDocument {
    spawn_policy: String,
    instructions: String,
    scope: serde_json::Value,
    #[serde(default)]
    limits: Option<serde_json::Value>,
}

pub fn parse_team_skill(source: &str) -> Result<ParsedTeamSkill, String> {
    let document = parse_document(source)?;
    Ok(ParsedTeamSkill {
        name: document.name,
        version: document.version,
        roles: document.roles.into_keys().collect(),
    })
}

fn parse_document(source: &str) -> Result<TeamSkillDocument, String> {
    let mut lines = source.lines();
    if lines.next() != Some("---") {
        return Err("team skill must begin with YAML front-matter".into());
    }
    let Some(end) = lines.position(|line| line == "---") else {
        return Err("team skill YAML front-matter is not closed".into());
    };
    let yaml = source
        .lines()
        .skip(1)
        .take(end)
        .collect::<Vec<_>>()
        .join("\n");
    let document: TeamSkillDocument = serde_saphyr::from_str(&yaml)
        .map_err(|error| format!("invalid team skill YAML: {error}"))?;
    if document.kind != "team" {
        return Err("team skill kind must be 'team'".into());
    }
    if !valid_skill_name(&document.name) || document.version.trim().is_empty() {
        return Err("team skill name or version is invalid".into());
    }
    if document.description.trim().is_empty() {
        return Err("team skill description must not be empty".into());
    }
    if document.roles.is_empty() {
        return Err("team skill must declare at least one role".into());
    }
    for (name, role) in &document.roles {
        if !valid_skill_name(name) {
            return Err(format!("invalid team skill role name {name:?}"));
        }
        if !matches!(role.spawn_policy.as_str(), "allow" | "ask" | "deny") {
            return Err(format!("invalid spawn_policy for role {name:?}"));
        }
        if !valid_relative_path(&role.instructions) {
            return Err(format!("invalid instructions path for role {name:?}"));
        }
        if !role.scope.is_object() {
            return Err(format!("scope for role {name:?} must be an object"));
        }
        validate_scope_template(&role.scope, name)?;
        if let Some(limits) = &role.limits {
            validate_role_limits(limits, name)?;
        }
    }
    Ok(document)
}

pub fn install_team_skill(
    store: &mut SupervisorStore,
    skills_home: &Path,
    source_dir: &Path,
) -> Result<ParsedTeamSkill, String> {
    let source_dir = fs::canonicalize(source_dir)
        .map_err(|error| format!("cannot resolve team skill directory: {error}"))?;
    if !source_dir.is_dir() {
        return Err("team skill source must be a directory".into());
    }
    let source = fs::read_to_string(source_dir.join("SKILL.md"))
        .map_err(|error| format!("cannot read team skill SKILL.md: {error}"))?;
    let document = parse_document(&source)?;
    let skill_hash = hash_tree(&source_dir)?;
    for (role_name, role) in &document.roles {
        let instructions = source_dir.join(&role.instructions);
        let resolved = fs::canonicalize(&instructions).map_err(|error| {
            format!("cannot resolve instructions for role {role_name:?}: {error}")
        })?;
        if !resolved.starts_with(&source_dir) || !resolved.is_file() {
            return Err(format!(
                "instructions for role {role_name:?} escape the skill directory"
            ));
        }
    }

    fs::create_dir_all(skills_home)
        .map_err(|error| format!("cannot create skills directory: {error}"))?;
    let target = skills_home.join(&document.name);
    let temporary = unique_sibling(skills_home, &document.name, "install");
    let backup = unique_sibling(skills_home, &document.name, "backup");
    copy_tree(&source_dir, &temporary)?;
    let had_target = target.exists();
    if had_target {
        fs::rename(&target, &backup)
            .map_err(|error| format!("cannot stage existing team skill: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, &target) {
        if had_target {
            let _ = fs::rename(&backup, &target);
        }
        let _ = fs::remove_dir_all(&temporary);
        return Err(format!("cannot install team skill atomically: {error}"));
    }

    let roles = document
        .roles
        .iter()
        .map(|(name, role)| {
            let mut scope = role.scope.clone();
            if let Some(limits) = &role.limits {
                scope["limits"] = limits.clone();
            }
            TeamRoleRecord {
                skill: document.name.clone(),
                skill_version: document.version.clone(),
                skill_hash: skill_hash.clone(),
                role: name.clone(),
                spawn_policy: role.spawn_policy.clone(),
                scope,
                instructions: target
                    .join(&role.instructions)
                    .to_string_lossy()
                    .into_owned(),
            }
        })
        .collect::<Vec<_>>();
    if let Err(error) = store.replace_team_roles(&document.name, &roles) {
        let _ = fs::remove_dir_all(&target);
        if had_target {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("cannot register team skill roles: {error}"));
    }
    if had_target {
        fs::remove_dir_all(&backup).map_err(|error| {
            format!("team skill installed but old version cleanup failed: {error}")
        })?;
    }
    Ok(ParsedTeamSkill {
        name: document.name,
        version: document.version,
        roles: document.roles.into_keys().collect(),
    })
}

fn validate_scope_template(scope: &serde_json::Value, role: &str) -> Result<(), String> {
    let object = scope
        .as_object()
        .ok_or_else(|| format!("scope for role {role:?} must be an object"))?;
    reject_unknown(object, &["mounts", "credentials", "tools"], role)?;
    if let Some(mounts) = scope.get("mounts") {
        let mounts = mounts
            .as_array()
            .ok_or_else(|| format!("mounts for role {role:?} must be an array"))?;
        for mount in mounts {
            let mount = mount
                .as_object()
                .ok_or_else(|| format!("mount for role {role:?} must be an object"))?;
            reject_unknown(mount, &["path", "mode"], role)?;
            let path = mount.get("path").and_then(serde_json::Value::as_str);
            if !path.is_some_and(|path| {
                path.starts_with('/') && valid_relative_path(path.trim_start_matches('/'))
            }) {
                return Err(format!(
                    "mount path for role {role:?} must be an absolute normalized path"
                ));
            }
            if !matches!(
                mount.get("mode").and_then(serde_json::Value::as_str),
                Some("ro" | "rw")
            ) {
                return Err(format!("mount mode for role {role:?} must be ro or rw"));
            }
        }
    }
    if let Some(credentials) = scope.get("credentials")
        && !credentials.as_array().is_some_and(|values| {
            values
                .iter()
                .all(|value| value.as_str().is_some_and(|name| !name.trim().is_empty()))
        })
    {
        return Err(format!(
            "credentials for role {role:?} must be nonempty strings"
        ));
    }
    if let Some(tools) = scope.get("tools") {
        let tools = tools
            .as_array()
            .ok_or_else(|| format!("tools for role {role:?} must be an array"))?;
        for grant in tools {
            let grant = grant
                .as_object()
                .ok_or_else(|| format!("tool grant for role {role:?} must be an object"))?;
            reject_unknown(grant, &["server", "tools"], role)?;
            if !grant
                .get("server")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|server| !server.trim().is_empty())
            {
                return Err(format!("tool grant for role {role:?} requires a server"));
            }
            if let Some(names) = grant.get("tools")
                && !names.as_array().is_some_and(|names| {
                    names
                        .iter()
                        .all(|name| name.as_str().is_some_and(|name| !name.is_empty()))
                })
            {
                return Err(format!("tool names for role {role:?} must be strings"));
            }
        }
    }
    Ok(())
}

fn validate_role_limits(limits: &serde_json::Value, role: &str) -> Result<(), String> {
    let limits = limits
        .as_object()
        .ok_or_else(|| format!("limits for role {role:?} must be an object"))?;
    reject_unknown(limits, &["max_turns", "memory_mb"], role)?;
    for key in ["max_turns", "memory_mb"] {
        if let Some(value) = limits.get(key)
            && value.as_u64().is_none_or(|number| number == 0)
        {
            return Err(format!(
                "role limit {key} for {role:?} must be a positive integer"
            ));
        }
    }
    Ok(())
}

fn reject_unknown(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    role: &str,
) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unknown field {key:?} in role {role:?}"));
    }
    Ok(())
}

fn hash_tree(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut hash = Sha256::new();
    for relative in files {
        let name = relative
            .to_str()
            .ok_or_else(|| "team skill path is not valid UTF-8".to_owned())?;
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(fs::read(root.join(&relative)).map_err(|error| error.to_string())?);
        hash.update([0xff]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "team skill may not contain symlinks: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(
                path.strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_path_buf(),
            );
        } else {
            return Err(format!(
                "team skill contains an unsupported file: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|error| format!("cannot create temporary skill directory: {error}"))?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "team skill may not contain symlinks: {}",
                from.display()
            ));
        }
        if metadata.is_dir() {
            copy_tree(&from, &to)?;
        } else if metadata.is_file() {
            fs::copy(&from, &to)
                .map_err(|error| format!("cannot copy team skill file: {error}"))?;
        } else {
            return Err(format!(
                "team skill contains an unsupported file: {}",
                from.display()
            ));
        }
    }
    Ok(())
}

fn unique_sibling(parent: &Path, name: &str, suffix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{name}.{suffix}-{}-{nonce}", std::process::id()))
}

fn valid_skill_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_relative_path(value: &str) -> bool {
    let path = std::path::Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}
