use ms_store::{SessionRecord, SupervisorStore};
use ms_supervisor::sessions::{SessionEvent, SessionLifecycle, SessionStatus};
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

#[test]
fn lifecycle_handles_turns_graceful_exit_restarts_and_fail_closed_codes() {
    let mut session = SessionLifecycle::new(3);
    assert_eq!(session.status(), SessionStatus::Pending);
    assert_eq!(
        session.apply(SessionEvent::Start).unwrap(),
        SessionStatus::Starting
    );
    assert_eq!(
        session.apply(SessionEvent::AgentReady).unwrap(),
        SessionStatus::Ready
    );
    assert_eq!(
        session.apply(SessionEvent::TurnStarted).unwrap(),
        SessionStatus::Busy
    );
    assert_eq!(
        session.apply(SessionEvent::TurnFinished).unwrap(),
        SessionStatus::Ready
    );
    assert_eq!(
        session.apply(SessionEvent::Shutdown).unwrap(),
        SessionStatus::Stopping
    );
    assert_eq!(
        session
            .apply(SessionEvent::ContainerExited {
                code: 0,
                oom_killed: false
            })
            .unwrap(),
        SessionStatus::Closed
    );
    assert!(session.apply(SessionEvent::Start).is_err());

    let mut transient = SessionLifecycle::new(1);
    transient.apply(SessionEvent::Start).unwrap();
    transient.apply(SessionEvent::AgentReady).unwrap();
    assert_eq!(
        transient
            .apply(SessionEvent::ContainerExited {
                code: 75,
                oom_killed: false
            })
            .unwrap(),
        SessionStatus::Crashed
    );
    assert_eq!(
        transient.apply(SessionEvent::Restart).unwrap(),
        SessionStatus::Starting
    );
    assert_eq!(
        transient
            .apply(SessionEvent::ContainerExited {
                code: 75,
                oom_killed: false
            })
            .unwrap(),
        SessionStatus::Failed
    );

    let mut unknown = SessionLifecycle::new(3);
    unknown.apply(SessionEvent::Start).unwrap();
    unknown.apply(SessionEvent::AgentReady).unwrap();
    assert_eq!(
        unknown
            .apply(SessionEvent::ContainerExited {
                code: 42,
                oom_killed: false
            })
            .unwrap(),
        SessionStatus::Failed
    );

    let mut oom = SessionLifecycle::new(3);
    oom.apply(SessionEvent::Start).unwrap();
    oom.apply(SessionEvent::AgentReady).unwrap();
    assert_eq!(
        oom.apply(SessionEvent::ContainerExited {
            code: 137,
            oom_killed: true
        })
        .unwrap(),
        SessionStatus::Crashed
    );
}

#[test]
fn lifecycle_status_and_restart_count_are_persisted_to_supervisor_db() {
    let dir = std::env::temp_dir().join(format!(
        "ms-session-lifecycle-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    let id = "01J00000000000000000000000";
    store
        .insert_session(&SessionRecord {
            id: id.into(),
            root_id: id.into(),
            parent_id: None,
            role: "lead".into(),
            kind: "lead".into(),
            channel: Some("cli".into()),
            channel_thread: Some("thread-1".into()),
            status: "pending".into(),
            scope_hash: None,
            scope: None,
            created_at: "2026-10-19T00:00:00Z".into(),
            last_active_at: "2026-10-19T00:00:00Z".into(),
        })
        .unwrap();
    let mut lifecycle = SessionLifecycle::new(2);
    lifecycle.apply(SessionEvent::Start).unwrap();
    lifecycle
        .persist(&mut store, id, Some("container-1"), "2026-10-19T00:00:01Z")
        .unwrap();
    lifecycle.apply(SessionEvent::AgentReady).unwrap();
    lifecycle
        .persist(&mut store, id, None, "2026-10-19T00:00:02Z")
        .unwrap();
    lifecycle
        .apply(SessionEvent::ContainerExited {
            code: 75,
            oom_killed: false,
        })
        .unwrap();
    lifecycle
        .persist(&mut store, id, None, "2026-10-19T00:00:03Z")
        .unwrap();

    let (status, container, restart_count): (String, Option<String>, i64) = store
        .connection()
        .query_row(
            "SELECT status, container_id, restart_count FROM sessions WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "crashed");
    assert_eq!(container.as_deref(), Some("container-1"));
    assert_eq!(restart_count, 1);
    let restored = SessionLifecycle::restore(&status, restart_count as u8, 2).unwrap();
    assert_eq!(restored.status(), SessionStatus::Crashed);
    assert!(SessionLifecycle::restore("invented", 0, 2).is_err());
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
