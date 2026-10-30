#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    let updates = Arc::new(AtomicUsize::new(0));
    let (delivered, messages) = mpsc::channel();
    let server_stop = Arc::clone(&stop);
    let server_updates = Arc::clone(&updates);
    let api_server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => serve_telegram_request(stream, &server_updates, &delivered),
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

    harness.stop_supervisor_for_test();
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    api_server.join().unwrap();
}

fn serve_telegram_request(
    stream: TcpStream,
    updates: &AtomicUsize,
    delivered: &mpsc::Sender<Value>,
) {
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
        if updates.fetch_add(1, Ordering::SeqCst) == 0 {
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
