use std::path::Path;

#[derive(Debug)]
pub struct ToolError(String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ToolError {}

pub fn fs_read(_root: &Path, _path: &str) -> Result<String, ToolError> {
    todo!("T035: read a file confined to the work mount")
}

pub fn fs_list(_root: &Path, _path: &str) -> Result<Vec<String>, ToolError> {
    todo!("T035: list a directory confined to the work mount")
}

pub fn fs_write(_root: &Path, _path: &str, _content: &str) -> Result<(), ToolError> {
    todo!("T035: write a file confined to the work mount")
}
