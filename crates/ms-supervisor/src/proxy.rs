use http::{HeaderValue, Method, Request, Response, StatusCode, Uri, header};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo},
};
use ms_proto::Scope;
use ms_store::{StoreError, SupervisorStore};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};
use tokio::net::TcpListener;

const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub trait CredentialProvider: Send + Sync + 'static {
    fn resolve(&self, name: &str) -> Option<String>;
}

pub trait ProxyEventObserver: Send + Sync + 'static {
    fn record(&self, session_id: &str, status: StatusCode);
}

#[derive(Clone)]
pub struct ProxyRoutes {
    pub model_upstream: Option<String>,
    pub model_credential: Option<String>,
    pub anthropic_model: bool,
    pub tools: BTreeMap<String, String>,
}

#[derive(Clone)]
pub struct SessionProxy {
    scope: Scope,
    observer: Option<Arc<dyn ProxyEventObserver>>,
}

impl SessionProxy {
    pub fn from_store(store: &SupervisorStore, session_id: &str) -> Result<Self, StoreError> {
        let (hash, value) = store
            .session_scope(session_id)?
            .ok_or(StoreError::InvalidContract(
                "session has no authoritative scope",
            ))?;
        let scope: Scope = serde_json::from_value(value)?;
        if scope.session_id != session_id || scope.hash != hash || scope.canonical_hash() != hash {
            return Err(StoreError::InvalidContract(
                "authoritative scope identity or hash is invalid",
            ));
        }
        Ok(Self {
            scope,
            observer: None,
        })
    }

    pub fn with_observer(mut self, observer: Arc<dyn ProxyEventObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn credential_allowed(&self, credential_name: Option<&str>) -> bool {
        credential_name.is_none_or(|name| self.scope.credentials.iter().any(|item| item == name))
    }

    pub async fn serve(
        self,
        listener: TcpListener,
        routes: ProxyRoutes,
        credentials: Arc<dyn CredentialProvider>,
    ) -> Result<(), String> {
        let connector: HttpsConnector<HttpConnector> = HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(rustls::crypto::ring::default_provider())
            .map_err(|error| format!("cannot initialize proxy TLS client: {error}"))?
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        loop {
            let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let proxy = self.clone();
            let routes = routes.clone();
            let credentials = Arc::clone(&credentials);
            let client = client.clone();
            tokio::spawn(async move {
                let service = service_fn(move |request| {
                    let proxy = proxy.clone();
                    let routes = routes.clone();
                    let credentials = Arc::clone(&credentials);
                    let client = client.clone();
                    async move {
                        Ok::<_, std::convert::Infallible>(
                            proxy
                                .handle(request, &routes, credentials.as_ref(), &client)
                                .await,
                        )
                    }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    }

    async fn handle(
        &self,
        request: Request<hyper::body::Incoming>,
        routes: &ProxyRoutes,
        credentials: &dyn CredentialProvider,
        client: &Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    ) -> Response<Full<Bytes>> {
        let result = self
            .handle_request(request, routes, credentials, client)
            .await;
        if let Some(observer) = &self.observer {
            observer.record(&self.scope.session_id, result.status());
        }
        result
    }

    async fn handle_request(
        &self,
        request: Request<hyper::body::Incoming>,
        routes: &ProxyRoutes,
        credentials: &dyn CredentialProvider,
        client: &Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    ) -> Response<Full<Bytes>> {
        let (parts, incoming) = request.into_parts();
        let body = match Limited::new(incoming, MAX_REQUEST_BYTES).collect().await {
            Ok(body) => body.to_bytes(),
            Err(error) => return response(StatusCode::BAD_REQUEST, error.to_string()),
        };
        let credential_name = parts
            .headers
            .get("x-ms-credential")
            .and_then(|v| v.to_str().ok());
        let path = parts.uri.path();
        let mut method = parts.method.clone();
        let mut outbound_body = body.clone();
        let (target, credential_name) = if path == "/fetch" {
            #[derive(Deserialize)]
            struct Fetch {
                url: String,
            }
            let fetch: Fetch = match serde_json::from_slice(&body) {
                Ok(value) => value,
                Err(error) => return response(StatusCode::BAD_REQUEST, error.to_string()),
            };
            method = Method::GET;
            outbound_body = Bytes::new();
            (fetch.url, credential_name)
        } else if let Some(server) = path.strip_prefix("/mcp/") {
            let Some(grant) = self.scope.tools.iter().find(|grant| grant.server == server) else {
                return response(StatusCode::FORBIDDEN, "MCP server is outside session scope");
            };
            let _ = grant;
            let Some(upstream) = routes.tools.get(server) else {
                return response(StatusCode::NOT_FOUND, "unknown MCP server");
            };
            (upstream.clone(), credential_name)
        } else {
            let (Some(base), Some(name)) = (&routes.model_upstream, &routes.model_credential)
            else {
                return response(StatusCode::NOT_FOUND, "model proxy route is not configured");
            };
            if credential_name != Some(name.as_str()) {
                return response(
                    StatusCode::FORBIDDEN,
                    "model credential is outside session scope",
                );
            }
            (append_path(base, &parts.uri), Some(name.as_str()))
        };

        if !self.credential_allowed(credential_name) {
            return response(StatusCode::FORBIDDEN, "credential is outside session scope");
        }
        let upstream: Uri = match target.parse() {
            Ok(uri) if valid_upstream(&uri) => uri,
            _ => {
                return response(
                    StatusCode::BAD_REQUEST,
                    "upstream URL must be HTTP(S) without user information",
                );
            }
        };
        let mut builder = Request::builder().method(method).uri(upstream);
        for (name, value) in &parts.headers {
            if name != header::HOST && name != "x-ms-credential" {
                builder = builder.header(name, value);
            }
        }
        if let Some(name) = credential_name {
            let secret = credentials.resolve(name);
            if let Some(secret) = secret {
                let header_name =
                    if routes.anthropic_model && path != "/fetch" && !path.starts_with("/mcp/") {
                        "x-api-key"
                    } else {
                        "authorization"
                    };
                let value = if header_name == "authorization" {
                    format!("Bearer {secret}")
                } else {
                    secret
                };
                let Ok(value) = HeaderValue::from_str(&value) else {
                    return response(
                        StatusCode::BAD_GATEWAY,
                        "credential cannot be represented as an HTTP header",
                    );
                };
                builder = builder.header(header_name, value);
            }
        }
        let upstream_request = match builder.body(Full::new(outbound_body)) {
            Ok(request) => request,
            Err(error) => return response(StatusCode::BAD_REQUEST, error.to_string()),
        };
        let upstream_response = match client.request(upstream_request).await {
            Ok(response) => response,
            Err(_) => return response(StatusCode::BAD_GATEWAY, "upstream request failed"),
        };
        let status = upstream_response.status();
        let headers = upstream_response.headers().clone();
        let bytes = match Limited::new(upstream_response.into_body(), MAX_RESPONSE_BYTES)
            .collect()
            .await
        {
            Ok(body) => body.to_bytes(),
            Err(_) => {
                return response(StatusCode::BAD_GATEWAY, "upstream response exceeded limits");
            }
        };
        let mut response_builder = Response::builder().status(status);
        if let Some(content_type) = headers.get(header::CONTENT_TYPE) {
            response_builder = response_builder.header(header::CONTENT_TYPE, content_type);
        }
        response_builder
            .body(Full::new(bytes))
            .unwrap_or_else(|_| response(StatusCode::BAD_GATEWAY, "invalid upstream response"))
    }
}

fn append_path(base: &str, request_uri: &Uri) -> String {
    let path = request_uri.path_and_query().map_or("/", |v| v.as_str());
    format!("{}{path}", base.trim_end_matches('/'))
}

fn valid_upstream(uri: &Uri) -> bool {
    matches!(uri.scheme_str(), Some("http" | "https"))
        && uri
            .authority()
            .is_some_and(|authority| !authority.as_str().contains('@'))
}

fn response(status: StatusCode, message: impl Into<String>) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(message.into())))
        .expect("static response headers are valid")
}
