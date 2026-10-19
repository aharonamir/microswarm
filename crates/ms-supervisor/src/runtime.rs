use ms_proto::Scope;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug)]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

#[derive(Clone, Debug)]
pub struct ContainerProfile {
    pub read_only_rootfs: bool,
    pub no_new_privileges: bool,
    pub cap_drop: Vec<String>,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    pub network: String,
    pub tmpfs: BTreeMap<String, String>,
    pub mounts: Vec<MountSpec>,
}

impl ContainerProfile {
    pub fn for_session(
        _scope: &Scope,
        _image: &str,
        _session_dir: &Path,
        _network: &str,
    ) -> Result<Self, String> {
        todo!("T036: derive a restrictive container profile from authoritative scope")
    }
}
