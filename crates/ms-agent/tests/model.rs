use ms_agent::model::{ModelClient, ModelProvider};
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
