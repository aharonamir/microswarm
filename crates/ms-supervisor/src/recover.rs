//! Reconcile persisted supervisor state with the container runtime before RPCs open.
use crate::{
    config::ConfigSet,
    proxy::SessionProxy,
    runtime::{CredentialProvider, FileCredentials, recovery},
    service::{proxy_routes, runtime_for},
};
use ms_proto::SupervisorEventType;
use ms_store::SupervisorStore;
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::net::TcpListener;

pub async fn boot(home: &Path, config: &ConfigSet) -> Result<(), String> {
    let runtime = runtime_for(config)?;
    let scan = recovery::scan(home, &runtime).await?;
    {
        let mut store =
            SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
        for (session, container) in &scan.adopted {
            let environment = runtime.environment(&container.id).await?;
            let proxy_url = environment
                .iter()
                .find_map(|value| value.strip_prefix("MS_PROXY_URL="))
                .ok_or_else(|| format!("adopted session {} has no proxy URL", session.id))?;
            let port = proxy_url
                .parse::<http::Uri>()
                .map_err(|error| format!("invalid proxy URL for {}: {error}", session.id))?
                .port_u16()
                .ok_or_else(|| format!("proxy URL for {} has no port", session.id))?;
            let listener = TcpListener::bind(("0.0.0.0", port))
                .await
                .map_err(|error| format!("cannot restore proxy for {}: {error}", session.id))?;
            let proxy =
                SessionProxy::from_store(&store, &session.id).map_err(|error| error.to_string())?;
            spawn_proxy(listener, proxy, config, home);
            store
                .update_session_lifecycle(&ms_store::SessionLifecycleUpdate {
                    session_id: session.id.clone(),
                    status: "ready".into(),
                    container_id: Some(container.id.clone()),
                    restart_count: session.restart_count,
                    last_active_at: ms_runtime::utc_now(),
                    closed_at: None,
                    close_reason: None,
                })
                .map_err(|error| error.to_string())?;
            recovery::record_event(
                &mut store,
                &session.id,
                &session.root,
                &session.role,
                SupervisorEventType::ContainerAdopted,
                json!({"container_id":container.id}),
            )?;
        }
        recovery::reissue_pending(
            &mut store,
            Duration::from_secs(config.microswarm.limits.approval_timeout_s),
        )?;
    }

    let model = config.microswarm.model.as_ref();
    let recovery_config = recovery::RecoveryConfig {
        image: config.microswarm.runtime.image.clone(),
        model_provider: model.map(|model| model.provider.clone()),
        model_name: model.map(|model| model.model.clone()),
        model_credential: model.map(|model| model.credential.clone()),
        mcp_servers: serde_json::to_string(&crate::service::mcp_descriptors(config))
            .map_err(|error| error.to_string())?,
        restart_max: config.microswarm.limits.restart_max,
    };
    for session in scan.missing {
        let listener = TcpListener::bind(("0.0.0.0", 0))
            .await
            .map_err(|error| error.to_string())?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        recovery::recreate_missing(home, &runtime, &session, &recovery_config, port).await?;
        let store =
            SupervisorStore::open(home.join("supervisor.db")).map_err(|error| error.to_string())?;
        let proxy =
            SessionProxy::from_store(&store, &session.id).map_err(|error| error.to_string())?;
        spawn_proxy(listener, proxy, config, home);
    }

    // The persisted wall clock deadline is authoritative; processing due rows at boot
    // catches deadlines that elapsed while the supervisor was down.
    crate::deadlines::process_due(home).await?;
    Ok(())
}

fn spawn_proxy(listener: TcpListener, proxy: SessionProxy, config: &ConfigSet, home: &Path) {
    let routes = proxy_routes(config);
    let credentials: Arc<dyn CredentialProvider> = Arc::new(FileCredentials {
        home: home.to_path_buf(),
    });
    tokio::spawn(async move {
        if let Err(error) = proxy.serve(listener, routes, credentials).await {
            eprintln!("recovered session proxy stopped: {error}");
        }
    });
}
