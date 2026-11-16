//! Test fixture that commits supervisor status rows and streams their events to a console.
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::BufRead,
    path::{Path, PathBuf},
    thread,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::mpsc,
};

const SESSION_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn init_home(home: &Path) -> Result<Connection, Box<dyn std::error::Error>> {
    fs::create_dir_all(home.join("sessions"))?;
    let db = Connection::open(home.join("supervisor.db"))?;
    db.execute_batch(
        "CREATE TABLE sessions(id TEXT PRIMARY KEY,root_id TEXT,parent_id TEXT,kind TEXT,role TEXT,channel TEXT,channel_thread TEXT,status TEXT,created_at TEXT,last_active_at TEXT,closed_at TEXT,close_reason TEXT,scope_json TEXT);
         CREATE TABLE approvals(id TEXT,code TEXT,root_session_id TEXT,requesting_session_id TEXT,kind TEXT,payload TEXT,status TEXT,expires_at TEXT,created_at TEXT,resolved_at TEXT,reissued_as TEXT);
         CREATE TABLE delegations(id TEXT,lead_id TEXT,status TEXT);
         CREATE TABLE supervisor_events(seq INTEGER PRIMARY KEY,ts TEXT,ts_mono INTEGER,session_id TEXT,root_session_id TEXT,agent_role TEXT,type TEXT,data TEXT);",
    )?;
    let now = ms_runtime::utc_now();
    db.execute(
        "INSERT INTO sessions(id,root_id,kind,role,channel,status,created_at,last_active_at,scope_json) VALUES (?1,?1,'lead','lead','web','ready',?2,?2,'{}')",
        params![SESSION_ID, now],
    )?;
    Ok(db)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = PathBuf::from(
        env::args()
            .nth(1)
            .ok_or("usage: console_status_fixture MS_HOME")?,
    );
    let db = init_home(&home)?;
    let listener = UnixListener::bind(home.join("supervisor.sock"))?;
    println!("READY");
    use std::io::Write;
    std::io::stdout().flush()?;
    let (stream, _) = listener.accept().await?;
    let (read, mut write) = stream.into_split();
    let mut request = String::new();
    BufReader::new(read).read_line(&mut request).await?;
    if serde_json::from_str::<Value>(&request)?["op"] != "event_stream" {
        return Err("console follower did not request event_stream".into());
    }
    println!("CONNECTED");

    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    std::io::stdout().flush()?;

    let mut pending_event = None;
    while let Some(line) = receiver.recv().await {
        let command: Value = serde_json::from_str(&line)?;
        match command["op"].as_str() {
            Some("commit") => {
                let status = command["status"].as_str().ok_or("missing status")?;
                let seq = command["seq"].as_i64().ok_or("missing seq")?;
                let ts = ms_runtime::utc_now();
                let data = json!({"status":status});
                let transaction = db.unchecked_transaction()?;
                transaction.execute(
                    "UPDATE sessions SET status=?1,last_active_at=?2 WHERE id=?3",
                    params![status, ts, SESSION_ID],
                )?;
                transaction.execute(
                    "INSERT INTO supervisor_events(seq,ts,ts_mono,session_id,root_session_id,agent_role,type,data) VALUES (?1,?2,?1,?3,?3,'lead','turn.started',?4)",
                    params![seq, ts, SESSION_ID, data.to_string()],
                )?;
                transaction.commit()?;
                pending_event = Some(
                    json!({"seq":seq,"ts":ts,"ts_mono":seq,"session_id":SESSION_ID,"root_session_id":SESSION_ID,"agent_role":"lead","source":"supervisor","type":"turn.started","data":data}),
                );
                println!("{{\"committed\":{seq}}}");
                std::io::stdout().flush()?;
            }
            Some("deliver") => {
                let event = pending_event
                    .take()
                    .ok_or("no committed event to deliver")?;
                write.write_all(format!("{}\n", event).as_bytes()).await?;
                write.flush().await?;
                println!("{{\"delivered\":{}}}", event["seq"]);
                std::io::stdout().flush()?;
            }
            _ => return Err("unknown fixture command".into()),
        }
    }
    Ok(())
}
