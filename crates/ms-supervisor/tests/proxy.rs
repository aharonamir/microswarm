use http::{Request, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use ms_proto::{DerivedFrom, Limits, Scope};
use ms_store::{SessionRecord, SupervisorStore};
use ms_supervisor::proxy::SessionProxy;
use ms_supervisor::proxy::{CredentialProvider, ProxyRoutes};
use std::sync::Arc;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::net::TcpListener;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn session() -> SessionRecord {
    SessionRecord {
        id: "01J00000000000000000000000".into(),
        root_id: "01J00000000000000000000000".into(),
        parent_id: None,
        role: "lead".into(),
        kind: "lead".into(),
        channel: None,
        channel_thread: None,
        status: "ready".into(),
        scope_hash: None,
        scope: None,
        created_at: "2026-10-19T00:00:00Z".into(),
        last_active_at: "2026-10-19T00:00:00Z".into(),
    }
}

#[test]
fn proxy_uses_the_authoritative_scope_and_allows_only_named_credentials() {
    let dir = std::env::temp_dir().join(format!(
        "ms-proxy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    let mut record = session();
    let scope = Scope {
        version: 1,
        session_id: record.id.clone(),
        derived_from: DerivedFrom::Config,
        source_ref: "config".into(),
        mounts: vec![],
        credentials: vec!["model-key".into()],
        tools: vec![],
        delegation: None,
        limits: Limits {
            memory_mb: 256,
            pids: 64,
            tmp_mb: 128,
            max_turns: 30,
            max_tokens: 65_536,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    let mut scope = scope;
    scope.update_hash();
    record.scope_hash = Some(scope.hash.clone());
    record.scope = Some(serde_json::to_value(&scope).unwrap());
    store.insert_session(&record).unwrap();
    let proxy = SessionProxy::from_store(&store, &record.id).unwrap();
    assert!(proxy.credential_allowed(Some("model-key")));
    assert!(!proxy.credential_allowed(Some("other-key")));
    assert!(proxy.credential_allowed(None));
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

struct NoCredentials;

impl CredentialProvider for NoCredentials {
    fn resolve(&self, _name: &str) -> Option<String> {
        None
    }
}

#[tokio::test]
async fn per_session_listener_forwards_fetch_without_exposing_agent_credential_header() {
    let dir = std::env::temp_dir().join(format!(
        "ms-proxy-http-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    let mut record = session();
    let mut scope = Scope {
        version: 1,
        session_id: record.id.clone(),
        derived_from: DerivedFrom::Config,
        source_ref: "config".into(),
        mounts: vec![],
        credentials: vec![],
        tools: vec![],
        delegation: None,
        limits: Limits {
            memory_mb: 256,
            pids: 64,
            tmp_mb: 128,
            max_turns: 30,
            max_tokens: 65_536,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    scope.update_hash();
    record.scope_hash = Some(scope.hash.clone());
    record.scope = Some(serde_json::to_value(&scope).unwrap());
    store.insert_session(&record).unwrap();
    let proxy = SessionProxy::from_store(&store, &record.id).unwrap();
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let upstream_task = tokio::spawn(async move {
        let (socket, _) = upstream.accept().await.unwrap();
        hyper::server::conn::http1::Builder::new()
            .serve_connection(
                hyper_util::rt::TokioIo::new(socket),
                hyper::service::service_fn(
                    |request: http::Request<hyper::body::Incoming>| async move {
                        assert_eq!(request.method(), http::Method::GET);
                        Ok::<_, std::convert::Infallible>(http::Response::new(Full::new(
                            Bytes::from("fake response"),
                        )))
                    },
                ),
            )
            .await
            .unwrap();
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let proxy_task = tokio::spawn(proxy.serve(
        listener,
        ProxyRoutes {
            model_upstream: None,
            model_credential: None,
            anthropic_model: false,
            tools: Default::default(),
        },
        Arc::new(NoCredentials),
    ));
    let client: Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let request = Request::post(format!("http://{proxy_addr}/fetch"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(format!(
            "{{\"url\":\"http://{upstream_addr}/content\"}}"
        ))))
        .unwrap();
    let response = client.request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, "fake response");
    proxy_task.abort();
    upstream_task.await.unwrap();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn configured_but_unavailable_credential_is_rejected_before_upstream() {
    let dir = std::env::temp_dir().join(format!(
        "ms-proxy-missing-credential-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut store = SupervisorStore::open(dir.join("supervisor.db")).unwrap();
    let mut record = session();
    let mut scope = Scope {
        version: 1,
        session_id: record.id.clone(),
        derived_from: DerivedFrom::Config,
        source_ref: "config".into(),
        mounts: vec![],
        credentials: vec!["upstream-key".into()],
        tools: vec![],
        delegation: None,
        limits: Limits {
            memory_mb: 256,
            pids: 64,
            tmp_mb: 128,
            max_turns: 30,
            max_tokens: 65_536,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    scope.update_hash();
    record.scope_hash = Some(scope.hash.clone());
    record.scope = Some(serde_json::to_value(&scope).unwrap());
    store.insert_session(&record).unwrap();
    let proxy = SessionProxy::from_store(&store, &record.id).unwrap();

    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let upstream_task = tokio::spawn(async move {
        let accepted =
            tokio::time::timeout(std::time::Duration::from_millis(300), upstream.accept()).await;
        if let Ok(Ok((socket, _))) = accepted {
            hyper::server::conn::http1::Builder::new()
                .serve_connection(
                    hyper_util::rt::TokioIo::new(socket),
                    hyper::service::service_fn(|_| async {
                        Ok::<_, std::convert::Infallible>(http::Response::new(Full::new(
                            Bytes::new(),
                        )))
                    }),
                )
                .await
                .unwrap();
            true
        } else {
            false
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let proxy_task = tokio::spawn(proxy.serve(
        listener,
        ProxyRoutes {
            model_upstream: None,
            model_credential: None,
            anthropic_model: false,
            tools: Default::default(),
        },
        Arc::new(NoCredentials),
    ));
    let client: Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let request = Request::post(format!("http://{proxy_addr}/fetch"))
        .header("content-type", "application/json")
        .header("x-ms-credential", "upstream-key")
        .body(Full::new(Bytes::from(format!(
            "{{\"url\":\"http://{upstream_addr}/content\"}}"
        ))))
        .unwrap();
    let response = client.request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert!(
        !upstream_task.await.unwrap(),
        "proxy contacted upstream without its configured credential"
    );
    proxy_task.abort();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
