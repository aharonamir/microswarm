use ms_supervisor::{config::SpawnPolicy, policy::resolve_spawn_policy};

#[test]
fn effective_role_policy_is_the_stricter_skill_and_host_policy() {
    use SpawnPolicy::{Allow, Ask, Deny};
    for (skill, host, expected) in [
        (Allow, Allow, Allow),
        (Allow, Ask, Ask),
        (Allow, Deny, Deny),
        (Ask, Allow, Ask),
        (Ask, Ask, Ask),
        (Ask, Deny, Deny),
        (Deny, Allow, Deny),
        (Deny, Ask, Deny),
        (Deny, Deny, Deny),
    ] {
        assert_eq!(
            resolve_spawn_policy(Some(skill), Some(host), Deny),
            expected
        );
    }
    assert_eq!(resolve_spawn_policy(None, None, Ask), Ask);
    assert_eq!(resolve_spawn_policy(None, Some(Deny), Ask), Deny);
}
