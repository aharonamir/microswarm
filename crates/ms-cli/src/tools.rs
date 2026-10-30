use http::Uri;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn run(home: &Path, args: &[String]) -> Result<(), String> {
    let path = tools_config_path(home);
    let mut document: toml::Value = toml::from_str(
        &fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?,
    )
    .map_err(|error| format!("{}: {error}", path.display()))?;
    let servers = document
        .get_mut("servers")
        .and_then(toml::Value::as_array_mut)
        .ok_or("tools.toml must contain a servers array")?;
    match args.first().map(String::as_str) {
        Some("list") if args.len() == 1 => {
            for server in servers {
                let id = server.get("id").and_then(toml::Value::as_str).unwrap_or("?");
                let transport = server.get("transport").and_then(toml::Value::as_str).unwrap_or("?");
                let detail = match transport {
                    "stdio" => {
                        let command = server.get("command").and_then(toml::Value::as_str).unwrap_or("?");
                        let args = server.get("args").and_then(toml::Value::as_array)
                            .map(|items| items.iter().filter_map(toml::Value::as_str).collect::<Vec<_>>().join(" "))
                            .unwrap_or_default();
                        format!("{command} {args}").trim().to_owned()
                    }
                    _ => server.get("url").and_then(toml::Value::as_str).unwrap_or("?").to_owned(),
                };
                println!("{id}  {transport}  {detail}");
            }
            Ok(())
        }
        Some("add") => {
            let id = args.get(1).ok_or("usage: microswarm tools add <id> --stdio <command> | --http <https-url>")?;
            if !valid_tool_id(id) {
                return Err("tool id must start with a lowercase letter or digit and contain only lowercase letters, digits, _, . or -".into());
            }
            if servers.iter().any(|server| server.get("id").and_then(toml::Value::as_str) == Some(id)) {
                return Err(format!("tool server {id:?} already exists"));
            }
            let entry = match (args.get(2).map(String::as_str), args.get(3)) {
                (Some("--stdio"), Some(command_line)) if args.len() == 4 => {
                    let mut words = parse_command_words(command_line)?;
                    if words.is_empty() { return Err("stdio command must not be empty".into()); }
                    let command = words.remove(0);
                    toml::Value::try_from(serde_json::json!({
                        "id":id,"transport":"stdio","command":command,"args":words
                    })).map_err(|error| error.to_string())?
                }
                (Some("--http"), Some(url)) if args.len() == 4 => {
                    let parsed = url.parse::<Uri>().map_err(|error| format!("invalid HTTP MCP URL: {error}"))?;
                    if parsed.scheme_str() != Some("https") || parsed.authority().is_none()
                        || parsed.authority().is_some_and(|authority| authority.as_str().contains('@')) {
                        return Err("HTTP MCP URL must use https and contain no user information".into());
                    }
                    toml::Value::try_from(serde_json::json!({"id":id,"transport":"streamable_http","url":url}))
                        .map_err(|error| error.to_string())?
                }
                _ => return Err("usage: microswarm tools add <id> --stdio <command> | --http <https-url>".into()),
            };
            servers.push(entry);
            write_toml_atomically(&path, &document)?;
            println!("Added MCP server {id}. Grant it under [lead.scope].tools before use.");
            Ok(())
        }
        Some("remove") if args.len() == 2 => {
            let id = &args[1];
            let before = servers.len();
            servers.retain(|server| server.get("id").and_then(toml::Value::as_str) != Some(id));
            if servers.len() == before { return Err(format!("tool server {id:?} was not found")); }
            write_toml_atomically(&path, &document)?;
            println!("Removed MCP server {id}.");
            Ok(())
        }
        _ => Err("usage: microswarm tools add <id> --stdio <command> | --http <https-url> ; tools list ; tools remove <id>".into()),
    }
}

fn tools_config_path(home: &Path) -> PathBuf {
    let nested = home.join("config/tools.toml");
    if nested.is_file() {
        nested
    } else {
        home.join("tools.toml")
    }
}

fn valid_tool_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && (value.as_bytes()[0].is_ascii_lowercase() || value.as_bytes()[0].is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
}

fn parse_command_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            word.push(character);
            escaped = false;
        } else if character == '\\' && quote != Some('\'') {
            escaped = true;
        } else if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                word.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(character);
        }
    }
    if escaped || quote.is_some() {
        return Err("stdio command has an unfinished escape or quote".into());
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

fn write_toml_atomically(path: &Path, document: &toml::Value) -> Result<(), String> {
    let contents = toml::to_string_pretty(document).map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("toml.{}.{nonce}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    use std::io::Write as _;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            let _ = fs::remove_file(&temporary);
            error.to_string()
        })?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
