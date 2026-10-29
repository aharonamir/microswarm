use ms_agent::model::{ModelClient, ModelProvider};
use ms_agent::{agent_loop::TurnProcessor, boot::AgentContext};
use ms_proto::{InboundKind, InboundMessage, Scope};
use ms_store::{SessionStore, open_session_dir};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

fn assert_model_request(provider: ModelProvider, expected_path: &str, response_body: &'static str) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let expected_path = expected_path.to_owned();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut request = BufReader::new(stream);
        let mut first_line = String::new();
        request.read_line(&mut first_line).unwrap();
        assert!(first_line.starts_with(&format!("POST {expected_path} HTTP/1.")));

        let mut content_length = 0;
        let mut has_credential_name = false;
        let mut line = String::new();
        loop {
            line.clear();
            request.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.trim().parse::<usize>().unwrap();
                }
                if name.eq_ignore_ascii_case("x-ms-credential") {
                    has_credential_name = value.trim() == "fixture-model-key";
                }
            }
        }
        assert!(
            has_credential_name,
            "proxy request must identify the scoped credential by name"
        );
        let mut body = vec![0; content_length];
        request.read_exact(&mut body).unwrap();
        let request_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request_json["model"], "fixture-model");
        assert_eq!(request_json["messages"][0]["content"], "hello");

        let stream = request.get_mut();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .unwrap();
        stream.flush().unwrap();
    });

    let client = ModelClient::new(
        format!("http://{address}/v1"),
        provider,
        "fixture-model",
        "fixture-model-key",
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reply = runtime.block_on(client.complete(&[], "hello")).unwrap();
    server.join().unwrap();
    assert_eq!(reply, "fixture reply");
}

#[test]
fn openai_compatible_request_uses_proxy_and_credential_name() {
    assert_model_request(
        ModelProvider::OpenAiCompatible,
        "/v1/chat/completions",
        r#"{"choices":[{"message":{"content":"fixture reply"}}]}"#,
    );
}

#[test]
fn anthropic_request_uses_provider_endpoint_and_parses_text_content() {
    assert_model_request(
        ModelProvider::Anthropic,
        "/v1/messages",
        r#"{"content":[{"type":"text","text":"fixture reply"}]}"#,
    );
}

#[test]
fn each_turn_returns_only_the_two_new_history_entries() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..4 {
            let (stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream);
            let mut line = String::new();
            request.read_line(&mut line).unwrap();
            let mut content_length = 0;
            loop {
                line.clear();
                request.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; content_length];
            request.read_exact(&mut body).unwrap();
            let stream = request.get_mut();
            let response = r#"{"choices":[{"message":{"content":"reply"}}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            stream.flush().unwrap();
        }
    });

    let dir = std::env::temp_dir().join(format!("ms-history-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let supervisor_store = open_session_dir(&dir).unwrap();
    let store = SessionStore::open_dir(&dir).unwrap();
    let scope: Scope = serde_json::from_value(json!({
        "version": 1, "session_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "derived_from": "config",
        "source_ref": "config", "mounts": [], "credentials": [], "tools": [],
        "delegation": null,
        "limits": {"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},
        "narrowed": [], "hash": "placeholder"
    }))
    .unwrap();
    let mut context = AgentContext {
        session_id: scope.session_id.clone(),
        root_session_id: scope.session_id.clone(),
        kind: "lead".into(),
        role: "lead".into(),
        proxy_url: Some(format!("http://{address}/v1")),
        scope,
        store,
    };
    let mut client = ModelClient::new(
        format!("http://{address}/v1"),
        ModelProvider::OpenAiCompatible,
        "m",
        "c",
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut history = Vec::new();
    for turn in 0..4 {
        let output = runtime
            .block_on(client.process(
                &mut context,
                InboundMessage {
                    id: Some(turn + 1),
                    kind: InboundKind::UserMessage,
                    body: json!({"text": format!("message {turn}")}),
                    created_at: None,
                },
                history.clone(),
            ))
            .unwrap();
        assert_eq!(
            output.history.len(),
            2,
            "turn {turn} should return only user + assistant additions"
        );
        context
            .store
            .commit_turn(turn + 1, &output.outbound, &output.history)
            .unwrap();
        history.extend(output.history);
    }
    server.join().unwrap();
    assert_eq!(history.len(), 8);
    assert_eq!(
        context.store.history().unwrap().len(),
        8,
        "four turns must persist exactly two new rows each"
    );
    drop(context);
    drop(supervisor_store);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn model_http_client_errors_are_configuration_failures() {
    for (status, reason) in [
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (403, "Forbidden"),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream);
            let mut line = String::new();
            request.read_line(&mut line).unwrap();
            loop {
                line.clear();
                request.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let stream = request.get_mut();
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            stream.flush().unwrap();
        });
        let dir =
            std::env::temp_dir().join(format!("ms-auth-status-{}-{status}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let supervisor_store = open_session_dir(&dir).unwrap();
        let store = SessionStore::open_dir(&dir).unwrap();
        let scope: Scope = serde_json::from_value(json!({
            "version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config",
            "source_ref":"test","mounts":[],"credentials":[],"tools":[],"delegation":null,
            "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},
            "narrowed":[],"hash":"placeholder"
        }))
        .unwrap();
        let mut context = AgentContext {
            session_id: scope.session_id.clone(),
            root_session_id: scope.session_id.clone(),
            kind: "lead".into(),
            role: "lead".into(),
            proxy_url: None,
            scope,
            store,
        };
        let mut client = ModelClient::new(
            format!("http://{address}/v1"),
            ModelProvider::OpenAiCompatible,
            "m",
            "c",
        )
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime
            .block_on(client.process(
                &mut context,
                InboundMessage {
                    id: Some(1),
                    kind: InboundKind::UserMessage,
                    body: json!({"text":"hello"}),
                    created_at: None,
                },
                vec![],
            ))
            .unwrap_err();
        assert_eq!(error.exit_code(), 78, "HTTP {status} must map to EX_CONFIG");
        server.join().unwrap();
        drop(context);
        drop(supervisor_store);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn model_error_body_cannot_fake_auth_status() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut request = BufReader::new(stream);
        let mut line = String::new();
        request.read_line(&mut line).unwrap();
        loop {
            line.clear();
            request.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        let body = "upstream detail contains HTTP 401 but actual status is 500";
        let stream = request.get_mut();
        write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        stream.flush().unwrap();
    });
    let dir = std::env::temp_dir().join(format!("ms-auth-spoof-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let _supervisor = open_session_dir(&dir).unwrap();
    let store = SessionStore::open_dir(&dir).unwrap();
    let scope: Scope = serde_json::from_value(json!({
        "version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config",
        "source_ref":"test","mounts":[],"credentials":[],"tools":[],"delegation":null,
        "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},
        "narrowed":[],"hash":"placeholder"
    }))
    .unwrap();
    let mut context = AgentContext {
        session_id: scope.session_id.clone(),
        root_session_id: scope.session_id.clone(),
        kind: "lead".into(),
        role: "lead".into(),
        proxy_url: None,
        scope,
        store,
    };
    let mut client = ModelClient::new(
        format!("http://{address}/v1"),
        ModelProvider::OpenAiCompatible,
        "m",
        "c",
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = runtime
        .block_on(client.process(
            &mut context,
            InboundMessage {
                id: Some(1),
                kind: InboundKind::UserMessage,
                body: json!({"text":"hello"}),
                created_at: None,
            },
            vec![],
        ))
        .unwrap_err();
    assert_eq!(
        error.exit_code(),
        75,
        "HTTP 500 must remain transient regardless of body text"
    );
    server.join().unwrap();
    drop(context);
    let _ = std::fs::remove_dir_all(dir);
}
