use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde_json::{Value, json};
use std::time::Duration;

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_CHECK_RESPONSE_BYTES: usize = 64 * 1024;

pub fn check(provider: &str, base_url: &str, model: &str, key: &str) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "could not start model connection check".to_owned())?;
    runtime.block_on(check_async(provider, base_url, model, key))
}

async fn check_async(provider: &str, base_url: &str, model: &str, key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("model connection check requires a non-empty API key".into());
    }
    let provider = match provider {
        "openai-compatible" => Provider::OpenAiCompatible,
        "anthropic" => Provider::Anthropic,
        _ => return Err("unsupported model provider".into()),
    };
    let endpoint = provider.endpoint(base_url)?;
    let client = Client::builder()
        .timeout(CHECK_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .map_err(|_| "could not initialize model connection check".to_owned())?;
    let mut request = client.post(endpoint).json(&provider.request(model));
    request = match provider {
        Provider::OpenAiCompatible => request.bearer_auth(key),
        Provider::Anthropic => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
    };
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "model connection check timed out".to_owned()
        } else {
            "could not reach the model provider; check the base URL and network".to_owned()
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(status_message(status));
    }
    let mut response = response;
    let mut response_body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "model provider returned an unreadable response".to_owned())?
    {
        if response_body.len() + chunk.len() > MAX_CHECK_RESPONSE_BYTES {
            return Err("model provider response was too large for a connection check".into());
        }
        response_body.extend_from_slice(&chunk);
    }
    let body: Value = serde_json::from_slice(&response_body)
        .map_err(|_| "model provider returned an unreadable response".to_owned())?;
    let has_text = match provider {
        Provider::OpenAiCompatible => body
            .pointer("/choices/0/message/content")
            .is_some_and(|content| content.is_string() || content.is_array()),
        Provider::Anthropic => body
            .pointer("/content/0/text")
            .is_some_and(Value::is_string),
    };
    if has_text {
        Ok(())
    } else {
        Err(
            "model provider returned an unexpected response; check provider and model settings"
                .into(),
        )
    }
}

#[derive(Clone, Copy)]
enum Provider {
    OpenAiCompatible,
    Anthropic,
}

impl Provider {
    fn endpoint(self, base_url: &str) -> Result<Url, String> {
        let mut url = Url::parse(base_url.trim_end_matches('/'))
            .map_err(|_| "model base URL is invalid".to_owned())?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "model base URL must be HTTP(S) without user information or a fragment".into(),
            );
        }
        let base_path = url.path().trim_end_matches('/');
        let path = match (self, base_path.ends_with("/v1")) {
            (Self::OpenAiCompatible, true) => format!("{base_path}/chat/completions"),
            (Self::Anthropic, true) => format!("{base_path}/messages"),
            (Self::OpenAiCompatible, false) => format!("{base_path}/v1/chat/completions"),
            (Self::Anthropic, false) => format!("{base_path}/v1/messages"),
        };
        url.set_path(&path);
        Ok(url)
    }

    fn request(self, model: &str) -> Value {
        match self {
            Self::OpenAiCompatible => json!({
                "model": model,
                "messages": [{"role":"user","content":"Reply with OK."}],
                "max_tokens": 1
            }),
            Self::Anthropic => json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{"role":"user","content":"Reply with OK."}]
            }),
        }
    }
}

fn status_message(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            format!(
                "model provider rejected the API key (HTTP {})",
                status.as_u16()
            )
        }
        StatusCode::NOT_FOUND => "model endpoint or model name was not found (HTTP 404)".into(),
        StatusCode::TOO_MANY_REQUESTS => "model provider rate limited the check (HTTP 429)".into(),
        StatusCode::BAD_REQUEST => {
            "model provider rejected the request (HTTP 400); check the provider and model name"
                .into()
        }
        _ => format!("model provider returned HTTP {}", status.as_u16()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn mock_response(status: &str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let _ = stream.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{address}/v1")
    }

    #[tokio::test]
    async fn openai_compatible_check_sends_one_token_request() {
        let base_url =
            mock_response("200 OK", r#"{"choices":[{"message":{"content":"O"}}]}"#).await;
        assert!(
            check_async("openai-compatible", &base_url, "test-model", "secret")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn failed_check_never_returns_provider_body_or_key() {
        let base_url = mock_response(
            "401 Unauthorized",
            r#"{"error":"secret-token must not leak"}"#,
        )
        .await;
        let error = check_async("openai-compatible", &base_url, "test-model", "secret-token")
            .await
            .unwrap_err();
        assert!(error.contains("rejected the API key"));
        assert!(!error.contains("secret-token"));
    }

    #[test]
    fn provider_endpoints_follow_the_agent_model_client_paths() {
        let openai = Provider::OpenAiCompatible
            .endpoint("https://model.example/api/v1/")
            .unwrap();
        let anthropic = Provider::Anthropic
            .endpoint("https://model.example")
            .unwrap();
        assert_eq!(openai.path(), "/api/v1/chat/completions");
        assert_eq!(anthropic.path(), "/v1/messages");
    }
}
