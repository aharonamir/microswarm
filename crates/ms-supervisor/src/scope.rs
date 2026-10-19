use crate::config::ConfigSet;
use ms_proto::Scope;
use ms_store::{SessionRecord, SupervisorStore};
use std::path::Path;

pub fn build_lead_scope(_config: &ConfigSet, _session_id: &str) -> Result<Scope, String> {
    todo!("T038: build a lead scope and intersect it with the system ceiling")
}

pub fn install_lead_scope(
    _store: &mut SupervisorStore,
    _record: SessionRecord,
    _session_dir: &Path,
    _scope: &Scope,
) -> Result<(), String> {
    todo!("T038: persist the authoritative scope and fsync control/scope.json")
}
