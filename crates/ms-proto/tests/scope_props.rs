use ms_proto::{Permission, Scope, ScopeCeiling, intersect_scope};
use proptest::prelude::*;
use serde_json::json;

fn scope_with_policy(policy: Permission, mount_mode: &str, credentials: Vec<String>) -> Scope {
    serde_json::from_value(json!({
        "version": 1,
        "session_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "derived_from": "config",
        "source_ref": "config",
        "mounts": [{"host_path":"/tmp/ms-root", "container_path":"/work", "mode":mount_mode}],
        "credentials": credentials,
        "tools": [{"server":"fs", "tools":["read"], "policy":policy}],
        "limits": { "memory_mb": 128, "pids": 32, "tmp_mb": 64, "max_turns": 8, "max_tokens": 4096 },
        "narrowed": [],
        "hash": format!("sha256:{}", "0".repeat(64))
    }))
    .unwrap()
}

proptest! {
    #[test]
    fn intersection_never_widens_permissions(
        lead_rank in 0u8..3,
        request_rank in 0u8..3,
        lead_rw in any::<bool>(),
        request_rw in any::<bool>(),
        lead_credential in any::<bool>(),
        request_credential in any::<bool>(),
    ) {
        let permissions = [Permission::Allow, Permission::Ask, Permission::Deny];
        let lead = scope_with_policy(permissions[lead_rank as usize], if lead_rw {"rw"} else {"ro"}, if lead_credential {vec!["model".into()]} else {vec![]});
        let request = scope_with_policy(permissions[request_rank as usize], if request_rw {"rw"} else {"ro"}, if request_credential {vec!["model".into()]} else {vec!["other".into()]});
        let ceiling = ScopeCeiling::default();
        let result = intersect_scope(&request, &lead, &ceiling).unwrap();
        prop_assert!(result.is_subset_of(&lead));
        prop_assert!(result.is_subset_of(&request));
        prop_assert!(result.is_outside_ceiling(&ceiling));
        prop_assert!(result.mounts[0].mode <= lead.mounts[0].mode);
        prop_assert!(result.tools[0].policy >= lead.tools[0].policy);
        prop_assert!(!result.credentials.contains(&"other".to_string()));
    }
}

#[test]
fn symlink_mount_canonicalization_cannot_escape_lead_path() {
    let root = std::env::temp_dir().join(format!("ms-scope-{}", std::process::id()));
    let outside = root.with_extension("outside");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

    let lead: Scope = serde_json::from_value(serde_json::json!({
        "version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"config","source_ref":"config",
        "mounts":[{"host_path":root,"container_path":"/work","mode":"rw"}],"credentials":[],"tools":[],
        "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},"narrowed":[],"hash":format!("sha256:{}","0".repeat(64))
    })).unwrap();
    let request: Scope = serde_json::from_value(serde_json::json!({
        "version":1,"session_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","derived_from":"delegation","source_ref":"d",
        "mounts":[{"host_path":root.join("escape"),"container_path":"/work/escape","mode":"rw"}],"credentials":[],"tools":[],
        "limits":{"memory_mb":128,"pids":32,"tmp_mb":64,"max_turns":8,"max_tokens":4096},"narrowed":[],"hash":format!("sha256:{}","0".repeat(64))
    })).unwrap();
    let ceiling = ScopeCeiling::default();
    let result = intersect_scope(&request, &lead, &ceiling).unwrap();
    assert!(
        result.mounts.is_empty(),
        "canonicalized symlink target outside lead mount must be dropped"
    );
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}
