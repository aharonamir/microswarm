#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RESPONSE: &str = r#"{"id":"chatcmpl-fixture","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"fixture reply"},"finish_reason":"stop"}]}"#;
const TOOL_RESPONSE: &str = r#"{"id":"chatcmpl-tool","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"fixture-call","type":"function","function":{"name":"first-party/fs.write","arguments":"{\"path\":\"/work/approval-marker\",\"content\":\"approved\"}"}}]},"finish_reason":"tool_calls"}]}"#;

pub struct FakeModelServer {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeModelServer {
    pub fn start() -> std::io::Result<Self> {
        Self::start_with_approval_tool(false)
    }

    pub fn start_with_approval_tool(approval_tool: bool) -> std::io::Result<Self> {
        Self::start_with_options(approval_tool, false)
    }

    pub fn start_for_swarm() -> std::io::Result<Self> {
        Self::start_with_options(false, true)
    }

    fn start_with_options(approval_tool: bool, delegation_tools: bool) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = respond(stream, approval_tool, delegation_tools);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            shutdown,
            thread: Some(thread),
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.address)
    }
}

impl Drop for FakeModelServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn respond(stream: TcpStream, approval_tool: bool, delegation_tools: bool) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut content_length = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut request_body = vec![0; content_length];
    reader.read_exact(&mut request_body)?;
    let request_body = String::from_utf8_lossy(&request_body);
    let body = if delegation_tools {
        delegation_response(&request_body)
    } else if approval_tool
        && request_body.contains("run approval tool")
        && !request_body.contains("\"role\":\"tool\"")
    {
        TOOL_RESPONSE.to_owned()
    } else {
        RESPONSE.to_owned()
    };

    let stream = reader.get_mut();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()
}

fn delegation_response(body: &str) -> String {
    let request: serde_json::Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return RESPONSE.into(),
    };
    let messages = request["messages"].as_array().cloned().unwrap_or_default();
    let user_text = messages
        .iter()
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let names = request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            tool.pointer("/function/name")
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    if !names
        .iter()
        .any(|name| name.starts_with("microswarm-core/"))
    {
        return RESPONSE.into();
    }
    let count = |name: &str| {
        messages
            .iter()
            .flat_map(|message| message["tool_calls"].as_array().into_iter().flatten())
            .filter(|call| {
                call.pointer("/function/name")
                    .and_then(serde_json::Value::as_str)
                    == Some(name)
            })
            .count()
    };
    let member_id = messages
        .iter()
        .filter(|message| message["role"] == "tool")
        .filter_map(|message| message["content"].as_str())
        .filter_map(|content| serde_json::from_str::<serde_json::Value>(content).ok())
        .find_map(|result| result["member_id"].as_str().map(str::to_owned));

    if user_text.contains("forbidden") && count("microswarm-core/spawn_member") == 0 {
        return spawn_call("forbidden", "try a forbidden role", serde_json::json!({}));
    }
    if user_text.contains("tester") && count("microswarm-core/spawn_member") == 0 {
        return spawn_call(
            "tester",
            "run cargo test for src/auth",
            serde_json::json!({
                "mounts":[{"path":"/work","mode":"ro"},{"path":"/work/target","mode":"rw"}],
                "tools":[{"server":"microswarm-tools","tools":["shell.run","fs.read"]}]
            }),
        );
    }
    if user_text.contains("researcher") && count("microswarm-core/spawn_member") < 2 {
        let path = if user_text.contains("escape-link") {
            "/work/escape-link"
        } else if user_text.contains("/outside") {
            "/outside"
        } else {
            "/work"
        };
        if count("microswarm-core/spawn_member") == 0
            || (user_text.contains("again") && count("microswarm-core/dismiss_member") > 0)
        {
            return spawn_call(
                "researcher",
                "review the auth module",
                serde_json::json!({"mounts":[{"path":path,"mode":"ro"}]}),
            );
        }
    }
    if user_text.contains("send")
        && count("microswarm-core/send_to_member") == 0
        && let Some(member_id) = &member_id
    {
        return tool_call(
            "send_to_member",
            serde_json::json!({"member_id":member_id,"text":"run the auth test suite"}),
        );
    }
    if user_text.contains("dismiss")
        && count("microswarm-core/dismiss_member") == 0
        && let Some(member_id) = &member_id
    {
        return tool_call("dismiss_member", serde_json::json!({"member_id":member_id}));
    }
    if messages.iter().any(|message| {
        message["role"] == "tool"
            && message["content"]
                .as_str()
                .is_some_and(|content| content.contains("\"status\":\"denied\""))
    }) {
        return response_with_text("delegation denied");
    }
    RESPONSE.into()
}

fn spawn_call(role: &str, task: &str, scope: serde_json::Value) -> String {
    tool_call(
        "spawn_member",
        serde_json::json!({"role":role,"task":task,"scope":scope}),
    )
}

fn tool_call(name: &str, arguments: serde_json::Value) -> String {
    serde_json::json!({
        "id":"chatcmpl-delegation",
        "object":"chat.completion",
        "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{
            "id":"fixture-delegation","type":"function","function":{
                "name":format!("microswarm-core/{name}"),
                "arguments":arguments.to_string()
            }
        }]},"finish_reason":"tool_calls"}]
    })
    .to_string()
}

fn response_with_text(text: &str) -> String {
    serde_json::json!({
        "id":"chatcmpl-fixture",
        "object":"chat.completion",
        "choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}]
    })
    .to_string()
}
