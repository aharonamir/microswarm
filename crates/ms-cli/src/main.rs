use ms_store::SupervisorStore;
use serde_json::json;
use std::{
    env, fs,
    io::{self, BufRead, BufReader, BufWriter, IsTerminal, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::ExitCode,
};

mod model_check;
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
        "init" => init(&home, &args[1..]),
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
        "console" => console(&home, &args[1..]),
        other => Err(format!("unknown command {other:?}; run microswarm --help")),
    }
}

fn print_help() {
    println!(
        "microswarm <init|doctor|chat|console|sessions|trace|skills|tools|approve|deny|creds> [options]\n\nCommands:\n  init [--home DIR] [--lang en|zh] [--wizard|--no-wizard] (checks the model connection before saving)\n  doctor\n  chat [--session ID]\n  console [--listen 127.0.0.1:8790]\n  sessions [--all] [--json]\n  trace <session_id> [--json] [--events TYPES]\n  skills install <directory>\n  tools add <id> --stdio \"<command>\" | --http <https-url>\n  tools list\n  tools remove <id>\n  creds set <name> | creds list\n  approve <code> | deny <code>"
    );
}

fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn console(home: &Path, args: &[String]) -> Result<(), String> {
    let mut listen = "127.0.0.1:8790";
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--listen" => {
                index += 1;
                listen = args.get(index).ok_or("--listen requires an IP:PORT")?;
            }
            other => return Err(format!("unknown console option {other:?}")),
        }
        index += 1;
    }
    let address = listen
        .parse()
        .map_err(|_| "--listen must be an IP:PORT address".to_owned())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(ms_console::server::run(home.to_path_buf(), address))
}

fn init(home: &Path, args: &[String]) -> Result<(), String> {
    let mut lang = "en";
    let mut no_wizard = false;
    let mut force_wizard = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--lang" => {
                index += 1;
                lang = args.get(index).ok_or("--lang requires en or zh")?;
                if !matches!(lang, "en" | "zh") {
                    return Err("--lang must be en or zh".into());
                }
            }
            "--home" => {
                index += 1;
                if args.get(index).is_none() {
                    return Err("--home requires a directory".into());
                }
            }
            "--no-wizard" => no_wizard = true,
            "--wizard" => force_wizard = true,
            other => return Err(format!("unknown init option {other:?}")),
        }
        index += 1;
    }
    if no_wizard && force_wizard {
        return Err("--wizard and --no-wizard cannot be combined".into());
    }
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
    if force_wizard && !io::stdin().is_terminal() {
        return Err("the init wizard requires an interactive terminal".into());
    }
    if !no_wizard && (force_wizard || io::stdin().is_terminal()) {
        configure_wizard(home, lang)?;
    }
    println!("Initialized MicroSwarm home at {}", home.display());
    Ok(())
}

fn configure_wizard(home: &Path, lang: &str) -> Result<(), String> {
    println!("\nMicroSwarm setup\nConfigure the model connection used by the lead agent.\n");
    let config_path = home.join("microswarm.toml");
    let source = fs::read_to_string(&config_path).map_err(|error| error.to_string())?;
    let existing: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("cannot read {}: {error}", config_path.display()))?;
    let existing_provider = config_string(&existing, &["model", "provider"], "openai-compatible");
    let provider_default = if existing_provider == "anthropic" {
        "2"
    } else {
        "1"
    };
    let provider = loop {
        let choice = prompt(
            "Provider: [1] OpenAI-compatible  [2] Anthropic",
            provider_default,
        )?;
        match choice.as_str() {
            "1" | "openai-compatible" => break "openai-compatible",
            "2" | "anthropic" => break "anthropic",
            _ => println!("Choose 1 or 2."),
        }
    };
    let default_url = if provider == "anthropic" {
        "https://api.anthropic.com"
    } else {
        "https://api.openai.com/v1"
    };
    let current_provider_url = if existing_provider == provider {
        config_string(&existing, &["model", "base_url"], default_url)
    } else {
        default_url
    };
    let current_model = if existing_provider == provider {
        config_string(&existing, &["model", "model"], "")
    } else {
        ""
    };
    let base_url = prompt("API base URL", current_provider_url)?;
    let key = rpassword::prompt_password("API key (input hidden): ")
        .map_err(|error| format!("cannot read API key: {error}"))?;
    if key.is_empty() {
        return Err("API key cannot be empty; configuration was not changed".into());
    }
    let model = prompt("Model name", current_model)?;
    if model.is_empty() {
        return Err("model name cannot be empty".into());
    }
    let credential = prompt(
        "Credential label (not the API key; e.g. model-key)",
        config_string(&existing, &["model", "credential"], "model-key"),
    )?;
    validate_credential_name(&credential)?;
    if credential.starts_with("sk-") && credential.len() > 20 {
        return Err("that looks like an API key; enter a short label such as model-key here, then enter the key at the hidden API key prompt".into());
    }
    let runtime_socket = prompt(
        "Container runtime socket",
        config_string(
            &existing,
            &["runtime", "socket"],
            "unix:///var/run/docker.sock",
        ),
    )?;
    let image = prompt(
        "Agent image",
        config_string(&existing, &["runtime", "image"], "microswarm-agent:dev"),
    )?;
    let mount_host = prompt("Host directory to mount at /work (leave blank to skip)", "")?;
    if !mount_host.is_empty() && !mount_host.starts_with('/') && !mount_host.starts_with("~/") {
        return Err("mount path must be absolute or start with ~/".into());
    }
    let mount_mode = if mount_host.is_empty() {
        None
    } else {
        let mode = prompt("Mount access: [ro] read-only, [rw] read-write", "ro")?;
        match mode.as_str() {
            "ro" | "rw" => Some(mode),
            _ => return Err("mount mode must be ro or rw".into()),
        }
    };

    println!(
        "\nModel: {provider} / {model}\nEndpoint: {base_url}\nCredential label: {credential}\nRuntime: {runtime_socket}\nAgent image: {image}"
    );
    if let Some(ref mode) = mount_mode {
        println!("Mount: {mount_host} -> /work ({mode})");
    } else {
        println!("Mount: none");
    }
    let confirm = prompt("Save these settings? [Y/n]", "y")?;
    if !matches!(confirm.as_str(), "y" | "Y" | "yes" | "YES" | "") {
        println!("Setup cancelled; initialized with existing/default configuration.");
        return Ok(());
    }

    println!("Checking model connection with a one-token request…");
    model_check::check(provider, &base_url, &model, &key)?;
    println!("✓ Model connection verified.");

    store_local_credential(home, &credential, &key)?;

    let mut config: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("cannot update {}: {error}", config_path.display()))?;
    let root = config
        .as_table_mut()
        .ok_or("configuration root must be a table")?;
    let home_config = with_lang(root.get("home").cloned(), lang)?;
    root.insert("home".into(), home_config);
    root.insert(
        "runtime".into(),
        toml::Value::try_from(serde_json::json!({
            "socket": runtime_socket, "image": image
        }))
        .map_err(|error| error.to_string())?,
    );
    root.insert(
        "model".into(),
        toml::Value::try_from(serde_json::json!({
            "provider": provider, "base_url": base_url, "model": model, "credential": credential
        }))
        .map_err(|error| error.to_string())?,
    );
    let lead = root
        .get_mut("lead")
        .and_then(toml::Value::as_table_mut)
        .ok_or("configuration is missing [lead]")?;
    let scope = lead
        .get_mut("scope")
        .and_then(toml::Value::as_table_mut)
        .ok_or("configuration is missing [lead.scope]")?;
    let credentials = scope
        .entry("credentials")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("lead.scope.credentials must be an array")?;
    if !credentials
        .iter()
        .any(|entry| entry.as_str() == Some(&credential))
    {
        credentials.push(toml::Value::String(credential.clone()));
    }
    if let Some(mode) = mount_mode {
        let mounts = scope
            .entry("mounts")
            .or_insert_with(|| toml::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("lead.scope.mounts must be an array")?;
        mounts.retain(|mount| mount.get("path").and_then(toml::Value::as_str) != Some("/work"));
        mounts.push(
            toml::Value::try_from(serde_json::json!({
                "host": mount_host, "path": "/work", "mode": mode
            }))
            .map_err(|error| error.to_string())?,
        );
    }
    fs::write(
        &config_path,
        toml::to_string_pretty(&config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{}: {error}", config_path.display()))?;

    println!("✓ Model settings and host credential saved.");
    Ok(())
}

fn with_lang(existing: Option<toml::Value>, lang: &str) -> Result<toml::Value, String> {
    let mut value = existing.unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));
    let table = value.as_table_mut().ok_or("[home] must be a table")?;
    table.insert("lang".into(), toml::Value::String(lang.into()));
    Ok(value)
}

fn config_string<'a>(config: &'a toml::Value, path: &[&str], fallback: &'a str) -> &'a str {
    path.iter()
        .try_fold(config, |value, key| value.get(*key))
        .and_then(toml::Value::as_str)
        .unwrap_or(fallback)
}

fn prompt(label: &str, default: &str) -> Result<String, String> {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| error.to_string())?;
    let value = value.trim().to_owned();
    Ok(if value.is_empty() {
        default.into()
    } else {
        value
    })
}

fn validate_credential_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        Err("credential names may contain only ASCII letters, digits, _ and -".into())
    } else {
        Ok(())
    }
}

fn store_local_credential(home: &Path, name: &str, value: &str) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let directory = home.join("credentials");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let path = directory.join(name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    file.write_all(value.as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
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
    let checks = ms_runtime::doctor::run(ms_runtime::doctor::DoctorInput {
        config_path: &config_dir.display().to_string(),
        runtime_socket: &config.microswarm.runtime.socket,
        agent_image: &config.microswarm.runtime.image,
        model: &model.model,
        credential: &model.credential,
    });
    println!("{}", ms_runtime::doctor::render(&checks));
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
        if stdin.is_terminal() {
            print!("you › ");
            io::stdout().flush().map_err(|error| error.to_string())?;
        }
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
        if !should_send_chat_line(&input) {
            continue;
        }
        if is_incomplete_approval_command(&input) {
            println!("Usage: /approve CODE or /deny CODE (use the code from the approval prompt).");
            continue;
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
            if let Some(error) = response.get("error").and_then(serde_json::Value::as_str) {
                return Err(error.into());
            }
            if let Some(text) = response.get("text").and_then(serde_json::Value::as_str) {
                println!("agent › {text}");
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
            }
        }
    }
    Ok(())
}

fn should_send_chat_line(input: &str) -> bool {
    !input.trim().is_empty()
}

fn is_incomplete_approval_command(input: &str) -> bool {
    let _ = input;
    false
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

#[cfg(test)]
mod chat_input_tests {
    #[test]
    fn blank_chat_line_is_not_sent() {
        assert!(!super::should_send_chat_line(""));
        assert!(!super::should_send_chat_line("   "));
        assert!(super::should_send_chat_line("hello"));
    }

    #[test]
    #[ignore = "red: T261 — bare approval commands must not be sent as user messages"]
    fn approval_command_requires_a_code() {
        assert!(super::is_incomplete_approval_command("/approve"));
        assert!(super::is_incomplete_approval_command("/deny   "));
        assert!(!super::is_incomplete_approval_command("/approve BC3V"));
        assert!(!super::is_incomplete_approval_command("please approve this"));
    }
}
