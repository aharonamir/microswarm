#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[test]
fn telegram_updates_create_channel_scoped_sessions_and_deliver_replies() {
    let Some(mut harness) = IntegrationHarness::start_or_skip() else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let (delivered, messages) = mpsc::channel();
    let server_stop = Arc::clone(&stop);
    let api_server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => serve_telegram_request(stream, &delivered),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("fake Telegram API failed: {error}"),
            }
        }
    });

    fs::write(
        harness.home.join("credentials/telegram-token"),
        "fixture-token\n",
    )
    .unwrap();
    let config = harness.home.join("config/microswarm.toml");
    let mut settings = fs::read_to_string(&config).unwrap();
    settings.push_str(&format!(
        "\n[[channels]]\nkind = \"telegram\"\ntoken = \"telegram-token\"\napi_base = \"http://{address}\"\n"
    ));
    fs::write(config, settings).unwrap();
    harness.restart_supervisor().unwrap();

    let body = messages
        .recv_timeout(Duration::from_secs(10))
        .expect("Telegram channel should deliver a reply");
    assert_eq!(body["chat_id"], 777);
    assert_eq!(body["text"], "fixture reply");
    let store =
        ms_store::SupervisorStore::open_readonly(harness.home.join("supervisor.db")).unwrap();
    let (channel, thread): (String, String) = store
        .connection()
        .query_row(
            "SELECT channel,channel_thread FROM sessions WHERE kind='lead' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(channel, "telegram");
    assert_eq!(thread, "777");
    let offset_path = harness.home.join("telegram-offset-telegram-token");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::read_to_string(&offset_path)
        .ok()
        .as_deref()
        .map(str::trim)
        != Some("42")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Telegram update offset was not persisted"
        );
        thread::sleep(Duration::from_millis(10));
    }
    harness.restart_supervisor().unwrap();
    assert!(
        messages.recv_timeout(Duration::from_millis(600)).is_err(),
        "restart must not replay the processed update"
    );

    harness.stop_supervisor_for_test();
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    api_server.join().unwrap();
}

fn serve_telegram_request(stream: TcpStream, delivered: &mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or_default() == 0 {
        return;
    }
    let mut content_length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or_default() == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; content_length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or_default();
    let response = if path.contains("/getUpdates") {
        if !path.contains("offset=42") {
            json!({"ok":true,"result":[{"update_id":41,"message":{"chat":{"id":777},"text":"hello from Telegram"}}]})
        } else {
            thread::sleep(Duration::from_millis(100));
            json!({"ok":true,"result":[]})
        }
    } else if path.contains("/sendMessage") {
        let payload: Value = serde_json::from_slice(&body).unwrap();
        let _ = delivered.send(payload);
        json!({"ok":true,"result":{"message_id":42}})
    } else {
        json!({"ok":false,"description":"unknown method"})
    };
    let encoded = response.to_string();
    let stream = reader.get_mut();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        encoded.len(),
        encoded
    );
    let _ = stream.flush();
}

#[test]
fn telegram_approval_delivers_the_final_reply() {
    let Some(mut harness) = IntegrationHarness::start_approval_test_or_skip() else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let code = Arc::new(Mutex::new(None::<String>));
    let (delivered, messages) = mpsc::channel();
    let server_stop = Arc::clone(&stop);
    let server_code = Arc::clone(&code);
    let api_server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => serve_approval_request(stream, &delivered, &server_code),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("fake Telegram API failed: {error}"),
            }
        }
    });
    fs::write(
        harness.home.join("credentials/telegram-token"),
        "fixture-token\n",
    )
    .unwrap();
    let config = harness.home.join("config/microswarm.toml");
    let mut settings = fs::read_to_string(&config).unwrap();
    settings.push_str(&format!("\n[[channels]]\nkind = \"telegram\"\ntoken = \"telegram-token\"\napi_base = \"http://{address}\"\n"));
    fs::write(config, settings).unwrap();
    harness.restart_supervisor().unwrap();
    let prompt = messages.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(prompt["text"].as_str().unwrap().contains("/approve "));
    let answer = messages.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(answer["text"], "fixture reply");
    assert!(harness.home.join("work/approval-marker").exists());
    harness.stop_supervisor_for_test();
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    api_server.join().unwrap();
}

fn serve_approval_request(
    stream: TcpStream,
    delivered: &mpsc::Sender<Value>,
    code: &Mutex<Option<String>>,
) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or_default() == 0 {
        return;
    }
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or_default() == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or_default();
    let response = if path.contains("/getUpdates") {
        if !path.contains("offset=") {
            json!({"ok":true,"result":[{"update_id":1,"message":{"chat":{"id":777},"text":"run approval tool"}}]})
        } else if path.contains("offset=2") {
            if let Some(code) = code.lock().unwrap().clone() {
                json!({"ok":true,"result":[{"update_id":2,"callback_query":{"data":format!("/approve {code}"),"message":{"chat":{"id":777}}}}]})
            } else {
                json!({"ok":true,"result":[]})
            }
        } else {
            json!({"ok":true,"result":[]})
        }
    } else if path.contains("/sendMessage") {
        let payload: Value = serde_json::from_slice(&body).unwrap();
        if let Some(text) = payload["text"].as_str()
            && let Some(value) = text
                .split("/approve ")
                .nth(1)
                .and_then(|tail| tail.split_whitespace().next())
        {
            *code.lock().unwrap() = Some(value.to_owned());
        }
        if payload["text"]
            .as_str()
            .is_some_and(|text| text.contains("/approve ") || text == "fixture reply")
        {
            let _ = delivered.send(payload);
        }
        json!({"ok":true,"result":{"message_id":42}})
    } else {
        json!({"ok":false})
    };
    let encoded = response.to_string();
    let stream = reader.get_mut();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        encoded.len(),
        encoded
    );
    let _ = stream.flush();
}
