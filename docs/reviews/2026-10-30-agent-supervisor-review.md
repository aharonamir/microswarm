# Adversarial review: agent and supervisor

Date: 2026-10-30  
Scope: `crates/ms-agent/` and `crates/ms-supervisor/`, reviewed against the eight Constitution Check gates in `.specify/memory/constitution.md`. This is a source review supported by the listed tests, not a penetration test or proof of absence of vulnerabilities.

## Findings

No exploitable cross-session authority or scope-widening path was confirmed in the reviewed paths. The following controls were verified in source and regression tests:

| Gate | Evidence and disposition |
|---|---|
| I. Audit budget | `scripts/loc-budget.sh` reported ms-agent 3,756, ms-supervisor/shared 5,993, and ms-runtime 3,365 Rust lines (2026-10-30). Supervisor has only seven lines of headroom; avoid adding source there without extracting code. |
| II. Isolation | `crates/ms-runtime/src/lib.rs` constructs per-session containers with separate networks, read-only control mount, and dropped capabilities. `tests/adversarial/member_isolation.rs` and `tests/adversarial/mount_escape.rs` cover member isolation and mount boundaries. |
| III. Authority | `crates/ms-supervisor/src/scope.rs` persists authoritative scope in supervisor state before writing the control scope file. `crates/ms-supervisor/src/proxy.rs` authorizes credential names from that scope. `tests/adversarial/no_secrets.rs` checks that credentials and runtime access do not leak to containers. |
| IV. Scope | `build_member_scope` in `crates/ms-supervisor/src/scope.rs` intersects the request, role template, lead scope, and ceiling; the result is persisted before spawn. `tests/adversarial/symlink_scope.rs` and `tests/integration/us2_narrowing.rs` cover path narrowing. |
| V. Boundaries | Agent extensions communicate through MCP and the host protocol; no native plugin ABI was found in the reviewed paths. |
| VI. Events | Agent emits typed events at source. `crates/ms-runtime/src/events.rs` verifies event session/root/role against supervisor-owned session records and uses a persistent merged sequence ledger. `tests/integration/us5_event_stream.rs` checks merged delivery and cursor behavior. |
| VII. Fail closed | `evaluate_shell` in `crates/ms-agent/src/permit.rs` denies parse failures and asks for complex commands. `classify_exit_with_agent_reason` in `crates/ms-runtime/src/lifecycle.rs` maps unknown agent reasons to safety. `crates/ms-supervisor/src/approvals.rs` binds approvals to the requested operation. |
| VIII. Dumb core | The agent performs protocol/tool execution and emits events; no host-plane memory, evaluation, or behavioral baseline subsystem was found in the reviewed agent/supervisor paths. |

## Residual risks

- Agent event databases are writable by the agent container. Identity checks prevent a row from claiming another session, but a compromised agent can still forge or damage telemetry for its own session. Treat agent events as untrusted observations; never use them as authorization or host-state authority. The host supervisor database remains authoritative.
- Shell classification is a defense-in-depth permission heuristic, not a sandbox. Writable mounts and explicitly approved commands can still cause damage within granted scope.
- The review did not exercise kernel/container-runtime escapes or audit the host OS, Docker daemon, model endpoint, or operator-supplied MCP servers.

## Evidence

Relevant tests: `tests/adversarial/{member_isolation,mount_escape,no_secrets,symlink_scope,member_cannot_delegate}.rs`; `tests/integration/us2_narrowing.rs`; `tests/integration/us5_event_stream.rs`. The full workspace test suite was not run as part of this source review.
