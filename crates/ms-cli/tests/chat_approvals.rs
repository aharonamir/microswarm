use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn bare_approval_uses_the_latest_prompt_code_after_wait() {
    let home = std::env::temp_dir().join(format!(
        "ms-cli-chat-approval-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&home).unwrap();
    let listener = UnixListener::bind(home.join("supervisor.sock")).unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let read = stream.try_clone().unwrap();
        let mut reader = BufReader::new(read);
        let mut writer = stream;

        assert_eq!(request(&mut reader)["text"], "run task");
        respond(
            &mut writer,
            json!({"session_id":"S1","pending":true,"prompt":"Approve first? Reply: /approve AAAA or /deny AAAA.","after_id":11}),
        );
        assert_eq!(request(&mut reader)["text"], "/approve AAAA");
        respond(&mut writer, json!({"session_id":"S1"}));
        assert_eq!(request(&mut reader)["op"], "wait");
        respond(
            &mut writer,
            json!({"session_id":"S1","pending":true,"prompt":"Approve second? Reply: /approve BBBB or /deny BBBB.","after_id":22}),
        );
        let second_approval = request(&mut reader);
        assert_eq!(second_approval["op"], "chat");
        assert_eq!(second_approval["text"], "/approve BBBB");
        respond(&mut writer, json!({"session_id":"S1"}));
        assert_eq!(request(&mut reader)["op"], "wait");
        respond(&mut writer, json!({"text":"second approval completed"}));
    });

    let mut child = Command::new(env!("CARGO_BIN_EXE_microswarm"))
        .args(["chat", "--home"])
        .arg(&home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"run task\n/approve\n/approve\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("second approval completed"));
    fs::remove_dir_all(home).unwrap();
}

fn request(reader: &mut BufReader<std::os::unix::net::UnixStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn respond(writer: &mut std::os::unix::net::UnixStream, value: Value) {
    serde_json::to_writer(&mut *writer, &value).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
}
