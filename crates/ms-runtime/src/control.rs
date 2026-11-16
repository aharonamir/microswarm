use ms_proto::Scope;
use ms_store::SupervisorStore;
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn copy_session_instructions(
    session_dir: &Path,
    source: Option<&Path>,
) -> Result<bool, String> {
    let Some(source) = source else {
        return Ok(false);
    };
    if !source.is_file() {
        return Err(format!(
            "session instruction source is not a file: {}",
            source.display()
        ));
    }
    let control = session_dir.join("control");
    fs::create_dir_all(&control).map_err(|error| error.to_string())?;
    let destination = control.join("instructions.md");
    let temporary = control.join("instructions.md.tmp");
    fs::copy(source, &temporary)
        .map_err(|error| format!("cannot copy session instructions: {error}"))?;
    let file = fs::File::open(&temporary).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o444))
        .map_err(|error| error.to_string())?;
    fs::rename(&temporary, &destination).map_err(|error| error.to_string())?;
    if let Some((skill, version)) = source
        .ancestors()
        .take(4)
        .find_map(|path| {
            path.join("SKILL.md")
                .is_file()
                .then(|| fs::read_to_string(path.join("SKILL.md")).ok())
                .flatten()
        })
        .and_then(|source| skill_identity(&source))
    {
        write_control_json(
            session_dir,
            "instructions.meta.json",
            &serde_json::json!({"source":format!("{skill}@{version}")}),
        )?;
    }
    fs::File::open(&control)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

fn skill_identity(source: &str) -> Option<(String, String)> {
    let mut lines = source.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut name = None;
    let mut version = None;
    for line in lines.take_while(|line| *line != "---") {
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(value.trim().trim_matches(['\"', '\'']).to_owned());
        }
        if let Some(value) = line.strip_prefix("version:") {
            version = Some(value.trim().trim_matches(['\"', '\'']).to_owned());
        }
    }
    Some((name?, version?))
}

pub fn write_team_roster(
    store: &SupervisorStore,
    home: &Path,
    session_dir: &Path,
    lead_scope: &Scope,
) -> Result<Value, String> {
    let mut statement = store
        .connection()
        .prepare("SELECT skill, role, spawn_policy, scope, instructions FROM team_roles ORDER BY skill, role")
        .map_err(|error| error.to_string())?;
    let records = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut roster = Vec::new();
    for (skill, role, spawn_policy, template, instructions) in records {
        let template: Value = serde_json::from_str(&template)
            .map_err(|error| format!("invalid scope template for {skill}/{role}: {error}"))?;
        let Some(scope_summary) = intersecting_summary(&template, lead_scope) else {
            continue;
        };
        let skill_root = skill_root(home, &instructions, &skill);
        let description = skill_root
            .as_ref()
            .and_then(|path| fs::read_to_string(path.join("SKILL.md")).ok())
            .and_then(|source| skill_description(&source))
            .unwrap_or_default();
        roster.push(serde_json::json!({
            "role": role,
            "skill": skill,
            "description": description,
            "spawn_policy": spawn_policy,
            "scope_summary": scope_summary,
        }));
    }
    let value = Value::Array(roster);
    write_control_json(session_dir, "team.json", &value)?;
    Ok(value)
}

fn write_control_json(session_dir: &Path, name: &str, value: &Value) -> Result<(), String> {
    let control = session_dir.join("control");
    fs::create_dir_all(&control).map_err(|error| error.to_string())?;
    let destination = control.join(name);
    let temporary = control.join(format!("{name}.tmp"));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o444)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec(value).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o444))
        .map_err(|error| error.to_string())?;
    fs::rename(temporary, destination).map_err(|error| error.to_string())?;
    fs::File::open(control)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn skill_root(home: &Path, instructions: &str, skill: &str) -> Option<PathBuf> {
    let instruction_path = PathBuf::from(instructions);
    instruction_path
        .ancestors()
        .take(4)
        .find(|path| {
            path.join("SKILL.md").is_file() && path.file_name().is_some_and(|name| name == skill)
        })
        .filter(|path| path.starts_with(home))
        .map(Path::to_path_buf)
}

fn skill_description(source: &str) -> Option<String> {
    let mut lines = source.lines();
    if lines.next()? != "---" {
        return None;
    }
    for line in lines.take_while(|line| *line != "---") {
        if let Some(value) = line.strip_prefix("description:") {
            return Some(value.trim().trim_matches(['\"', '\'']).to_owned());
        }
    }
    None
}

fn intersecting_summary(template: &Value, lead: &Scope) -> Option<String> {
    let mut grants = Vec::new();
    if let Some(mounts) = template.get("mounts").and_then(Value::as_array) {
        for mount in mounts {
            let path = mount.get("path").and_then(Value::as_str)?;
            let requested = Path::new(path);
            if let Some(parent) = lead.mounts.iter().find(|parent| {
                let container = &parent.container_path;
                requested.starts_with(container) || container.starts_with(requested)
            }) {
                let requested_mode = match mount.get("mode").and_then(Value::as_str) {
                    Some("rw") => ms_proto::MountMode::ReadWrite,
                    _ => ms_proto::MountMode::ReadOnly,
                };
                grants.push(format!(
                    "{} ({})",
                    path,
                    requested_mode.min(parent.mode).as_str()
                ));
            }
        }
    }
    if let Some(credentials) = template.get("credentials").and_then(Value::as_array) {
        for credential in credentials.iter().filter_map(Value::as_str) {
            if lead.credentials.iter().any(|value| value == credential) {
                grants.push(format!("credential {credential}"));
            }
        }
    }
    if let Some(tools) = template.get("tools").and_then(Value::as_array) {
        for tool in tools {
            let Some(server) = tool.get("server").and_then(Value::as_str) else {
                continue;
            };
            let requested = tool.get("tools").and_then(Value::as_array);
            for parent in lead.tools.iter().filter(|grant| grant.server == server) {
                match requested {
                    Some(names) => {
                        for name in names.iter().filter_map(Value::as_str) {
                            if parent.tools.contains(name) {
                                grants.push(format!("{server}/{name}"));
                            }
                        }
                    }
                    None => match &parent.tools {
                        ms_proto::ToolNames::All(_) => grants.push(format!("{server}/*")),
                        ms_proto::ToolNames::Selected(names) => {
                            grants.extend(names.iter().map(|name| format!("{server}/{name}")))
                        }
                    },
                }
            }
        }
    }
    (!grants.is_empty()).then(|| grants.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ms-runtime-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn copies_instructions_into_control_read_only() {
        let dir = temp_dir("instructions");
        let source = dir.join("lead.md");
        fs::write(&source, "operator guidance").unwrap();
        let session = dir.join("session");
        fs::create_dir_all(&session).unwrap();
        assert!(copy_session_instructions(&session, Some(&source)).unwrap());
        assert_eq!(
            fs::read_to_string(session.join("control/instructions.md")).unwrap(),
            "operator guidance"
        );
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &fs::metadata(session.join("control/instructions.md"))
                .unwrap()
                .permissions(),
        );
        assert_eq!(mode & 0o222, 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lead_roster_lists_only_roles_intersecting_its_scope() {
        let dir = temp_dir("roster");
        let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
        let skill_root = dir.join("skills/review");
        fs::create_dir_all(skill_root.join("roles")).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: review\ndescription: Reviews code.\nkind: team\n---\n",
        )
        .unwrap();
        let inside = skill_root.join("roles/inside.md");
        let outside = skill_root.join("roles/outside.md");
        fs::write(&inside, "instructions").unwrap();
        fs::write(&outside, "instructions").unwrap();
        for (role, path) in [("inside", "/work/src"), ("outside", "/data/private")] {
            store
                .upsert_team_role(&ms_store::TeamRoleRecord {
                    skill: "review".into(),
                    skill_version: "1.0".into(),
                    skill_hash: "hash".into(),
                    role: role.into(),
                    spawn_policy: "ask".into(),
                    scope: serde_json::json!({"mounts":[{"path":path,"mode":"ro"}]}),
                    instructions: if role == "inside" {
                        inside.display().to_string()
                    } else {
                        outside.display().to_string()
                    },
                })
                .unwrap();
        }
        let lead_scope: Scope = serde_json::from_value(serde_json::json!({
            "version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config","source_ref":"config",
            "mounts":[{"host_path":"/host/src","container_path":"/work","mode":"ro"}],"credentials":[],"tools":[],"delegation":{"enabled":true,"max_members":2},
            "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},"narrowed":[],"hash":"hash"
        })).unwrap();
        let session_dir = dir.join("session");
        fs::create_dir_all(&session_dir).unwrap();
        let roster = write_team_roster(&store, &dir, &session_dir, &lead_scope).unwrap();
        assert_eq!(roster.as_array().unwrap().len(), 1);
        assert_eq!(roster[0]["role"], "inside");
        assert_eq!(roster[0]["description"], "Reviews code.");
        assert_eq!(
            fs::read_to_string(session_dir.join("control/team.json")).unwrap(),
            roster.to_string()
        );
        drop(store);
        let _ = fs::remove_dir_all(dir);
    }
}
