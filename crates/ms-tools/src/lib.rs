use http_body_util::BodyExt;
use rmcp::ServiceExt;
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub struct ToolError(String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ToolError {}

pub fn fs_read(root: &Path, path: &str) -> Result<String, ToolError> {
    let path = resolve_path(root, path, false)?;
    fs::read_to_string(path).map_err(|error| ToolError(format!("cannot read file: {error}")))
}

pub fn fs_list(root: &Path, path: &str) -> Result<Vec<String>, ToolError> {
    let path = resolve_path(root, path, false)?;
    let mut names = fs::read_dir(path)
        .map_err(|error| ToolError(format!("cannot list directory: {error}")))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .map_err(|error| ToolError(format!("cannot read directory entry: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}

pub fn fs_write(root: &Path, path: &str, content: &str) -> Result<(), ToolError> {
    let path = resolve_path(root, path, true)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| ToolError(format!("cannot write file: {error}")))?;
    file.write_all(content.as_bytes())
        .map_err(|error| ToolError(format!("cannot write file: {error}")))
}

#[derive(Debug, serde::Deserialize, rmcp::schemars::JsonSchema)]
struct PathArgs {
    /// File or directory path inside /work.
    path: String,
}

#[derive(Debug, serde::Deserialize, rmcp::schemars::JsonSchema)]
struct WriteArgs {
    /// File path inside /work.
    path: String,
    /// UTF-8 contents to write.
    content: String,
}

#[derive(Debug, serde::Deserialize, rmcp::schemars::JsonSchema)]
struct ShellArgs {
    /// Shell command. The agent permission engine evaluates this before dispatch.
    command: String,
}

#[derive(Debug, serde::Deserialize, rmcp::schemars::JsonSchema)]
struct FetchArgs {
    /// HTTP or HTTPS URL to fetch through the session proxy.
    url: String,
    /// Scoped credential name, never a credential value.
    credential: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FirstPartyTools {
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
    work_dir: PathBuf,
    proxy_url: Option<String>,
}

impl FirstPartyTools {
    pub fn new(work_dir: impl Into<PathBuf>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            work_dir: work_dir.into(),
            proxy_url: std::env::var("MS_PROXY_URL").ok(),
        }
    }

    pub fn with_proxy(work_dir: impl Into<PathBuf>, proxy_url: impl Into<String>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            work_dir: work_dir.into(),
            proxy_url: Some(proxy_url.into()),
        }
    }
}

#[rmcp::tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for FirstPartyTools {}

#[rmcp::tool_router(router = tool_router)]
impl FirstPartyTools {
    #[rmcp::tool(name = "fs.read", description = "Read a UTF-8 file inside /work.")]
    async fn fs_read_tool(
        &self,
        args: rmcp::handler::server::wrapper::Parameters<PathArgs>,
    ) -> Result<String, rmcp::ErrorData> {
        fs_read(&self.work_dir, &args.0.path).map_err(tool_error)
    }

    #[rmcp::tool(name = "fs.list", description = "List a directory inside /work.")]
    async fn fs_list_tool(
        &self,
        args: rmcp::handler::server::wrapper::Parameters<PathArgs>,
    ) -> Result<String, rmcp::ErrorData> {
        let names = fs_list(&self.work_dir, &args.0.path).map_err(tool_error)?;
        serde_json::to_string(&names)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))
    }

    #[rmcp::tool(name = "fs.write", description = "Write a UTF-8 file inside /work.")]
    async fn fs_write_tool(
        &self,
        args: rmcp::handler::server::wrapper::Parameters<WriteArgs>,
    ) -> Result<String, rmcp::ErrorData> {
        fs_write(&self.work_dir, &args.0.path, &args.0.content).map_err(tool_error)?;
        Ok("written".into())
    }

    #[rmcp::tool(
        name = "shell.run",
        description = "Run a shell command in /work after agent approval."
    )]
    async fn shell_run_tool(
        &self,
        args: rmcp::handler::server::wrapper::Parameters<ShellArgs>,
    ) -> Result<String, rmcp::ErrorData> {
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(&args.0.command)
            .current_dir(&self.work_dir)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let timeout = std::env::var("MS_COMMAND_TIMEOUT_S")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(1, 300);
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(timeout), command.output())
                .await
                .map_err(|_| rmcp::ErrorData::internal_error("shell command timed out", None))?
                .map_err(|error| {
                    rmcp::ErrorData::internal_error(format!("cannot start shell: {error}"), None)
                })?;
        Ok(format!(
            "exit_code={}\nstdout={}\nstderr={}",
            output.status.code().unwrap_or(128),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }

    #[rmcp::tool(
        name = "web.fetch",
        description = "Fetch an HTTP(S) URL through the session credential proxy."
    )]
    async fn web_fetch_tool(
        &self,
        args: rmcp::handler::server::wrapper::Parameters<FetchArgs>,
    ) -> Result<String, rmcp::ErrorData> {
        fetch_via_proxy(
            self.proxy_url.as_deref(),
            &args.0.url,
            args.0.credential.as_deref(),
        )
        .await
    }
}

fn tool_error(error: ToolError) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(error.to_string(), None)
}

async fn fetch_via_proxy(
    proxy: Option<&str>,
    url: &str,
    credential: Option<&str>,
) -> Result<String, rmcp::ErrorData> {
    let upstream = url
        .parse::<http::Uri>()
        .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
    if !matches!(upstream.scheme_str(), Some("http" | "https"))
        || upstream.authority().is_none()
        || upstream
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(rmcp::ErrorData::invalid_params(
            "URL must be HTTP(S) without user information",
            None,
        ));
    }
    let proxy =
        proxy.ok_or_else(|| rmcp::ErrorData::internal_error("MS_PROXY_URL is missing", None))?;
    let proxy_uri = proxy.parse::<http::Uri>().map_err(|error| {
        rmcp::ErrorData::internal_error(format!("invalid proxy URL: {error}"), None)
    })?;
    if proxy_uri.scheme_str() != Some("http") || proxy_uri.authority().is_none() {
        return Err(rmcp::ErrorData::internal_error(
            "MS_PROXY_URL must be an HTTP URL",
            None,
        ));
    }
    let endpoint = format!("{}/fetch", proxy.trim_end_matches('/'))
        .parse::<http::Uri>()
        .map_err(|error| {
            rmcp::ErrorData::internal_error(format!("invalid fetch proxy endpoint: {error}"), None)
        })?;
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri(endpoint)
        .header(http::header::CONTENT_TYPE, "application/json");
    if let Some(name) = credential {
        let value = http::HeaderValue::from_str(name).map_err(|error| {
            rmcp::ErrorData::invalid_params(format!("invalid credential name: {error}"), None)
        })?;
        request = request.header("X-MS-Credential", value);
    }
    let request = request
        .body(http_body_util::Full::new(hyper::body::Bytes::from(
            serde_json::json!({"url": url}).to_string(),
        )))
        .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build_http();
    let response = client.request(request).await.map_err(|error| {
        rmcp::ErrorData::internal_error(format!("session proxy unavailable: {error}"), None)
    })?;
    let status = response.status();
    let bytes = http_body_util::Limited::new(response.into_body(), 1_048_576)
        .collect()
        .await
        .map_err(|error| {
            rmcp::ErrorData::internal_error(format!("invalid proxy response: {error}"), None)
        })?
        .to_bytes();
    if !status.is_success() {
        return Err(rmcp::ErrorData::internal_error(
            format!("web proxy returned HTTP {status}"),
            None,
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let work_dir = std::env::var_os("MS_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/work"));
    FirstPartyTools::new(work_dir)
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

fn resolve_path(root: &Path, requested: &str, allow_new_file: bool) -> Result<PathBuf, ToolError> {
    let root = fs::canonicalize(root)
        .map_err(|error| ToolError(format!("cannot resolve work directory: {error}")))?;
    let requested = Path::new(requested);
    let candidate = if requested.is_absolute() {
        requested
            .strip_prefix(&root)
            .map_err(|_| ToolError("path is outside the work directory".into()))?
            .to_owned()
    } else {
        requested.to_owned()
    };
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ToolError("path is outside the work directory".into()));
    }
    let full = root.join(candidate);
    let checked = match fs::canonicalize(&full) {
        Ok(existing) => existing,
        Err(error) if allow_new_file && error.kind() == std::io::ErrorKind::NotFound => {
            let parent = full
                .parent()
                .ok_or_else(|| ToolError("file path has no parent directory".into()))?;
            let parent = fs::canonicalize(parent)
                .map_err(|error| ToolError(format!("cannot resolve parent directory: {error}")))?;
            parent.join(
                full.file_name()
                    .ok_or_else(|| ToolError("file path has no filename".into()))?,
            )
        }
        Err(error) => return Err(ToolError(format!("cannot resolve path: {error}"))),
    };
    if !checked.starts_with(&root) {
        return Err(ToolError("path resolves outside the work directory".into()));
    }
    Ok(checked)
}
