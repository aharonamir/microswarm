use ms_tools::{fs_list, fs_read, fs_write};
use rmcp::ServiceExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, path::PathBuf};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestClient;

impl rmcp::ClientHandler for TestClient {}

fn temp_work() -> (PathBuf, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "ms-tools-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    let work = path.join("work");
    fs::create_dir_all(&work).unwrap();
    (path, work)
}

#[test]
fn filesystem_tools_read_list_write_and_reject_parent_or_symlink_escape() {
    let (base, work) = temp_work();
    fs::write(work.join("hello.txt"), "hello").unwrap();
    assert_eq!(fs_read(&work, "hello.txt").unwrap(), "hello");
    fs_write(&work, "new.txt", "written").unwrap();
    assert_eq!(fs::read_to_string(work.join("new.txt")).unwrap(), "written");
    assert!(
        fs_list(&work, ".")
            .unwrap()
            .contains(&"hello.txt".to_owned())
    );
    assert!(fs_read(&work, "../outside.txt").is_err());

    let outside = base.join("outside.txt");
    fs::write(&outside, "outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, work.join("link.txt")).unwrap();
    #[cfg(unix)]
    assert!(fs_read(&work, "link.txt").is_err());

    let _ = fs::remove_file(outside);
    fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn first_party_server_exposes_and_executes_scoped_filesystem_tools() {
    let (base, work) = temp_work();
    fs::write(work.join("hello.txt"), "hello MCP").unwrap();
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    let proxy_task = tokio::spawn(async move {
        let (mut socket, _) = proxy_listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = tokio::io::AsyncReadExt::read(&mut socket, &mut chunk)
                .await
                .unwrap();
            assert_ne!(read, 0, "proxy client closed before request completed");
            request.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&request);
            let Some((headers, body)) = text.split_once("\r\n\r\n") else {
                continue;
            };
            let expected_body = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if body.len() >= expected_body {
                break;
            }
        }
        tokio::io::AsyncWriteExt::write_all(
            &mut socket,
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
        )
        .await
        .unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    let (server_io, client_io) = tokio::io::duplex(16_384);
    let server_task = tokio::spawn(async move {
        ms_tools::FirstPartyTools::with_proxy(work, format!("http://{proxy_addr}"))
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = TestClient.serve(client_io).await.unwrap();
    let tools = client.peer().list_all_tools().await.unwrap();
    let names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<Vec<_>>();
    for expected in ["fs.read", "fs.list", "fs.write", "shell.run", "web.fetch"] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    let mut params = rmcp::model::CallToolRequestParams::new("fs.read");
    params.arguments = Some(
        serde_json::json!({"path":"hello.txt"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let result = client.peer().call_tool(params).await.unwrap();
    assert!(format!("{result:?}").contains("hello MCP"));
    let mut list = rmcp::model::CallToolRequestParams::new("fs.list");
    list.arguments = Some(serde_json::json!({"path":"."}).as_object().unwrap().clone());
    let result = client.peer().call_tool(list).await.unwrap();
    assert!(format!("{result:?}").contains("hello.txt"));
    let mut write = rmcp::model::CallToolRequestParams::new("fs.write");
    write.arguments = Some(
        serde_json::json!({"path":"created.txt","content":"written MCP"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let result = client.peer().call_tool(write).await.unwrap();
    assert!(format!("{result:?}").contains("written"));
    assert_eq!(
        fs::read_to_string(base.join("work/created.txt")).unwrap(),
        "written MCP"
    );
    let mut shell = rmcp::model::CallToolRequestParams::new("shell.run");
    shell.arguments = Some(
        serde_json::json!({"command":"printf shell-ok"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let result = client.peer().call_tool(shell).await.unwrap();
    assert!(format!("{result:?}").contains("shell-ok"));
    let mut fetch = rmcp::model::CallToolRequestParams::new("web.fetch");
    fetch.arguments = Some(
        serde_json::json!({"url":"https://example.test/","credential":"weather-api"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let result = client.peer().call_tool(fetch).await.unwrap();
    assert!(format!("{result:?}").contains("OK"));
    let proxy_request = proxy_task.await.unwrap();
    assert!(
        proxy_request
            .to_ascii_lowercase()
            .contains("x-ms-credential: weather-api")
    );
    assert!(proxy_request.contains("https://example.test/"));
    drop(client);
    server_task.await.unwrap();
    fs::remove_dir_all(base).unwrap();
}
