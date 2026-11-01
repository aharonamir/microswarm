#[path = "harness.rs"]
mod harness;

use harness::IntegrationHarness;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
fn console_lists_real_session_and_decides_approval_through_supervisor() {
    let Some(harness) = IntegrationHarness::start_approval_test_or_skip() else {
        return;
    };
    let mut chat = harness.start_chat(None).expect("start fake-model chat");
    chat.send_line("run approval tool")
        .expect("send tool request");
    chat.close_input();
    chat.wait_for("/approve ", Duration::from_secs(10))
        .expect("fake-model chat should wait for approval");

    let session = harness
        .session_dirs()
        .expect("list test sessions")
        .remove(0);
    let session_id = session.file_name().unwrap().to_string_lossy().into_owned();
    let (address, port_guard) = loopback_address();
    drop(port_guard);
    let mut console = ConsoleProcess(start_console(&harness.home, address));
    let token = console_token(&mut console.0);
    let host = format!("127.0.0.1:{address}");

    let login = http_request(
        address,
        &host,
        "GET",
        &format!("/login?token={token}"),
        None,
        None,
    );
    assert!(login.starts_with("HTTP/1.1 303"), "{login}");
    let cookie = header(&login, "set-cookie")
        .expect("login response has cookie")
        .split(';')
        .next()
        .unwrap()
        .to_owned();

    let sessions = http_request(
        address,
        &host,
        "GET",
        "/api/v1/sessions",
        Some(&cookie),
        None,
    );
    let sessions_json: serde_json::Value = response_json(&sessions);
    assert!(
        sessions_json["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == session_id),
        "console session list omitted real supervisor session: {sessions}"
    );

    let approvals = http_request(
        address,
        &host,
        "GET",
        "/api/v1/approvals",
        Some(&cookie),
        None,
    );
    let approvals_json: serde_json::Value = response_json(&approvals);
    let code = approvals_json["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["state"] == "pending")
        .and_then(|item| item["code"].as_str())
        .expect("console lists pending supervisor approval");
    let decision_path = format!("/api/v1/approvals/{code}/approve");
    let decision = http_request(
        address,
        &host,
        "POST",
        &decision_path,
        Some(&cookie),
        Some("{}"),
    );
    let decision_json: serde_json::Value = response_json(&decision);
    assert!(decision.starts_with("HTTP/1.1 200"), "{decision}");
    assert_eq!(decision_json["decision"], "approved");

    assert!(
        chat.wait_for("fixture reply", Duration::from_secs(10))
            .expect("real supervisor resumes agent after console approval")
            .contains("fixture reply")
    );
    assert!(chat.wait().expect("chat exits after approval").success());
}

struct ConsoleProcess(Child);

impl Drop for ConsoleProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn loopback_address() -> (u16, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    let port = listener.local_addr().unwrap().port();
    (port, listener)
}

fn start_console(home: &std::path::Path, port: u16) -> Child {
    let current = std::env::current_exe().expect("integration test executable path");
    let binary = current
        .parent()
        .and_then(|deps| deps.parent())
        .expect("target profile directory")
        .join(if cfg!(windows) {
            "microswarm.exe"
        } else {
            "microswarm"
        });
    assert!(binary.is_file(), "CLI binary missing: {}", binary.display());
    Command::new(binary)
        .args(["console", "--listen", &format!("127.0.0.1:{port}")])
        .env("MS_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start console CLI")
}

fn console_token(child: &mut Child) -> String {
    let stderr = child.stderr.take().expect("console stderr piped");
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stderr).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    let line = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("console prints startup link")
        .expect("read console startup link");
    line.split("token=")
        .nth(1)
        .and_then(|token| token.split_whitespace().next())
        .map(str::to_owned)
        .expect("console startup link contains a token")
}

fn http_request(
    port: u16,
    host: &str,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<&str>,
) -> String {
    let body = body.unwrap_or("");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to console");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n"
    )
    .unwrap();
    if let Some(cookie) = cookie {
        write!(stream, "Cookie: {cookie}\r\n").unwrap();
    }
    if method == "POST" {
        write!(
            stream,
            "Origin: http://{host}\r\nX-MS-Console: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        )
        .unwrap();
    }
    write!(stream, "\r\n{body}").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    response.split("\r\n\r\n").next()?.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

fn response_json<T: serde::de::DeserializeOwned>(response: &str) -> T {
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).expect("decode response JSON")
}
