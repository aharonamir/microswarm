use ms_store::SupervisorStore;
use serde_json::json;
use std::{
    env, fs,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::ExitCode,
};

mod tools;
mod trace;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("microswarm: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    let home = option_value(&args, "--home")
        .map(PathBuf::from)
        .or_else(|| env::var_os("MS_HOME").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".microswarm")))
        .ok_or_else(|| "HOME or MS_HOME is required".to_owned())?;
    match args[0].as_str() {
        "init" => init(&home),
        "doctor" => doctor(&home),
        "sessions" => sessions(&home),
        "trace" => trace::run(&home, &args[1..]),
        "skills" => skills(&home, &args[1..]),
        "tools" => tools::run(&home, &args[1..]),
        "creds" => credentials(&home, &args[1..]),
        "approve" | "deny" => rpc(
            &home,
            json!({"op":args[0], "code":args.get(1).ok_or("approval code is required")?}),
        )
        .map(|_| ()),
        "chat" => chat(&home, &args[1..]),
        other => Err(format!("unknown command {other:?}; run microswarm --help")),
    }
}

fn print_help() {
    println!(
        "microswarm <init|doctor|chat|sessions|trace|skills|tools|approve|deny|creds> [options]\n\nCommands:\n  init [--home DIR] [--lang en|zh]\n  doctor\n  chat [--session ID]\n  sessions [--all] [--json]\n  trace <session_id> [--json] [--events TYPES]\n  skills install <directory>\n  tools add <id> --stdio \"<command>\" | --http <https-url>\n  tools list\n  tools remove <id>\n  creds set <name> | creds list\n  approve <code> | deny <code>"
    );
}

fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn init(home: &Path) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|error| error.to_string())?;
    fs::create_dir_all(home.join("sessions")).map_err(|error| error.to_string())?;
    fs::create_dir_all(home.join("credentials")).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(home.join("credentials"), fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    write_default(home, "microswarm.toml", DEFAULT_MICROSWARM)?;
    write_default(home, "policy.toml", DEFAULT_POLICY)?;
    write_default(home, "ceiling.toml", DEFAULT_CEILING)?;
    write_default(home, "tools.toml", "servers = []\n")?;
    println!("Initialized MicroSwarm home at {}", home.display());
    Ok(())
}

fn write_default(home: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = home.join(name);
    if !path.exists() {
        fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

const DEFAULT_MICROSWARM: &str = "[home]\nretention_days = 30\nlang = \"en\"\n\n[runtime]\nsocket = \"unix:///var/run/docker.sock\"\nimage = \"microswarm-agent:dev\"\n\n[lead.scope]\nmounts = []\ncredentials = []\ntools = [{ server = \"microswarm-tools\", tools = \"*\", policy = \"ask\" }]\ndelegation = { enabled = false, max_members = 0 }\n\n[[channels]]\nkind = \"cli\"\n";
const DEFAULT_POLICY: &str = "[spawn]\ndefault = \"ask\"\n\n[tools.defaults]\n\"microswarm-tools/fs.read\" = \"ask\"\n\"microswarm-tools/fs.write\" = \"ask\"\n\"microswarm-tools/shell.run\" = \"ask\"\n";
const DEFAULT_CEILING: &str = "paths = [\"$MS_HOME\", \"/var/run\", \"/run\", \"/etc/shadow\", \"~/.ssh\"]\ncredentials = []\ntools = []\n";

fn doctor(home: &Path) -> Result<(), String> {
    let config_dir = if home.join("config/microswarm.toml").is_file() {
        home.join("config")
    } else {
        home.to_path_buf()
    };
    let config = ms_supervisor::config::load_config_dir(&config_dir)
        .map_err(|error| format!("configuration check failed: {error}"))?;
    let model = config
        .microswarm
        .model
        .as_ref()
        .ok_or("[model] must be configured in microswarm.toml")?;
    let credential = home.join("credentials").join(&model.credential);
    if !credential.is_file() {
        return Err(format!(
            "model credential {:?} is missing; run microswarm creds set {}",
            model.credential, model.credential
        ));
    }
    let runtime =
        ms_supervisor::runtime::ContainerRuntime::connect(&config.microswarm.runtime.socket)?;
    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot create runtime checker: {error}"))?;
    tokio_runtime.block_on(runtime.ping())?;
    if !tokio_runtime.block_on(runtime.image_exists(&config.microswarm.runtime.image)) {
        return Err(format!(
            "agent image {:?} is missing; build or pull it before starting a session",
            config.microswarm.runtime.image
        ));
    }
    println!("✓ configuration   {}", config_dir.display());
    println!("✓ runtime socket  {}", config.microswarm.runtime.socket);
    println!("✓ agent image     {}", config.microswarm.runtime.image);
    println!("✓ model           {} via session proxy", model.model);
    println!(
        "✓ credential      {} is present on the host",
        model.credential
    );
    Ok(())
}

fn credentials(home: &Path, args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => {
            let response = rpc(home, json!({"op":"creds_list"}))?;
            for name in response["names"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
            {
                println!("{name}");
            }
            Ok(())
        }
        Some("set") => {
            let name = args.get(1).ok_or("credential name is required")?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            {
                return Err(
                    "credential names may contain only ASCII letters, digits, _ and -".into(),
                );
            }
            let mut value = String::new();
            io::stdin()
                .read_to_string(&mut value)
                .map_err(|error| error.to_string())?;
            while value.ends_with(['\n', '\r']) {
                value.pop();
            }
            rpc(home, json!({"op":"creds_set", "name":name, "value":value}))?;
            Ok(())
        }
        _ => Err("usage: microswarm creds set <name> | microswarm creds list".into()),
    }
}

fn sessions(home: &Path) -> Result<(), String> {
    let db = home.join("supervisor.db");
    if !db.exists() {
        println!("No active sessions.");
        return Ok(());
    }
    let store = SupervisorStore::open_readonly(&db).map_err(|error| error.to_string())?;
    let mut statement = store.connection().prepare("SELECT id, role, status, channel, container_id FROM sessions WHERE status NOT IN ('closed','failed') ORDER BY created_at").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut found = false;
    for row in rows {
        found = true;
        let (id, role, status, channel, container) = row.map_err(|error| error.to_string())?;
        println!(
            "{id}  {role}  {status}  {}  containers: {}",
            channel.unwrap_or_else(|| "-".into()),
            usize::from(container.is_some())
        );
    }
    if !found {
        println!("No active sessions.");
    }
    Ok(())
}

fn skills(home: &Path, args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("install") => {
            let source = args.get(1).ok_or("skill directory is required")?;
            let installed = rpc(home, json!({"op":"skills_install", "path":source}))?;
            println!(
                "installed {}@{}",
                installed["name"].as_str().unwrap_or("unknown"),
                installed["version"].as_str().unwrap_or("unknown")
            );
            if let Some(roles) = installed["roles"].as_array() {
                for role in roles.iter().filter_map(serde_json::Value::as_str) {
                    println!("  role {role}");
                }
            }
            Ok(())
        }
        _ => Err("usage: microswarm skills install <directory>".into()),
    }
}

fn chat(home: &Path, args: &[String]) -> Result<(), String> {
    let socket = home.join("supervisor.sock");
    let stream = UnixStream::connect(&socket).map_err(|error| {
        format!(
            "cannot connect to microswarmd at {}: {error}",
            socket.display()
        )
    })?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    let mut writer = BufWriter::new(stream);
    let mut session_id = option_value(args, "--session").map(str::to_owned);
    let mut pending_wait: Option<(String, i64)> = None;
    let mut input = String::new();
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    loop {
        input.clear();
        if lock
            .read_line(&mut input)
            .map_err(|error| error.to_string())?
            == 0
        {
            if let Some((pending_session, after_id)) = pending_wait.take() {
                send_chat_request(
                    &mut writer,
                    &mut reader,
                    json!({"op":"wait", "session_id":pending_session, "after_id":after_id}),
                )?;
                let mut response = String::new();
                reader
                    .read_line(&mut response)
                    .map_err(|error| error.to_string())?;
                let response: serde_json::Value =
                    serde_json::from_str(&response).map_err(|error| error.to_string())?;
                if let Some(error) = response.get("error").and_then(serde_json::Value::as_str) {
                    return Err(error.into());
                }
                if let Some(text) = response.get("text").and_then(serde_json::Value::as_str) {
                    println!("agent › {text}");
                }
            }
            break;
        }
        while input.ends_with(['\n', '\r']) {
            input.pop();
        }
        let request = json!({"op":"chat", "session_id":session_id, "text":input});
        send_chat_request(&mut writer, &mut reader, request)?;
        let mut response = String::new();
        reader
            .read_line(&mut response)
            .map_err(|error| error.to_string())?;
        let response: serde_json::Value =
            serde_json::from_str(&response).map_err(|error| error.to_string())?;
        if let Some(error) = response.get("error").and_then(serde_json::Value::as_str) {
            return Err(error.into());
        }
        if let Some(text) = response.get("text").and_then(serde_json::Value::as_str) {
            println!("agent › {text}");
        }
        if let Some(id) = response
            .get("session_id")
            .and_then(serde_json::Value::as_str)
        {
            session_id = Some(id.to_owned());
        }
        if response.get("pending").and_then(serde_json::Value::as_bool) == Some(true) {
            if let Some(prompt) = response.get("prompt").and_then(serde_json::Value::as_str) {
                println!("{prompt}");
            }
            if let (Some(id), Some(after)) = (
                response
                    .get("session_id")
                    .and_then(serde_json::Value::as_str),
                response.get("after_id").and_then(serde_json::Value::as_i64),
            ) {
                pending_wait = Some((id.to_owned(), after));
            }
        } else if pending_wait.is_some()
            && (input.starts_with("/approve ") || input.starts_with("/deny "))
        {
            let (pending_session, after_id) = pending_wait.take().expect("checked pending wait");
            send_chat_request(
                &mut writer,
                &mut reader,
                json!({"op":"wait", "session_id":pending_session, "after_id":after_id}),
            )?;
            let mut response = String::new();
            reader
                .read_line(&mut response)
                .map_err(|error| error.to_string())?;
            let response: serde_json::Value =
                serde_json::from_str(&response).map_err(|error| error.to_string())?;
            if let Some(text) = response.get("text").and_then(serde_json::Value::as_str) {
                println!("agent › {text}");
            }
        }
    }
    Ok(())
}

fn send_chat_request(
    writer: &mut BufWriter<UnixStream>,
    _reader: &mut BufReader<UnixStream>,
    request: serde_json::Value,
) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, &request).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn rpc(home: &Path, request: serde_json::Value) -> Result<serde_json::Value, String> {
    let socket = UnixStream::connect(home.join("supervisor.sock"))
        .map_err(|error| format!("cannot connect to microswarmd: {error}"))?;
    let mut reader = BufReader::new(socket.try_clone().map_err(|error| error.to_string())?);
    let mut writer = BufWriter::new(socket);
    serde_json::to_writer(&mut writer, &request).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let response: serde_json::Value =
        serde_json::from_str(&line).map_err(|error| error.to_string())?;
    if let Some(error) = response.get("error").and_then(serde_json::Value::as_str) {
        return Err(error.to_owned());
    }
    Ok(response)
}
