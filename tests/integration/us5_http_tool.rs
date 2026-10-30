use http::{Method, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use ms_agent::{
    mcp::{McpClient, McpServerDescriptor, McpTransport},
    permit::authorize_tool,
};
use ms_proto::{DerivedFrom, Limits, Permission, Scope, ToolGrant, ToolNames};
use ms_store::{SessionRecord, SupervisorStore};
use ms_supervisor::proxy::{CredentialProvider, ProxyRoutes, SessionProxy};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::net::TcpListener;

struct NoCredentials;

impl CredentialProvider for NoCredentials {
    fn resolve(&self, _: &str) -> Option<String> {
        None
    }
}

#[tokio::test]
async fn http_mcp_tool_reaches_upstream_only_through_session_proxy() {
    let home = std::env::temp_dir().join(format!("ms-us5-http-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let mut store = SupervisorStore::open(home.join("supervisor.db")).unwrap();
    let id = "01J00000000000000000000000";
    let mut scope = Scope {
        version: 1,
        session_id: id.into(),
        derived_from: DerivedFrom::Config,
        source_ref: "test".into(),
        mounts: vec![],
        credentials: vec![],
        tools: vec![ToolGrant {
            server: "weather".into(),
            tools: ToolNames::Selected(BTreeSet::from(["forecast".into()])),
            policy: Permission::Allow,
            overrides: BTreeMap::new(),
        }],
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
    let session = SessionRecord {
        id: id.into(),
        root_id: id.into(),
        parent_id: None,
        role: "lead".into(),
        kind: "lead".into(),
        channel: None,
        channel_thread: None,
        status: "ready".into(),
        scope_hash: Some(scope.hash.clone()),
        scope: Some(serde_json::to_value(&scope).unwrap()),
        created_at: "2026-10-30T00:00:00Z".into(),
        last_active_at: "2026-10-30T00:00:00Z".into(),
    };
    store.insert_session(&session).unwrap();
    let proxy = SessionProxy::from_store(&store, id).unwrap();

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_address = upstream.local_addr().unwrap();
    let upstream_seen = Arc::clone(&seen);
    let upstream_calls = Arc::clone(&calls);
    let upstream_task = tokio::spawn(async move {
        loop {
            let (socket, _) = upstream.accept().await.unwrap();
            let seen = Arc::clone(&upstream_seen);
            let calls = Arc::clone(&upstream_calls);
            tokio::spawn(async move {
                let service = hyper::service::service_fn(
                    move |request: http::Request<hyper::body::Incoming>| {
                        let seen = Arc::clone(&seen);
                        let calls = Arc::clone(&calls);
                        async move {
                            assert_eq!(request.method(), Method::POST);
                            seen.lock().unwrap().push(request.uri().path().to_owned());
                            let body = request.into_body().collect().await.unwrap().to_bytes();
                            let rpc: Value = serde_json::from_slice(&body).unwrap();
                            let id = rpc.get("id").cloned().unwrap_or(Value::Null);
                            let result = match rpc["method"].as_str().unwrap_or_default() {
                                "initialize" => {
                                    json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"weather","version":"1"}})
                                }
                                "notifications/initialized" => {
                                    return Ok::<_, std::convert::Infallible>(
                                        http::Response::builder()
                                            .status(StatusCode::ACCEPTED)
                                            .body(Full::new(Bytes::new()))
                                            .unwrap(),
                                    );
                                }
                                "tools/list" => {
                                    json!({"tools":[{"name":"forecast","description":"forecast","inputSchema":{"type":"object","properties":{}}}]})
                                }
                                "tools/call" => {
                                    calls.fetch_add(1, Ordering::SeqCst);
                                    json!({"content":[{"type":"text","text":"proxied forecast"}],"isError":false})
                                }
                                _ => json!({"error":{"code":-32601,"message":"unknown method"}}),
                            };
                            let body = json!({"jsonrpc":"2.0","id":id,"result":result});
                            Ok::<_, std::convert::Infallible>(
                                http::Response::builder()
                                    .header("content-type", "application/json")
                                    .body(Full::new(Bytes::from(body.to_string())))
                                    .unwrap(),
                            )
                        }
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(socket), service)
                    .await;
            });
        }
    });

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_address = listener.local_addr().unwrap();
    let proxy_task = tokio::spawn(proxy.serve(
        listener,
        ProxyRoutes {
            model_upstream: None,
            model_credential: None,
            anthropic_model: false,
            tools: BTreeMap::from([("weather".into(), format!("http://{upstream_address}/mcp"))]),
        },
        Arc::new(NoCredentials),
    ));
    let descriptor = McpServerDescriptor {
        id: "weather".into(),
        transport: McpTransport::StreamableHttp,
        command: None,
        args: vec![],
        url: Some("https://weather.example/mcp".into()),
        credential: None,
    };
    let proxy_url = format!("http://{proxy_address}");
    let mut client = McpClient::new(&scope, vec![descriptor], Some(&proxy_url)).unwrap();
    let tools = tokio::time::timeout(Duration::from_secs(5), client.list_tools("weather"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>(),
        ["forecast"]
    );
    let call = authorize_tool(&scope, "weather", "forecast", Some(Default::default())).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), client.call_tool(call))
        .await
        .unwrap()
        .unwrap();
    assert!(
        serde_json::to_string(&result.content)
            .unwrap()
            .contains("proxied forecast")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let paths = seen.lock().unwrap().clone();
    assert!(paths.len() >= 3);
    assert!(paths.iter().all(|path| path == "/mcp"));

    proxy_task.abort();
    upstream_task.abort();
    drop(store);
    fs::remove_dir_all(home).unwrap();
}
