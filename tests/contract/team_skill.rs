use ms_supervisor::skills::parse_team_skill;

const VALID_SKILL: &str = r#"---
name: review-team
version: 1.2.0
description: Reviews a change.
kind: team
roles:
  researcher:
    spawn_policy: allow
    instructions: roles/researcher.md
    scope:
      mounts:
        - { path: /work, mode: ro }
      credentials: []
      tools:
        - { server: microswarm-tools, tools: [fs.read, fs.list] }
    limits: { max_turns: 30, memory_mb: 256 }
---
Delegate research to the researcher role.
"#;

#[test]
fn parser_accepts_the_documented_team_skill_shape() {
    let parsed = parse_team_skill(VALID_SKILL).expect("valid team skill should parse");
    assert_eq!(parsed.name, "review-team");
    assert_eq!(parsed.version, "1.2.0");
    assert_eq!(parsed.roles, ["researcher"]);
}

#[test]
fn parser_rejects_invalid_role_metadata_without_returning_partial_roles() {
    let invalid = VALID_SKILL.replace("spawn_policy: allow", "spawn_policy: maybe");
    assert!(parse_team_skill(&invalid).is_err());
}
