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
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = respond(stream, approval_tool);
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

fn respond(stream: TcpStream, approval_tool: bool) -> std::io::Result<()> {
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
    let body = if approval_tool
        && request_body.contains("run approval tool")
        && !request_body.contains("\"role\":\"tool\"")
    {
        TOOL_RESPONSE
    } else {
        RESPONSE
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
