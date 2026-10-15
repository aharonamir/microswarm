use http::{HeaderName, HeaderValue};
use ms_proto::{Scope, ToolGrant};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, JsonObject, Tool},
    service::{RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt,
    process::Stdio,
};
use tokio::process::Command;

type ClientService = RunningService<RoleClient, ()>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    StreamableHttp,
}

/// Non-secret runtime metadata copied from host `tools.toml` by the supervisor.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpServerDescriptor {
    pub id: String,
    pub transport: McpTransport,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Host-side upstream URL. HTTP requests still go only to the session proxy.
    #[serde(default)]
    pub url: Option<String>,
    /// Credential selector, never a credential value.
    #[serde(default)]
    pub credential: Option<String>,
}

#[derive(Debug)]
pub struct McpError(String);

impl fmt::Display for McpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for McpError {}

pub struct McpClient {
    scope: Scope,
    servers: BTreeMap<String, McpServerDescriptor>,
    proxy_url: Option<String>,
    connections: HashMap<String, ClientService>,
}

impl McpClient {
    pub fn new(
        scope: &Scope,
        descriptors: Vec<McpServerDescriptor>,
        proxy_url: Option<&str>,
    ) -> Result<Self, McpError> {
        let grants = scope
            .tools
            .iter()
            .map(|grant| (grant.server.as_str(), grant))
            .collect::<HashMap<_, _>>();
        let mut servers = BTreeMap::new();
        for descriptor in descriptors {
            if !valid_server_id(&descriptor.id) {
                return Err(McpError(format!(
                    "invalid MCP server id {:?}",
                    descriptor.id
                )));
            }
            let Some(_grant) = grants.get(descriptor.id.as_str()) else {
                continue;
            };
            match descriptor.transport {
                McpTransport::Stdio if descriptor.command.as_deref().is_none_or(str::is_empty) => {
                    return Err(McpError(format!(
                        "stdio MCP server {:?} has no command",
                        descriptor.id
                    )));
                }
                McpTransport::StreamableHttp => {
                    let upstream = descriptor.url.as_deref().ok_or_else(|| {
                        McpError(format!(
                            "HTTP MCP server {:?} has no upstream URL",
                            descriptor.id
                        ))
                    })?;
                    let upstream_uri = upstream
                        .parse::<http::Uri>()
                        .map_err(|error| McpError(format!("invalid MCP upstream URL: {error}")))?;
                    if upstream_uri.scheme_str() != Some("https")
                        || upstream_uri.authority().is_none()
                        || upstream_uri
                            .authority()
                            .is_some_and(|authority| authority.as_str().contains('@'))
                    {
                        return Err(McpError(format!(
                            "HTTP MCP server {:?} upstream URL must be HTTPS and contain no user information",
                            descriptor.id
                        )));
                    }
                    let proxy = proxy_url
                        .ok_or_else(|| McpError("HTTP MCP server requires MS_PROXY_URL".into()))?;
                    let proxy_uri = proxy
                        .parse::<http::Uri>()
                        .map_err(|error| McpError(format!("invalid MS_PROXY_URL: {error}")))?;
                    if proxy_uri.scheme_str() != Some("http") || proxy_uri.authority().is_none() {
                        return Err(McpError(
                            "MS_PROXY_URL must be an http URL with an authority".into(),
                        ));
                    }
                    if let Some(name) = descriptor.credential.as_deref()
                        && !scope.credentials.iter().any(|granted| granted == name)
                    {
                        return Err(McpError(format!(
                            "MCP credential name {name:?} is outside the session scope"
                        )));
                    }
                }
                _ => {}
            }
            if servers.insert(descriptor.id.clone(), descriptor).is_some() {
                return Err(McpError("duplicate MCP server id".into()));
            }
        }
        Ok(Self {
            scope: scope.clone(),
            servers,
            proxy_url: proxy_url.map(str::to_owned),
            connections: HashMap::new(),
        })
    }

    pub fn from_env(scope: &Scope) -> Result<Self, McpError> {
        let descriptors = match std::env::var("MS_MCP_SERVERS") {
            Ok(value) => serde_json::from_str(&value)
                .map_err(|error| McpError(format!("invalid MS_MCP_SERVERS: {error}")))?,
            Err(std::env::VarError::NotPresent) => Vec::new(),
            Err(error) => return Err(McpError(format!("cannot read MS_MCP_SERVERS: {error}"))),
        };
        Self::new(
            scope,
            descriptors,
            std::env::var("MS_PROXY_URL").ok().as_deref(),
        )
    }

    pub fn server_ids(&self) -> impl Iterator<Item = &str> {
        self.servers.keys().map(String::as_str)
    }

    pub fn filter_tool_names<'a>(
        &self,
        server: &str,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<Vec<String>, McpError> {
        let grant = self.grant(server)?;
        Ok(names
            .into_iter()
            .filter(|name| grant.tools.contains(name))
            .map(str::to_owned)
            .collect())
    }

    pub async fn list_tools(&mut self, server: &str) -> Result<Vec<Tool>, McpError> {
        self.grant(server)?;
        self.connect(server).await?;
        let tools = self.connections[server]
            .peer()
            .list_all_tools()
            .await
            .map_err(|error| McpError(format!("MCP tools/list failed: {error}")))?;
        let grant = self.grant(server)?;
        Ok(tools
            .into_iter()
            .filter(|tool| grant.tools.contains(tool.name.as_ref()))
            .collect())
    }

    pub async fn call_tool(
        &mut self,
        server: &str,
        tool: &str,
        arguments: Option<JsonObject>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.grant(server)?.tools.contains(tool) {
            return Err(McpError(format!(
                "MCP tool {server}/{tool} is outside the session scope"
            )));
        }
        self.connect(server).await?;
        let mut params = CallToolRequestParams::new(tool.to_owned());
        params.arguments = arguments;
        self.connections[server]
            .peer()
            .call_tool(params)
            .await
            .map_err(|error| McpError(format!("MCP tools/call failed: {error}")))
    }

    fn grant(&self, server: &str) -> Result<&ToolGrant, McpError> {
        if !self.servers.contains_key(server) {
            return Err(McpError(format!(
                "MCP server {server:?} is not configured and granted"
            )));
        }
        self.scope
            .tools
            .iter()
            .find(|grant| grant.server == server)
            .ok_or_else(|| {
                McpError(format!(
                    "MCP server {server:?} is outside the session scope"
                ))
            })
    }

    async fn connect(&mut self, id: &str) -> Result<(), McpError> {
        if self.connections.contains_key(id) {
            return Ok(());
        }
        let descriptor = self
            .servers
            .get(id)
            .ok_or_else(|| McpError(format!("MCP server {id:?} is not configured and granted")))?;
        let client = match descriptor.transport {
            McpTransport::Stdio => {
                let mut command =
                    Command::new(descriptor.command.as_deref().expect("validated command"));
                command
                    .args(&descriptor.args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit());
                let transport = TokioChildProcess::new(command).map_err(|error| {
                    McpError(format!("cannot launch MCP server {id:?}: {error}"))
                })?;
                ().serve(transport).await.map_err(|error| {
                    McpError(format!("cannot initialize MCP server {id:?}: {error}"))
                })?
            }
            McpTransport::StreamableHttp => {
                let proxy = self
                    .proxy_url
                    .as_deref()
                    .expect("validated proxy URL")
                    .trim_end_matches('/');
                let uri = format!("{proxy}/mcp/{id}");
                let mut headers = HashMap::new();
                if let Some(name) = descriptor.credential.as_deref() {
                    let value = HeaderValue::from_str(name).map_err(|error| {
                        McpError(format!("invalid MCP credential name: {error}"))
                    })?;
                    headers.insert(HeaderName::from_static("x-ms-credential"), value);
                }
                let config =
                    StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers);
                let transport = StreamableHttpClientTransport::from_config(config);
                ().serve(transport).await.map_err(|error| {
                    McpError(format!("cannot initialize MCP server {id:?}: {error}"))
                })?
            }
        };
        self.connections.insert(id.to_owned(), client);
        Ok(())
    }
}

fn valid_server_id(id: &str) -> bool {
    let Some(first) = id.as_bytes().first() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
}
