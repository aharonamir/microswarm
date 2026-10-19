use ms_proto::Scope;
use ms_store::{StoreError, SupervisorStore};

pub struct SessionProxy {
    _scope: Scope,
}

impl SessionProxy {
    pub fn from_store(_store: &SupervisorStore, _session_id: &str) -> Result<Self, StoreError> {
        todo!("T041: load and validate authoritative scope from supervisor.db")
    }

    pub fn credential_allowed(&self, _credential_name: Option<&str>) -> bool {
        todo!("T041: enforce the session credential-name allowlist")
    }
}
