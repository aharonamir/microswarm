use reqwest::{Client, redirect::Policy};
use serde_json::{Value, json};
use std::time::Duration;

pub fn check(provider: &str, key: &str) -> Result<(), String> {
    let endpoint = match provider {
        "tavily" => "https://api.tavily.com/search",
        "brave" => "https://api.search.brave.com/res/v1/web/search",
        _ => return Err("unsupported web search provider".into()),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "could not start search connection check".to_owned())?;
    runtime.block_on(check_async(provider, key, endpoint))
}

async fn check_async(provider: &str, key: &str, endpoint: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "could not initialize search connection check".to_owned())?;
    let request = match provider {
        "tavily" => client
            .post(endpoint)
            .bearer_auth(key)
            .json(&json!({"query": "Wikipedia", "max_results": 1, "search_depth": "basic"})),
        "brave" => client
            .post(endpoint)
            .header("x-subscription-token", key)
            .json(&json!({"q": "Wikipedia", "count": 1})),
        _ => return Err("unsupported web search provider".into()),
    };
    let mut response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "search connection check timed out".to_owned()
        } else {
            "could not reach the search provider; check the network".to_owned()
        }
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "search provider rejected the check (HTTP {}); verify its API key and plan",
            response.status().as_u16()
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "search provider returned an unreadable response")?
    {
        if body.len() + chunk.len() > 128 * 1024 {
            return Err("search provider response was too large for a connection check".into());
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| "search provider returned an unreadable response".to_owned())?;
    let results = if provider == "brave" {
        value.pointer("/web/results")
    } else {
        value.get("results")
    };
    if results.is_some_and(Value::is_array) {
        Ok(())
    } else {
        Err("search provider returned an unexpected response".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn both_search_providers_verify_with_correct_auth() {
        for (provider, expected_header, response) in [
            (
                "tavily",
                "authorization: Bearer test-secret",
                r#"{"results":[]}"#,
            ),
            (
                "brave",
                "x-subscription-token: test-secret",
                r#"{"web":{"results":[]}}"#,
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0_u8; 4096];
                let size = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..size]).into_owned();
                let body = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                );
                stream.write_all(body.as_bytes()).await.unwrap();
                request
            });
            check_async(provider, "test-secret", &format!("http://{address}/search"))
                .await
                .unwrap();
            let request = server.await.unwrap();
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains(&expected_header.to_ascii_lowercase())
            );
        }
    }
}
