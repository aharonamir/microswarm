# Contract: supervisor additions and shared-library extractions

## 1. New RPC op: `end_session`

**Request** (one line on `supervisor.sock`):
```json
{"op":"end_session","session_id":"01JB8Q2K7M4X9R6T3V5W7Y9Z1A"}
```

**Behaviour** (implemented in `ms_runtime::lifecycle::end_session`; the supervisor's `dispatch` adds one arm):
1. Load the session. It must exist, have `kind='lead'` and not be `closed`/`failed`. Otherwise return `{"error":"not_a_lead"}` or `{"error":"already_closed"}`.
2. For every pending approval with `root_session_id` = this session: resolve it as `denied` with `resolved_at=now`, and emit `approval.resolved` with `data.reason="session_end"`.
3. Gracefully stop every non-closed member (`grace_ms`, then force). Each member gets `status='closed'`, `close_reason='session_end'`, its delegation becomes `dismissed`, and `session.closed` is emitted with `{"reason":"session_end","by":"operator"}`.
4. Gracefully stop the lead, set `status='closed'`, `close_reason='session_end'`, `closed_at=now`, and emit `session.closed` with `{"reason":"session_end","by":"operator"}`.
5. Session directories are kept (retention applies).

**Response:** `{"session_id":"…","closed_members":2,"status":"closed"}`

**Fail closed:** if any step errors, return `{"error":…}`. Already-closed members stay closed, and the lead is left in `stopping` for the existing exit watcher to finish. A partial failure is never reported as success.

**Tests (red first):** ends a lead with two members; rejects a member id; rejects a closed session; denies pending approvals in the tree; is idempotent on retry after a partial failure; and the supervisor line budget stays ≤ 6,000.

## 2. Budget extraction (before §1)

Move these from `crates/ms-supervisor/src/service.rs` into `crates/ms-runtime/src/lifecycle.rs` with no behaviour change:
`graceful_stop_session`, `stop_members_for_lead`, `stop_idle_lead`, `stop_member_idle`, and the shared parts of `close_idle_leads`/`close_idle_members`.

**Gate:**
- the full 001 test suite is green;
- `scripts/loc-budget.sh` shows supervisor + shared ≤ 5,850 after the move, and records before and after in the commit message.

## 3. Shared trace builder

Move the event collection and grouping in `crates/ms-cli/src/trace.rs` into `ms_runtime::trace::build(home, root_id) -> TraceData`. The CLI keeps text rendering. **Gate:** `crates/ms-cli/tests/trace_snapshot.rs` and `tests/contract/trace_json.rs` are unchanged and green.

## 4. Shared doctor

Move the checks in `ms-cli doctor` into `ms_runtime::doctor::run(home) -> Vec<Check{name, result, detail}>`. The CLI prints them as before. **Gate:** CLI doctor output is unchanged (snapshot).

## 5. Container profile, single source

Add `ms_runtime::profile::container_profile(scope, limits) -> ContainerProfile`, and make the container-spec builder in `ms-runtime/src/lib.rs` derive its read-only root, mounts, tmpfs, capability drop, `no-new-privileges`, pids and memory from that same value.

**Test:** for a sample scope, every fact in the profile is present in the generated container create body, and the reverse.
