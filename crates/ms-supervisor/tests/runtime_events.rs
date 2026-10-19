use bollard::models::{EventActor, EventMessage, EventMessageTypeEnum};
use ms_supervisor::runtime::exit_event_from;
use std::collections::HashMap;

fn event(action: &str) -> EventMessage {
    EventMessage {
        typ: Some(EventMessageTypeEnum::CONTAINER),
        action: Some(action.into()),
        actor: Some(EventActor {
            id: Some("container-123".into()),
            attributes: Some(HashMap::from([
                ("ms.managed".into(), "true".into()),
                ("ms.session".into(), "01J00000000000000000000000".into()),
                ("exitCode".into(), "77".into()),
            ])),
        }),
        ..Default::default()
    }
}

#[test]
#[ignore = "red: T037 — runtime event stream is not connected to session exits"]
fn watcher_maps_only_managed_container_exit_events() {
    let exit = exit_event_from(&event("die")).expect("managed die event is forwarded");
    assert_eq!(exit.container_id, "container-123");
    assert_eq!(exit.session_id, "01J00000000000000000000000");
    assert_eq!(exit.exit_code, Some(77));
    assert!(exit_event_from(&event("start")).is_none());

    let mut unmanaged = event("die");
    unmanaged
        .actor
        .as_mut()
        .unwrap()
        .attributes
        .as_mut()
        .unwrap()
        .remove("ms.managed");
    assert!(exit_event_from(&unmanaged).is_none());
}
