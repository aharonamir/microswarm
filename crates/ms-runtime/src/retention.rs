use std::path::Path;

/// Remove expired terminal session directories while retaining supervisor.db records.
pub fn purge_expired_sessions(
    _home: &Path,
    _retention_days: u32,
    _now: &str,
) -> Result<usize, String> {
    Err("session retention is not implemented".into())
}

#[cfg(test)]
mod tests {
    use super::purge_expired_sessions;
    use ms_store::open_supervisor_db;
    use rusqlite::params;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn home() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ms-retention-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("sessions")).unwrap();
        path
    }

    fn add_session(db: &rusqlite::Connection, id: &str, status: &str, closed_at: Option<&str>) {
        db.execute(
            "INSERT INTO sessions(id,root_id,role,kind,status,created_at,last_active_at,closed_at) VALUES (?1,?1,'lead','lead',?2,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',?3)",
            params![id, status, closed_at],
        ).unwrap();
    }

    #[test]
    #[ignore = "red: T096 — session retention is not implemented"]
    fn deletes_only_expired_terminal_session_directories() {
        let home = home();
        let db = open_supervisor_db(home.join("supervisor.db")).unwrap();
        add_session(&db, "old-closed", "closed", Some("2026-09-01T00:00:00Z"));
        add_session(&db, "recent-closed", "closed", Some("2026-10-15T00:00:00Z"));
        add_session(&db, "old-active", "ready", None);
        add_session(&db, "old-failed", "failed", Some("2026-09-01T00:00:00Z"));
        for id in ["old-closed", "recent-closed", "old-active", "old-failed"] {
            fs::create_dir_all(home.join("sessions").join(id)).unwrap();
            fs::write(home.join("sessions").join(id).join("payload"), "retained").unwrap();
        }
        drop(db);

        let removed = purge_expired_sessions(&home, 30, "2026-10-30T00:00:00Z").unwrap();
        assert_eq!(removed, 2);
        assert!(!home.join("sessions/old-closed").exists());
        assert!(!home.join("sessions/old-failed").exists());
        assert!(home.join("sessions/recent-closed/payload").is_file());
        assert!(home.join("sessions/old-active/payload").is_file());

        let db = rusqlite::Connection::open(home.join("supervisor.db")).unwrap();
        let rows: i64 = db
            .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            rows, 4,
            "retention removes session directories, not authority records"
        );
        drop(db);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    #[cfg(unix)]
    #[ignore = "red: T096 — safe session directory retention is not implemented"]
    fn removes_expired_session_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let home = home();
        let outside = home.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), "safe").unwrap();
        let db = open_supervisor_db(home.join("supervisor.db")).unwrap();
        add_session(
            &db,
            "linked-session",
            "closed",
            Some("2026-09-01T00:00:00Z"),
        );
        drop(db);
        symlink(&outside, home.join("sessions/linked-session")).unwrap();

        let removed = purge_expired_sessions(&home, 30, "2026-10-30T00:00:00Z").unwrap();
        assert_eq!(removed, 1);
        assert!(!home.join("sessions/linked-session").exists());
        assert!(outside.join("keep").is_file());
        fs::remove_dir_all(home).unwrap();
    }
}
