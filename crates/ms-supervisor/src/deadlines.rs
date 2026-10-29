//! Persisted delegation deadlines, processed independently of a live chat request.
use crate::{
    config::load_config_dir,
    runtime::ContainerRuntime,
    service::{config_dir, utc_now},
};
use ms_proto::InboundKind;
use ms_store::{SupervisorStore, open_session_dir};
use serde_json::json;
use std::{path::Path, time::Duration};

static PROCESS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn process_due(home: &Path) -> Result<(), String> {
    let _guard = PROCESS_LOCK.lock().await;
    let store = SupervisorStore::open(home.join("supervisor.db")).map_err(|e| e.to_string())?;
    let due = {
        let mut s=store.connection().prepare("SELECT d.id,d.lead_id,d.member_id FROM delegations d JOIN sessions s ON s.id=d.member_id WHERE d.deadline_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now') AND (d.status='waiting_reply' OR (d.status='failed' AND d.failure_reason='timeout' AND s.status='stopping'))").map_err(|e|e.to_string())?;
        s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    };
    for (delegation, lead, member) in due {
        let now = utc_now();
        let claimed=store.connection().execute("UPDATE delegations SET status='failed',failure_reason='timeout',updated_at=?2 WHERE id=?1 AND status='waiting_reply' AND deadline_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now')",[&delegation,&now]).map_err(|e|e.to_string())?;
        if claimed == 0 {
            let retry: bool = store.connection().query_row("SELECT EXISTS(SELECT 1 FROM delegations d JOIN sessions s ON s.id=d.member_id WHERE d.id=?1 AND d.failure_reason='timeout' AND s.status='stopping')",[&delegation],|row|row.get(0)).map_err(|e|e.to_string())?;
            if !retry {
                continue;
            }
        }
        let config = load_config_dir(config_dir(home)).map_err(|e| e.to_string())?;
        store.connection().execute("UPDATE sessions SET status='stopping' WHERE id=?1 AND status NOT IN ('closed','failed')",[&member]).map_err(|e|e.to_string())?;
        if let Some(container) = store
            .session_container_id(&member)
            .map_err(|e| e.to_string())?
        {
            let runtime = ContainerRuntime::connect(&config.microswarm.runtime.socket)?;
            if runtime.is_running(&container).await? {
                runtime.stop(&container, Duration::from_secs(2)).await?;
            }
        }
        store.connection().execute("UPDATE sessions SET status='failed',close_reason='failed',closed_at=?2,last_active_at=?2 WHERE id=?1 AND status='stopping'",[&member,&now]).map_err(|e|e.to_string())?;
        open_session_dir(home.join("sessions").join(&lead))
            .map_err(|e| e.to_string())?
            .append_inbound(
                &InboundKind::MemberFailed,
                &json!({"member_id":member,"delegation_id":delegation,"reason":"timeout"}),
            )
            .map_err(|e| e.to_string())?;
        if let Some(container) = store
            .session_container_id(&lead)
            .map_err(|e| e.to_string())?
        {
            let runtime = ContainerRuntime::connect(&config.microswarm.runtime.socket)?;
            let _ = runtime.signal(&container, "SIGUSR1").await;
        }
    }
    Ok(())
}
