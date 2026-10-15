use ms_agent::mcp::{McpClient, McpServerDescriptor, McpTransport};
use ms_proto::{Permission, Scope, ToolGrant, ToolNames};
use std::collections::{BTreeMap, BTreeSet};

const FAKE_STDIO_MCP: &str = r#"
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id"
      ;;
    *'"method":"tools/list"'*)
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"forecast","description":"forecast","inputSchema":{"type":"object","properties":{}}},{"name":"admin","description":"admin","inputSchema":{"type":"object","properties":{}}}]}}\n' "$id"
      ;;
    *'"method":"tools/call"'*)
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"forecast result"}],"isError":false}}\n' "$id"
      ;;
  esac
done
"#;

fn scope() -> Scope {
    let mut scope = Scope {
        version: 1,
        session_id: "01J00000000000000000000000".into(),
        derived_from: ms_proto::DerivedFrom::Config,
        source_ref: "test".into(),
        mounts: vec![],
        credentials: vec!["weather-api".into()],
        tools: vec![ToolGrant {
            server: "weather".into(),
            tools: ToolNames::Selected(BTreeSet::from(["forecast".into()])),
            policy: Permission::Allow,
            overrides: BTreeMap::new(),
        }],
        delegation: None,
        limits: ms_proto::Limits {
            memory_mb: 256,
            pids: 32,
            tmp_mb: 64,
            max_turns: 8,
            max_tokens: 4096,
        },
        narrowed: vec![],
        hash: String::new(),
    };
    scope.update_hash();
    scope
}

#[test]
fn adapter_ignores_ungranted_server_and_filters_tools() {
    let scope = scope();
    let servers = vec![
        McpServerDescriptor {
            id: "weather".into(),
            transport: McpTransport::Stdio,
            command: Some("fake-weather".into()),
            args: vec![],
            url: None,
            credential: None,
        },
        McpServerDescriptor {
            id: "ungranted".into(),
            transport: McpTransport::Stdio,
            command: Some("must-not-launch".into()),
            args: vec![],
            url: None,
            credential: None,
        },
    ];
    let client = McpClient::new(&scope, servers, None).expect("valid descriptors");
    assert_eq!(client.server_ids().collect::<Vec<_>>(), ["weather"]);
    assert_eq!(
        client
            .filter_tool_names("weather", ["forecast", "admin"])
            .unwrap(),
        ["forecast"]
    );
    assert!(client.filter_tool_names("ungranted", ["admin"]).is_err());
}

#[test]
fn http_server_requires_proxy_and_scoped_credential_name() {
    let scope = scope();
    let server = McpServerDescriptor {
        id: "weather".into(),
        transport: McpTransport::StreamableHttp,
        command: None,
        args: vec![],
        url: Some("https://weather.internal/mcp".into()),
        credential: Some("weather-api".into()),
    };
    assert!(McpClient::new(&scope, vec![server.clone()], None).is_err());
    let mut out_of_scope = server;
    out_of_scope.credential = Some("other-secret".into());
    assert!(McpClient::new(&scope, vec![out_of_scope], Some("http://proxy:8080")).is_err());
    assert!(
        McpClient::new(
            &scope,
            vec![McpServerDescriptor {
                id: "weather".into(),
                transport: McpTransport::StreamableHttp,
                command: None,
                args: vec![],
                url: Some("https://weather.internal/mcp".into()),
                credential: Some("weather-api".into()),
            }],
            Some("http://proxy:8080")
        )
        .is_ok()
    );
}

#[tokio::test]
async fn denied_tool_call_does_not_launch_server() {
    let scope = scope();
    let mut client = McpClient::new(
        &scope,
        vec![McpServerDescriptor {
            id: "weather".into(),
            transport: McpTransport::Stdio,
            command: Some("this-command-must-not-run".into()),
            args: vec![],
            url: None,
            credential: None,
        }],
        None,
    )
    .expect("valid descriptor");
    assert!(client.call_tool("weather", "admin", None).await.is_err());
}

#[tokio::test]
async fn stdio_lists_only_granted_tools_and_calls_allowed_tool() {
    let scope = scope();
    let mut client = McpClient::new(
        &scope,
        vec![McpServerDescriptor {
            id: "weather".into(),
            transport: McpTransport::Stdio,
            command: Some("/bin/sh".into()),
            args: vec!["-c".into(), FAKE_STDIO_MCP.into()],
            url: None,
            credential: None,
        }],
        None,
    )
    .expect("valid descriptor");

    let tools = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.list_tools("weather"),
    )
    .await
    .expect("list tools timed out")
    .expect("tools/list succeeds");
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>(),
        ["forecast"]
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.call_tool("weather", "forecast", None),
    )
    .await
    .expect("tool call timed out")
    .expect("granted tools/call succeeds");
    assert!(
        serde_json::to_string(&result.content)
            .expect("result serializes")
            .contains("forecast result")
    );
}
