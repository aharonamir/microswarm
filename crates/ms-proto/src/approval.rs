use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Identity of the one tool invocation a supervisor approval permits.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovalBinding {
    pub server: String,
    pub tool: String,
    pub argument_hash: String,
}

impl ApprovalBinding {
    pub fn for_call(server: &str, tool: &str, arguments: Option<&Value>) -> Self {
        let canonical_arguments = serde_json::to_vec(&arguments.unwrap_or(&Value::Null))
            .expect("JSON values always serialize");
        let digest = Sha256::digest(canonical_arguments);
        Self {
            server: server.to_owned(),
            tool: tool.to_owned(),
            argument_hash: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        }
    }

    pub fn matches_call(&self, server: &str, tool: &str, arguments: Option<&Value>) -> bool {
        self == &Self::for_call(server, tool, arguments)
    }
}
