use serde::Deserialize;
use std::collections::BTreeMap;

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
        let _ = &role.limits;
    }
    Ok(ParsedTeamSkill {
        name: document.name,
        version: document.version,
        roles: document.roles.into_keys().collect(),
    })
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
