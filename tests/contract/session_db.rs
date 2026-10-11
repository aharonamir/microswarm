use rusqlite::Connection;

const SCHEMA: &str = include_str!("../../specs/001-microswarm/contracts/session-db.sql");

#[test]
fn session_schema_applies_and_rejects_unknown_kinds_and_invalid_json() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    db.execute_batch(SCHEMA).unwrap();

    let valid = db.execute(
        "INSERT INTO inbound(kind, body) VALUES ('user_message', '{\"text\":\"hello\"}')",
        [],
    );
    assert!(valid.is_ok());

    let unknown_kind = db.execute(
        "INSERT INTO inbound(kind, body) VALUES ('unknown', '{}')",
        [],
    );
    assert!(
        unknown_kind.is_err(),
        "unknown inbound kind must violate CHECK"
    );

    let invalid_json = db.execute(
        "INSERT INTO inbound(kind, body) VALUES ('user_message', '{')",
        [],
    );
    assert!(
        invalid_json.is_err(),
        "invalid JSON body must violate CHECK"
    );
}
