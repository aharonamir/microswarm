use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use ms_channels::{ChannelAdapter, telegram::TelegramAdapter};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

#[test]
fn telegram_updates_keep_chat_threads_and_approval_buttons_use_codes() {
    let adapter = TelegramAdapter::new("test-token");
    let inbound = adapter
        .parse_inbound(&json!({
            "update_id": 91,
            "message": {"message_id": 7, "chat": {"id": 1234}, "text": "hello"}
        }))
        .unwrap()
        .unwrap();
    assert_eq!(inbound.thread_id, "1234");
    assert_eq!(inbound.message.body["text"], "hello");

    let payload = adapter.send_message_payload(
        "1234",
        "Approve weather/forecast? Reply: /approve A1B2 or /deny A1B2.",
    );
    assert_eq!(payload["chat_id"], 1234);
    assert_eq!(
        payload["reply_markup"]["inline_keyboard"][0][0]["callback_data"],
        "/approve A1B2"
    );
    assert_eq!(
        payload["reply_markup"]["inline_keyboard"][0][1]["callback_data"],
        "/deny A1B2"
    );
}

#[tokio::test]
async fn telegram_transport_polls_and_posts_through_bot_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let server_seen = Arc::clone(&seen);
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let service = hyper::service::service_fn(
            move |request: http::Request<hyper::body::Incoming>| {
                let seen = Arc::clone(&server_seen);
                async move {
                    let path = request.uri().to_string();
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    seen.lock()
                        .unwrap()
                        .push((path.clone(), String::from_utf8_lossy(&body).into_owned()));
                    let response = if path.contains("/getUpdates") {
                        json!({"ok":true,"result":[{"update_id":8,"message":{"chat":{"id":321},"text":"hello"}}]})
                    } else {
                        json!({"ok":true,"result":{"message_id":9}})
                    };
                    Ok::<_, std::convert::Infallible>(
                        http::Response::builder()
                            .header("content-type", "application/json")
                            .body(Full::new(Bytes::from(response.to_string())))
                            .unwrap(),
                    )
                }
            },
        );
        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(socket), service)
            .await
            .unwrap();
    });

    let adapter = TelegramAdapter::new("fixture-token").with_api_base(format!("http://{address}"));
    let updates = tokio::time::timeout(Duration::from_secs(3), adapter.get_updates(Some(9)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updates[0]["update_id"], 8);
    adapter.send_message("321", "hello back").await.unwrap();
    drop(adapter);
    server.await.unwrap();
    let requests = seen.lock().unwrap();
    assert!(requests[0].0.contains("/botfixture-token/getUpdates"));
    assert!(requests[0].0.contains("offset=9"));
    assert!(requests[1].0.contains("/botfixture-token/sendMessage"));
    assert!(requests[1].1.contains("hello back"));
}
