use ms_proto::{DelegationScope, Permission, Scope, ScopeCeiling, ToolNames, intersect_scope};
use proptest::prelude::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn scope_with_policy(policy: Permission, mount_mode: &str, credentials: Vec<String>) -> Scope {
    serde_json::from_value(json!({
        "version": 1,
        "session_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "derived_from": "config",
        "source_ref": "config",
        "mounts": [{"host_path":"/tmp", "container_path":"/work", "mode":mount_mode}],
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

proptest! {
    #[test]
    fn wildcard_intersection_preserves_strictest_effective_policy(
        lead_base in 0u8..3,
        request_base in 0u8..3,
        lead_shell in 0u8..3,
        request_shell in 0u8..3,
        lead_write in 0u8..3,
        request_write in 0u8..3,
        lead_wildcard in any::<bool>(),
        request_wildcard in any::<bool>(),
    ) {
        let permissions = [Permission::Allow, Permission::Ask, Permission::Deny];
        let mut lead = scope_with_policy(permissions[lead_base as usize], "rw", vec![]);
        let mut request = scope_with_policy(permissions[request_base as usize], "rw", vec![]);
        let selected = ToolNames::Selected(BTreeSet::from(["shell.run".into(), "fs.write".into(), "fs.read".into()]));
        lead.tools[0].tools = if lead_wildcard { ToolNames::all() } else { selected.clone() };
        request.tools[0].tools = if request_wildcard { ToolNames::all() } else { selected };
        lead.tools[0].overrides = BTreeMap::from([
            ("shell.run".into(), permissions[lead_shell as usize]),
            ("fs.write".into(), permissions[lead_write as usize]),
        ]);
        request.tools[0].overrides = BTreeMap::from([
            ("shell.run".into(), permissions[request_shell as usize]),
            ("fs.write".into(), permissions[request_write as usize]),
        ]);
        let result = intersect_scope(&request, &lead, &ScopeCeiling::default()).unwrap();
        let grant = &result.tools[0];
        for (tool, request_policy, lead_policy) in [
            ("shell.run", permissions[request_shell as usize], permissions[lead_shell as usize]),
            ("fs.write", permissions[request_write as usize], permissions[lead_write as usize]),
            ("fs.read", permissions[request_base as usize], permissions[lead_base as usize]),
        ] {
            let effective = grant.overrides.get(tool).copied().unwrap_or(grant.policy);
            prop_assert!(effective >= request_policy.max(lead_policy), "{tool}: {effective:?} < max({request_policy:?}, {lead_policy:?})");
        }
    }

    #[test]
    fn wildcard_subset_checks_effective_policy_for_named_tools(
        parent_shell in 0u8..3,
        child_shell in 0u8..3,
    ) {
        let permissions = [Permission::Allow, Permission::Ask, Permission::Deny];
        let mut parent = scope_with_policy(Permission::Allow, "rw", vec![]);
        let mut child = parent.clone();
        parent.tools[0].tools = ToolNames::all();
        child.tools[0].tools = ToolNames::all();
        parent.tools[0].overrides.insert("shell.run".into(), permissions[parent_shell as usize]);
        child.tools[0].overrides.insert("shell.run".into(), permissions[child_shell as usize]);
        prop_assert_eq!(child.is_subset_of(&parent), permissions[child_shell as usize] >= permissions[parent_shell as usize]);
    }
}

#[test]
fn missing_ceiling_path_does_not_fail_scope_intersection() {
    let root = std::env::temp_dir().join(format!("ms-missing-scope-{}", std::process::id()));
    let missing = root.join("not-created").join("workspace");
    let mut lead = scope_with_policy(Permission::Allow, "rw", vec![]);
    lead.mounts[0].host_path = root.clone();
    let mut request = scope_with_policy(Permission::Allow, "rw", vec![]);
    request.mounts[0].host_path = missing.clone();
    request.mounts[0].container_path = "/work/not-created/workspace".into();
    let ceiling = ScopeCeiling {
        paths: vec![root.join("blocked-not-created")],
        ..Default::default()
    };
    let result = intersect_scope(&request, &lead, &ceiling).unwrap();
    assert_eq!(result.mounts.len(), 1);
    assert_eq!(result.mounts[0].host_path, missing);
}

#[test]
fn intersection_never_copies_lead_delegation_to_member_scope() {
    let mut lead = scope_with_policy(Permission::Allow, "rw", vec![]);
    lead.delegation = Some(DelegationScope {
        enabled: true,
        max_members: 8,
    });
    let mut request = lead.clone();
    request.derived_from = ms_proto::DerivedFrom::Delegation;
    request.delegation = Some(DelegationScope {
        enabled: true,
        max_members: 8,
    });
    let result = intersect_scope(&request, &lead, &ScopeCeiling::default()).unwrap();
    assert!(result.delegation.is_none());
    assert!(result.derived_from != ms_proto::DerivedFrom::Config);
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

proptest! {
    #[test]
    fn delegation_never_widens_across_any_derived_from(
        derived in 0u8..3,
        request_enabled in any::<bool>(),
        lead_enabled in any::<bool>(),
        request_max in 0u8..33,
        lead_max in 0u8..33,
    ) {
        let mut lead = scope_with_policy(Permission::Allow, "rw", vec![]);
        lead.delegation = Some(DelegationScope { enabled: lead_enabled, max_members: lead_max });
        let mut request = scope_with_policy(Permission::Allow, "rw", vec![]);
        request.derived_from = [
            ms_proto::DerivedFrom::Config,
            ms_proto::DerivedFrom::TeamSkill,
            ms_proto::DerivedFrom::Delegation,
        ][derived as usize];
        request.delegation = Some(DelegationScope { enabled: request_enabled, max_members: request_max });
        let result = intersect_scope(&request, &lead, &ScopeCeiling::default()).unwrap();
        if request.derived_from == ms_proto::DerivedFrom::Config {
            let expected = DelegationScope {
                enabled: request_enabled && lead_enabled,
                max_members: request_max.min(lead_max),
            };
            prop_assert_eq!(result.delegation, Some(expected));
        } else {
            prop_assert!(result.delegation.is_none());
        }
    }
}
