use rusqlite::Connection;

const SCHEMA: &str = include_str!("../../specs/001-microswarm/contracts/supervisor-db.sql");

#[test]
fn supervisor_schema_applies_and_enforces_session_shape_and_pending_codes() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    db.execute_batch(SCHEMA).unwrap();

    db.execute(
        "INSERT INTO sessions (id, root_id, role, kind, status, channel, channel_thread, created_at, last_active_at)
         VALUES ('lead1', 'lead1', 'lead', 'lead', 'pending', 'cli', 'thread', 'now', 'now')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO sessions (id, root_id, parent_id, role, kind, status, created_at, last_active_at)
         VALUES ('member1', 'lead1', 'lead1', 'reviewer', 'member', 'pending', 'now', 'now')",
        [],
    )
    .unwrap();

    let malformed_lead = db.execute(
        "INSERT INTO sessions (id, root_id, parent_id, role, kind, status, created_at, last_active_at)
         VALUES ('badlead', 'badlead', 'lead1', 'lead', 'lead', 'pending', 'now', 'now')",
        [],
    );
    assert!(
        malformed_lead.is_err(),
        "lead must have no parent and root_id=id"
    );

    let malformed_member = db.execute(
        "INSERT INTO sessions (id, root_id, role, kind, status, channel, created_at, last_active_at)
         VALUES ('badmember', 'lead1', 'member', 'member', 'pending', 'cli', 'now', 'now')",
        [],
    );
    assert!(
        malformed_member.is_err(),
        "member must have a parent and no channel"
    );

    for id in ["approval1", "approval2"] {
        let result = db.execute(
            "INSERT INTO approvals (id, code, root_session_id, requesting_session_id, kind, payload, status, expires_at, created_at)
             VALUES (?1, 'AB12', 'lead1', 'lead1', 'tool_call', '{}', 'pending', 'later', 'now')",
            [id],
        );
        if id == "approval1" {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err(), "pending approval codes must be unique");
        }
    }
}
