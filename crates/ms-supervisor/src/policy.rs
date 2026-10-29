use crate::config::SpawnPolicy;

pub fn resolve_spawn_policy(
    skill_policy: Option<SpawnPolicy>,
    host_override: Option<SpawnPolicy>,
    default_policy: SpawnPolicy,
) -> SpawnPolicy {
    let Some(skill_policy) = skill_policy else {
        return host_override.unwrap_or(default_policy);
    };
    let host_policy = host_override.unwrap_or(SpawnPolicy::Allow);
    if policy_rank(skill_policy) >= policy_rank(host_policy) {
        skill_policy
    } else {
        host_policy
    }
}

const fn policy_rank(policy: SpawnPolicy) -> u8 {
    match policy {
        SpawnPolicy::Allow => 0,
        SpawnPolicy::Ask => 1,
        SpawnPolicy::Deny => 2,
    }
}
