# Tasks: MicroSwarm

**Input:** [spec.md](./spec.md) · [plan.md](./plan.md) · [research.md](./research.md) · [data-model.md](./data-model.md) · [contracts/](./contracts/) · [quickstart.md](./quickstart.md)

**Format:** `[ID] [P?] [Story] Description — path`
- **[P]** — can run in parallel with other [P] tasks in the same phase (different files, no dependency).
- **[USn]** — the user story the task delivers. Setup, Foundational and Polish tasks carry no story label.
- Tests are written first and must fail before their implementation task starts.

## Current execution status (2026-10-29)

- Completed: Phase 1 T001–T009; Phase 2 T010–T021; US1 T022–T045; US2 T046–T066. The Phase 0 research checkpoint and Phase 2 checkpoint are closed. T008 measured Docker 29.1.3 and rootless Podman 5.7.0; R-04 keeps one internal network per session. Owner decision (2026-10-29): Docker is the supported v0.1 runtime; Podman is deferred because the internal-network proxy route is not verified. US1 / v0.1 checkpoint reached: quickstart §1 acceptance and the four ordered CI commands passed locally; [CI run 40851756306](https://github.com/aharonamir/microswarm/actions/runs/40851756306) is green. US2 is authorized; quickstart §2 passes against the fake model and Docker runtime. The standard CI runner skips runtime tests when the agent image is absent; the future runtime job must build it and set `MS_REQUIRE_RUNTIME=1`.
- R-13 and the 2026-10-18 review fixes remain recorded above. T022–T027 now pass, including runtime checks for read-only control files, secret isolation, no direct egress, scoped proxy access, and exact approval resumption. Runtime harness cleanup removes only resources created under its temporary home.
- Runtime evidence found that supervisor-created output WAL/SHM sidecars need container-writable modes and that an internal network's own gateway, rather than Docker's default bridge gateway, is required to reach the host proxy. Container identity follows the host session owner on rootful runtimes and uses UID 0 in a rootless user namespace, with all capabilities dropped and only scoped mounts exposed.
- T044 and T045 are implemented and targeted tests pass. The owner has authorized US2 after the US1 / v0.1 checkpoint.

---

## Phase 1 — Setup

- [x] T001 Create Cargo workspace, `rust-toolchain.toml`, crate skeletons per plan — `Cargo.toml`, `crates/*/Cargo.toml`
- [x] T002 [P] CI: fmt, clippy `-D warnings`, test, musl build of `ms-agent` and `ms-tools` — `.github/workflows/ci.yml`
- [x] T003 [P] CI line-budget gate: `tokei` on `ms-agent` and `ms-supervisor` (+ their share of `ms-proto`, `ms-store`), fail > 6,000 — `scripts/loc-budget.sh`
- [x] T004 [P] CI dependency gate: fail if the tree contains a second async runtime or a TLS stack other than rustls — `scripts/dep-gate.sh`
- [x] T005 [P] `FROM scratch` agent image with static `microswarm-agent`, `microswarm-tools`, CA bundle; non-root UID — `images/agent.Containerfile`
- [x] T006 [P] Integration harness: start `microswarmd` against a temp `$MS_HOME` and real Podman/Docker, with a scripted fake model server — `tests/integration/harness.rs`, `tests/fixtures/fake_model.rs`

## Phase 0 research spikes (blocking; run alongside Setup)

- [x] T007 Verify R-02: `rmcp` client over stdio + streamable HTTP, dependency tree, binary-size delta with LTO; record result — `specs/001-microswarm/research.md`
- [x] T008 [P] Verify R-04: measure create→start→first byte on Podman rootless and Docker on the reference host; decide on network pooling — `research.md`
- [x] T009 [P] Verify R-01/R-07: read agent-core `harness/security/` rule format and shell checks; record decisions — `research.md`

**Checkpoint:** every ⚠ item in research.md has a recorded decision.

---

## Phase 2 — Foundational (blocks all stories)

### Contract tests first

- [x] T010 [P] Contract test: `session-db.sql` applies cleanly; CHECK constraints reject unknown kinds and invalid JSON — `tests/contract/session_db.rs`
- [x] T011 [P] Contract test: `supervisor-db.sql` applies; lead/member CHECK; unique pending approval code — `tests/contract/supervisor_db.rs`
- [x] T012 [P] Contract test: `ms-proto` scope serializes to JSON valid under `scope.schema.json`; hash is stable — `tests/contract/scope_schema.rs`
- [x] T013 [P] Contract test: every `ms-proto` event type serializes valid under `events.schema.json`; agent/supervisor type split enforced — `tests/contract/events_schema.rs`
- [x] T014 [P] Property tests for scope algebra: result ⊆ lead; result ∩ ceiling = ∅; mode never upgrades; policy never loosens; symlink canonicalization; never a union — `crates/ms-proto/tests/scope_props.rs`

### Implementation

- [x] T015 [P] Shared types: messages, events, exit reasons/codes — `crates/ms-proto/src/{message,event,exit}.rs`
- [x] T016 Scope types + pure intersection functions (mounts, credentials, tools, limits) + `narrowed` reporting + canonical hash — `crates/ms-proto/src/scope.rs`
- [x] T017 [P] SQLite open helper (WAL, pragmas, busy timeout), migrations from contracts, `user_version` check — `crates/ms-store/src/{lib,migrate}.rs`
- [x] T018 [P] Session store: inbound append, outbound + cursor transaction, history, event append with gapless `seq` — `crates/ms-store/src/session.rs`
- [x] T019 [P] Supervisor store: sessions, delegations, approvals, team_roles, tail_cursors, supervisor_events — `crates/ms-store/src/supervisor.rs`
- [x] T020 [P] i18n string table en/zh for all user-facing supervisor strings — `crates/ms-supervisor/src/i18n.rs`
- [x] T021 Config loading and validation: `microswarm.toml`, `policy.toml`, `ceiling.toml`, `tools.toml` — `crates/ms-supervisor/src/config.rs`

**Checkpoint:** T010–T014 pass.

---

## Phase 3 — US1: Talk to a single isolated agent (P1) 🎯 MVP

**Goal:** one channel, one lead container, tools via MCP, permission engine, events.
**Independent test:** quickstart §1.

### Tests first

- [x] T022 [P] [US1] Integration: message on CLI channel → reply; exactly one container with `ms.session` label; session dir has four files — `tests/integration/us1_single_agent.rs` (red against current CLI/supervisor stubs)
- [x] T023 [P] [US1] Integration: second message handled by the same container (no new `container.created`) — same file (red against current CLI/supervisor stubs)
- [x] T024 [P] [US1] Integration: tool with policy `ask` produces approval prompt; tool runs only after `/approve` — `tests/integration/us1_approval.rs` (red against CLI/supervisor stubs; checks marker absent before approval and present afterward)
- [x] T025 [P] [US1] Adversarial: agent reading `~/.ssh`, `$MS_HOME/supervisor.db`, `/run/podman/podman.sock`, another session's dir → all fail at OS level; writes to `/session/control/inbound.db`, `scope.json`, `-wal`, and `-shm` fail at the OS level; shell.run/fs.write cannot forge approval rows — `tests/adversarial/mount_escape.rs`
- [x] T026 [P] [US1] Adversarial: container has no secrets in env, filesystem or `/proc/1/environ`; direct egress to the internet fails; egress via proxy without a scoped credential name fails — `tests/adversarial/no_secrets.rs`
- [x] T027 [P] [US1] Unit: permit engine defaults to ask for operators/interpreters with parsed structure; only simple commands allow; unparseable shell ⇒ deny with structured rewrite guidance — `crates/ms-agent/tests/permit.rs` (shell parsing branches green; full permission flow remains T032)

### Agent core

- [x] T028 [US1] Boot: read env, validate `control/scope.json` hash ⇒ `config` exit on mismatch; open read-only control and writable output DBs; emit `agent.ready` — `crates/ms-agent/src/main.rs`
- [x] T029 [US1] Loop: wake on `SIGUSR1` or 500 ms poll; read past cursor; one turn; write outbound + cursor atomically — `crates/ms-agent/src/loop.rs`
- [x] T030 [P] [US1] Model client via `MS_PROXY_URL`, OpenAI- and Anthropic-compatible, credential by name header — `crates/ms-agent/src/model.rs`
- [x] T031 [P] [US1] MCP client adapter: launch stdio servers listed in scope, connect HTTP servers via proxy, list + call tools filtered by scope — `crates/ms-agent/src/mcp.rs`; fake stdio MCP integration tests green; runtime descriptor contract documented as `MS_MCP_SERVERS`; host `tools.toml` accepts only scoped credential names.
- [x] T032 [US1] Permission engine: per-tool policy from scope + defaults; shell lexer per R-07; `ask` ⇒ outbound `approval_request` and suspend turn until `approval_result`; approval tokens must bind exact server, tool, and argument hash — `crates/ms-agent/src/permit.rs`
- [x] T033 [P] [US1] Event emitter: every agent event type in the schema — `crates/ms-agent/src/emit.rs`
- [x] T034 [US1] Exit mapping to contract codes; final `agent.exit` event — `crates/ms-agent/src/main.rs`
- [x] T035 [P] [US1] First-party MCP server: `fs.read/list/write`, `shell.run`, `web.fetch` (via proxy) — `crates/ms-tools/src/`

### Supervisor

- [x] T036 [US1] Runtime adapter: create (labels, mounts from scope, read-only root, tmpfs, cap-drop, no-new-privileges, limits, per-session internal network), start, stop with grace, kill, signal — `crates/ms-supervisor/src/runtime.rs`
- [x] T037 [US1] Runtime events watcher: push exits to sessions (no polling) — `crates/ms-supervisor/src/runtime.rs`
- [x] T038 [US1] Lead scope from config ∩ ceiling ⇒ authoritative scope/hash persisted in `supervisor.db`; `control/scope.json` written + fsynced before create — `crates/ms-supervisor/src/scope.rs`
- [x] T039 [US1] Session lifecycle state machine per data-model — `crates/ms-supervisor/src/sessions.rs`
- [x] T040 [US1] Router: channel thread ⇒ session (create lead on first message); append inbound; signal agent; tail outbound via `data_version`; deliver replies — `crates/ms-supervisor/src/router.rs`
- [x] T041 [US1] Per-session credential proxy: listener on session network, credential-name allowlist from authoritative scope, inject, forward over rustls, report outcomes through a supervisor event observer — `crates/ms-supervisor/src/proxy.rs`; local listener test covers scope checks and fetch forwarding.
- [x] T042 [US1] Approvals: create with a short code, render en/zh prompts, resolve once, expire to timed_out, write an inbound `approval_result` carrying the exact server/tool/argument hash — `crates/ms-supervisor/src/approvals.rs`; tests cover binding and timeout.
- [x] T043 [P] [US1] CLI channel adapter reads user lines and renders reply/approval messages — `crates/ms-channels/src/cli.rs`
- [x] T044 [P] [US1] `microswarm init`, `doctor`, `chat`, `creds`, `sessions` — `crates/ms-cli/src/`
- [x] T045 [US1] Lead idle timeout ⇒ graceful stop, `close_reason=idle` — `crates/ms-supervisor/src/sessions.rs`

**Checkpoint:** quickstart §1 passes; T022–T027 green. **v0.1 shippable.**

---

## Phase 4 — US2: Lead delegates to isolated members (P1)

**Goal:** spawn/send/dismiss/list, policy, scope intersection, teardown.
**Independent test:** quickstart §2.

### Tests first

- [x] T046 [P] [US2] Integration: `allow` role ⇒ second container, new session id, lead gets `member_id` — `tests/integration/us2_spawn.rs`
- [x] T047 [P] [US2] Integration: `ask` ⇒ prompt shows role, task, mounts, creds, tools; no container before approval — `tests/integration/us2_ask.rs`
- [x] T048 [P] [US2] Integration: `deny` ⇒ `delegation.denied`, no container — `tests/integration/us2_deny.rs`
- [x] T049 [P] [US2] Integration: request outside lead scope ⇒ narrowed, `scope.narrowed` event, member cannot read the path — `tests/integration/us2_narrowing.rs`
- [x] T050 [P] [US2] Integration: dismiss ⇒ in-flight reply delivered within grace, container gone, directory retained, only lead remains — `tests/integration/us2_dismiss.rs`
- [x] T051 [P] [US2] Integration: re-spawn same role ⇒ new id, fresh isolated history and scope — `tests/integration/us2_clean_slate.rs`
- [x] T052 [P] [US2] Adversarial: member forges a `delegation_request` row ⇒ rejected by supervisor; members' tool list has no delegation tools — `tests/adversarial/member_cannot_delegate.rs`
- [x] T053 [P] [US2] Adversarial: member networks are separate and internal; member cannot use a credential name the lead holds but did not grant — `tests/adversarial/member_isolation.rs`
- [x] T054 [P] [US2] Adversarial: symlink in lead mount pointing outside it does not widen member scope — `tests/adversarial/symlink_scope.rs`
- [x] T055 [P] [US2] Contract: team-skill parser accepts the example, rejects invalid front-matter atomically — `tests/contract/team_skill.rs`

### Implementation

- [x] T056 [US2] Delegation tools in core, lead-only, validated against `delegation-tools.json`; write outbound rows; block turn on result — `crates/ms-agent/src/delegate.rs`
- [x] T057 [US2] Team-skill install/parse/register; hash recorded; role instructions path — `crates/ms-supervisor/src/skills.rs`
- [x] T058 [US2] Spawn policy resolution: stricter of skill and `policy.toml`; `adhoc` default — `crates/ms-supervisor/src/policy.rs`
- [x] T059 [US2] Member scope: lead-path → host-path translation; `request ∩ template ∩ lead ∩ ceiling`; reject `scope_empty`; persist authoritative scope; write `control/scope.json` — `crates/ms-supervisor/src/scope.rs`
- [x] T060 [US2] Delegation orchestration: requested → awaiting_approval → spawning → active; member limit; `delegation_result` to lead; `task` to member — `crates/ms-supervisor/src/delegation.rs`
- [x] T061 [US2] Routing: `send_to_member` ⇒ member inbound `task`; member `reply` ⇒ lead inbound `member_reply`; reject rows from wrong session kinds — `crates/ms-supervisor/src/router.rs`
- [x] T062 [US2] Dismissal: `control:shutdown`, grace window, force stop, `close_reason=dismissed`; not policy-gated — `crates/ms-supervisor/src/delegation.rs`
- [x] T063 [US2] Session end cascades: stop members first, then lead — `crates/ms-supervisor/src/sessions.rs`
- [x] T064 [US2] Member idle timeout — `crates/ms-supervisor/src/sessions.rs`
- [x] T065 [US2] Member `ask` gates routed to root channel, prefixed with role + member id — `crates/ms-supervisor/src/approvals.rs`
- [x] T066 [P] [US2] Reference team skill `code-review-team` — `examples/skills/code-review-team/`

**Checkpoint:** quickstart §2 passes; T046–T055 green.

### US2 review follow-up

- [x] T101 [US2] Regression: role instructions are mounted read-only and prepended to member model requests — `tests/integration/us2_review_regressions.rs`
- [x] T102 [US2] Regression: initial member task replies and approvals are routed to the lead — same file
- [x] T103 [US2] Regression: `adhoc` role uses default spawn policy without an installed skill — same file
- [x] T104 [US2] Regression: member is stopped and marked failed when `send_to_member` deadline expires — same file

---

## Phase 5 — US3: Failures contained and visible (P2)

**Independent test:** quickstart §3.

### Tests first

- [x] T067 [P] [US3] Integration: transient exit ⇒ restart with backoff, unacked message reprocessed once, max 3 — `tests/integration/us3_transient.rs`
- [x] T068 [P] [US3] Integration: safety exit, unknown code, and non-OOM signal ⇒ no restart; lead gets `member_failed{safety}` — `tests/integration/us3_safety.rs`
- [x] T069 [P] [US3] Integration: deadline exceeded ⇒ `member_failed{timeout}`, member torn down — `tests/integration/us3_timeout.rs`
- [x] T070 [P] [US3] Integration: runtime unavailable at spawn ⇒ `member_failed{spawn_error}` / `denied{spawn_error}` — `tests/integration/us3_spawn_error.rs`
- [x] T071 [P] [US3] Integration: lead exhausts retries ⇒ bilingual notice on channel — `tests/integration/us3_lead_notice.rs`

### Implementation

- [x] T072 [US3] Exit classifier per `agent-exit-codes.md` incl. stricter-wins cross-check with last `agent.exit` — `crates/ms-runtime/src/lifecycle.rs`, used by supervisor
- [x] T073 [US3] Restart policy with backoff and `restart_count` — `crates/ms-supervisor/src/exit_watcher.rs`, state in `crates/ms-runtime/src/lifecycle.rs`
- [x] T074 [US3] Delegation circuit breaker: arm on `waiting_reply`, persist `deadline_at`, fire ⇒ synthesize `member_failed`, tear down — `crates/ms-supervisor/src/deadlines.rs`
- [x] T075 [US3] Terminal-failure notices for leads (en/zh) — `crates/ms-supervisor/src/service.rs`

**Checkpoint:** quickstart §3 passes. **v0.2 shippable** (US1–US3).

---

## Phase 6 — US4: Supervisor restart (P2)

**Independent test:** quickstart §4.

### Tests first

- [x] T076 [P] [US4] Integration: `kill -9` with lead + member running ⇒ both adopted, 0 orphans — `tests/integration/us4_adopt.rs`
- [x] T077 [P] [US4] Integration: container with unknown/closed session label ⇒ stopped, `container.orphan_stopped` — `tests/integration/us4_orphans.rs`
- [x] T078 [P] [US4] Integration: pending approval ⇒ `expired` + re-issued with new code; old code rejected — `tests/integration/us4_approvals.rs`
- [x] T079 [P] [US4] Integration: deadline armed before crash fires at original time after restart — `tests/integration/us4_deadlines.rs`
- [x] T080 [P] [US4] Integration: session active in db with no container ⇒ treated as transient crash — `tests/integration/us4_missing_container.rs`

### Implementation

- [x] T081 [US4] Reconciliation per plan "Supervisor restart" steps 1–7 — `crates/ms-supervisor/src/recover.rs`
- [x] T082 [US4] Tail-cursor persistence and resume — `crates/ms-supervisor/src/router.rs`
- [x] T083 [US4] Approval expiry + re-issue on boot — `crates/ms-supervisor/src/approvals.rs`
- [x] T084 [US4] Deadline re-arm on boot — `crates/ms-supervisor/src/deadlines.rs`

**Checkpoint:** quickstart §4 passes.

---

## Phase 7 — US5: Extend without rebuilding (P2)

### Tests first

- [x] T085 [P] [US5] Integration: `tools add` stdio Python MCP server, grant in scope, agent calls it; no rebuild — `tests/integration/us5_add_tool.rs`
- [x] T086 [P] [US5] Integration: HTTP MCP server reached only through proxy — `tests/integration/us5_http_tool.rs`
- [x] T087 [P] [US5] Integration: host-plane consumer receives all events, per-session `seq` gapless, merged order stable — `tests/integration/us5_event_stream.rs`

### Implementation

- [x] T088 [US5] `microswarm tools add|list|remove` writing `tools.toml` — `crates/ms-cli/src/tools.rs`
- [x] T089 [US5] Tail `events.db` per session + `supervisor_events`; merged stream over local Unix socket (NDJSON) — `crates/ms-supervisor/src/events.rs`
- [x] T090 [P] [US5] Telegram channel adapter (second channel proves the channel boundary) — `crates/ms-channels/src/telegram.rs`
- [x] T091 [P] [US5] Generic webhook channel adapter — `crates/ms-channels/src/webhook.rs`

---

## Phase 8 — US6: Read the swarm's trajectory (P3)

- [x] T092 [P] [US6] Snapshot test: `trace` text tree for a recorded two-member session — `crates/ms-cli/tests/trace_snapshot.rs`
- [x] T093 [P] [US6] Contract test: `trace --json` validates against `events.schema.json` — `tests/contract/trace_json.rs`
- [x] T094 [US6] `microswarm trace` text + `--json` + `--events` filter — `crates/ms-cli/src/trace.rs`

**Checkpoint:** **v0.3 shippable** (all stories).

---

## Phase 9 — Polish & cross-cutting

- [ ] T095 [P] Benchmarks for SC-001…SC-004; CI job on reference runner fails on regression > 20% — `crates/ms-bench/`
- [ ] T096 [P] Retention: delete session dirs older than `retention_days` — `crates/ms-supervisor/src/sessions.rs`
- [ ] T097 [P] Adversarial review of `ms-agent` and `ms-supervisor` against the Constitution Check; record findings — `docs/reviews/`
- [ ] T098 [P] Operator docs: install, config reference, team-skill authoring, threat model mapped to OWASP ASI — `docs/`
- [ ] T099 [P] Chinese translations reviewed by a native speaker — `crates/ms-supervisor/src/i18n.rs`
- [ ] T100 Run quickstart §0–§6 on a clean host; fix drift — `specs/001-microswarm/quickstart.md`

---

## Dependencies

```text
Setup (T001–T006) ─┬─▶ Foundational (T010–T021) ─▶ US1 ─▶ US2 ─▶ US3 ─▶ US4
Research (T007–T009)┘                              │       │
                                                   ├─▶ US5 (tools: after US1; event stream: after US1)
                                                   └───────┴─▶ US6 (needs US2 for multi-session traces)
Polish after the stories it touches.
```

- US2 depends on US1 (runtime, router, scope writer, approvals).
- US3 depends on US2 for member failures; lead-only parts (T071, T075) can start after US1.
- US4 depends on US2 and US3 (it recovers their state).
- US5 tool registration depends only on US1.

## Parallel opportunities

- All contract tests T010–T014 together.
- Within US1: T030, T031, T033, T035, T043, T044 in parallel once T028–T029 land.
- All `[P]` tests at the top of each story phase together.
- US5 adapters T090/T091 in parallel with US3/US4.

## MVP

**US1 alone** (through T045) is a usable single-agent product with per-session isolation, MCP tools, permissions and events. Ship it, measure SC-001…SC-003, then start US2.
