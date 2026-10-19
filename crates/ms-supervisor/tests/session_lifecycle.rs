use ms_supervisor::sessions::{SessionEvent, SessionLifecycle, SessionStatus};

#[test]
#[ignore = "red: T039 — persisted session lifecycle state machine is not implemented"]
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
