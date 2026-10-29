use ms_store::SupervisorStore;
use ms_supervisor::skills::parse_team_skill;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

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

static NEXT: AtomicUsize = AtomicUsize::new(0);

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

#[test]
fn installation_registers_role_hash_and_installed_instruction_path() {
    let home = temporary_home("install");
    fs::create_dir_all(&home).unwrap();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/us2-team");
    let skill =
        ms_supervisor::skills::install_team_skill(&mut store, &home.join("skills"), &source)
            .expect("valid skill should install");
    assert_eq!(skill.name, "us2-test-team");
    let row: (String, String, String) = store.connection().query_row(
        "SELECT skill_version, skill_hash, instructions FROM team_roles WHERE skill=?1 AND role='researcher'",
        [&skill.name],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(row.0, "1.0.0");
    assert_eq!(row.1.len(), 64);
    assert!(PathBuf::from(&row.2).is_file());
    assert!(
        row.2
            .starts_with(home.join("skills").join(&skill.name).to_str().unwrap())
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn invalid_install_leaves_database_and_skill_directory_unchanged() {
    let home = temporary_home("atomic");
    let source = home.join("source");
    fs::create_dir_all(source.join("roles")).unwrap();
    fs::write(source.join("roles/researcher.md"), "instructions").unwrap();
    fs::write(
        source.join("SKILL.md"),
        VALID_SKILL.replace("spawn_policy: allow", "spawn_policy: maybe"),
    )
    .unwrap();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    assert!(
        ms_supervisor::skills::install_team_skill(&mut store, &home.join("skills"), &source)
            .is_err()
    );
    let count: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM team_roles", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert!(!home.join("skills/review-team").exists());
    fs::remove_dir_all(home).unwrap();
}

fn temporary_home(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ms-team-skill-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
