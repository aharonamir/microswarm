#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RESPONSE: &str = r#"{"id":"chatcmpl-fixture","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"fixture reply"},"finish_reason":"stop"}]}"#;
const TOOL_RESPONSE: &str = r#"{"id":"chatcmpl-tool","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"fixture-call","type":"function","function":{"name":"first-party/fs.write","arguments":"{\"path\":\"/work/approval-marker\",\"content\":\"approved\"}"}}]},"finish_reason":"tool_calls"}]}"#;

pub struct FakeModelServer {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    transient_failures: Arc<AtomicUsize>,
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
        let requests = Arc::new(AtomicUsize::new(0));
        let transient_failures = Arc::new(AtomicUsize::new(0));
        let thread_requests = Arc::clone(&requests);
        let thread_transient_failures = Arc::clone(&transient_failures);
        let thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = respond(
                            stream,
                            approval_tool,
                            delegation_tools,
                            &thread_requests,
                            &thread_transient_failures,
                        );
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
            requests,
            transient_failures,
            thread: Some(thread),
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    pub fn request_count(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
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

fn respond(
    stream: TcpStream,
    approval_tool: bool,
    delegation_tools: bool,
    requests: &AtomicUsize,
    transient_failures: &AtomicUsize,
) -> std::io::Result<()> {
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
    requests.fetch_add(1, Ordering::Relaxed);
    if request_body.contains("transient always")
        || (request_body.contains("transient once")
            && transient_failures.fetch_add(1, Ordering::Relaxed) == 0)
    {
        return Ok(());
    }
    let body = if delegation_tools
        && request_body.contains("\"hang\"")
        && !request_body.contains("microswarm-core/")
    {
        thread::sleep(Duration::from_secs(12));
        RESPONSE.to_owned()
    } else if delegation_tools {
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
    let system_text = messages
        .iter()
        .filter(|message| message["role"] == "system")
        .filter_map(|message| message["content"].as_str())
        .chain(request["system"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
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
    if system_text.contains("Research the requested files and report concise findings.")
        && user_text.contains("read file")
        && names.contains(&"microswarm-tools/fs.read")
    {
        return external_tool_call(
            "microswarm-tools/fs.read",
            serde_json::json!({"path":"/work/approval-target"}),
        );
    }
    if system_text.contains("Research the requested files and report concise findings.")
        && !user_text.contains("read file")
    {
        return response_with_text("role instructions reached model");
    }
    if !names
        .iter()
        .any(|name| name.starts_with("microswarm-core/"))
    {
        if user_text.contains("read file")
            && names.contains(&"microswarm-tools/fs.read")
            && !messages.iter().any(|message| {
                message["tool_calls"].as_array().is_some_and(|calls| {
                    calls.iter().any(|call| {
                        call.pointer("/function/name")
                            .and_then(serde_json::Value::as_str)
                            == Some("microswarm-tools/fs.read")
                    })
                })
            })
        {
            return external_tool_call(
                "microswarm-tools/fs.read",
                serde_json::json!({"path":"/work/approval-target"}),
            );
        }
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
    if user_text.contains("adhoc") && count("microswarm-core/spawn_member") == 0 {
        return spawn_call(
            "adhoc",
            "handle an ad hoc task",
            serde_json::json!({
                "mounts":[{"path":"/work","mode":"ro"}],
                "credentials":["fixture-model-key"],
                "tools":[]
            }),
        );
    }
    if user_text.contains("tester") && count("microswarm-core/spawn_member") == 0 {
        let task = if user_text.contains("read file") {
            "read file"
        } else if user_text.contains("run tests") {
            "run tests"
        } else {
            "run cargo test for src/auth"
        };
        return spawn_call(
            "tester",
            task,
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
            || (user_text.contains("twice") && count("microswarm-core/spawn_member") == 1)
            || (user_text.contains("again") && count("microswarm-core/dismiss_member") > 0)
        {
            return spawn_call(
                "researcher",
                if user_text.contains("read file") {
                    "read file"
                } else {
                    "review the auth module"
                },
                serde_json::json!({"mounts":[{"path":path,"mode":"ro"}]}),
            );
        }
    }
    if user_text.contains("send")
        && count("microswarm-core/send_to_member") == 0
        && let Some(member_id) = &member_id
    {
        let text = if user_text.contains("hang") {
            "hang"
        } else if user_text.contains("read file") {
            "read file"
        } else {
            "run the auth test suite"
        };
        return tool_call(
            "send_to_member",
            serde_json::json!({"member_id":member_id,"text":text,"timeout_s":10}),
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
    external_tool_call(&format!("microswarm-core/{name}"), arguments)
}

fn external_tool_call(name: &str, arguments: serde_json::Value) -> String {
    serde_json::json!({
        "id":"chatcmpl-delegation",
        "object":"chat.completion",
        "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{
            "id":"fixture-delegation","type":"function","function":{
                "name":name,
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
