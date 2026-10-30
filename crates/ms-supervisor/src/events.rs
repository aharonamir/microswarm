use serde_json::Value;
use std::path::Path;
use tokio::io::AsyncWrite;

pub async fn stream<W: AsyncWrite + Unpin>(
    home: &Path,
    request: &Value,
    writer: &mut W,
) -> Result<(), String> {
    ms_runtime::events::stream(home, request, writer).await
}
