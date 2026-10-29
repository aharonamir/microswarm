#[tokio::main(flavor = "current_thread")]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("--security-probe") {
        match security_probe() {
            Ok(result) => {
                println!("{result}");
                if result.as_object().is_some_and(|checks| {
                    checks.values().any(|value| value.as_bool() != Some(true))
                }) {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("security probe failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Err(error) = ms_tools::serve_stdio().await {
        eprintln!("microswarm-tools failed: {error}");
        std::process::exit(75);
    }
}

fn security_probe() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use std::{
        io::{Read, Write},
        net::{TcpStream, ToSocketAddrs},
        time::Duration,
    };

    fn sensitive_name(name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        ["api_key", "token", "secret", "password", "private_key"]
            .iter()
            .any(|marker| name.contains(marker))
    }

    let no_secret_environment = std::env::vars().all(|(name, _)| !sensitive_name(&name));
    let no_secret_proc_environment = std::fs::read("/proc/1/environ")
        .map(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter_map(|entry| entry.split(|byte| *byte == b'=').next())
                .all(|name| !sensitive_name(&String::from_utf8_lossy(name)))
        })
        .unwrap_or(false);
    let no_secret_files = [
        "/.env",
        "/root/.aws/credentials",
        "/root/.ssh/id_rsa",
        "/root/.config/gcloud/application_default_credentials.json",
        "/run/secrets",
    ]
    .iter()
    .all(|path| !std::path::Path::new(path).exists());

    let direct_egress_blocked =
        TcpStream::connect_timeout(&"1.1.1.1:443".parse()?, Duration::from_millis(300)).is_err();

    let proxy_requires_scoped_credential = (|| -> std::io::Result<bool> {
        let proxy = std::env::var("MS_PROXY_URL")
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let authority = proxy
            .strip_prefix("http://")
            .and_then(|value| value.split('/').next())
            .ok_or_else(|| std::io::Error::other("MS_PROXY_URL must use http"))?;
        let address = authority
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::other("MS_PROXY_URL has no address"))?;
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
        stream.set_read_timeout(Some(Duration::from_secs(1)))?;
        write!(
            stream,
            "GET /v1/models HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
        )?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            == Some("403"))
    })()
    .unwrap_or(false);

    Ok(serde_json::json!({
        "no_secret_environment": no_secret_environment,
        "no_secret_proc_environment": no_secret_proc_environment,
        "no_secret_files": no_secret_files,
        "direct_egress_blocked": direct_egress_blocked,
        "proxy_requires_scoped_credential": proxy_requires_scoped_credential,
    }))
}
