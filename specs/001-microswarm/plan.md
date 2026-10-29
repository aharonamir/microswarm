# Implementation Plan: MicroSwarm

**Branch:** `001-microswarm` · **Date:** 2026-10-07 · **Spec:** [spec.md](./spec.md)

## Summary

Two Rust binaries and one MCP server. The **supervisor** (`microswarmd`) runs on the host: it owns channels, sessions, the container runtime, scope computation, the credential proxy and `supervisor.db`. The **agent core** (`microswarm-agent`) runs inside each container: it polls `/session/control/inbound.db`, runs one turn against a model, dispatches tools over MCP, evaluates permissions, and writes `/session/out/outbound.db` and `/session/out/events.db`. First-party tools (fs, shell, web fetch) ship separately as **`microswarm-tools`**, an MCP server that runs inside the agent container.

Delegation is four tools the core exposes to a lead. The core does not spawn anything: it writes a `delegation_request` to `out/outbound.db`, and the supervisor decides.

## Technical context

| | |
|---|---|
| **Language** | Rust stable (MSRV pinned in `rust-toolchain.toml`) |
| **Async** | Tokio (multi-thread in supervisor, current-thread in agent) |
| **Storage** | SQLite via `rusqlite` (bundled), WAL mode |
| **HTTP** | `hyper` + `rustls` (single TLS stack) |
| **Container runtime** | v0.1 supports Docker Engine over its local socket (`bollard`). Podman API compatibility exists, but Podman proxy networking is deferred until it can reach the host proxy while preserving internal-network egress isolation. |
| **MCP** | MCP client in core over stdio and streamable HTTP — see research R-02 |
| **Serialization** | `serde`, `serde_json`, `toml` |
| **Testing** | `cargo test`, `insta` for snapshots, `testcontainers`-style harness for integration, `proptest` for scope algebra |
| **Target** | Linux x86_64 / aarch64; agent built for musl static |
| **Performance goals** | SC-001…SC-004 |
| **Constraints** | ≤ 6,000 LoC per binary; no credentials in containers; no inter-container network |
| **Scale (v1)** | 1 host, ≤ 200 concurrent sessions, ≤ 8 members per lead |

## Constitution check

*Gate: must pass before Phase 0 and again after Phase 1.*

| # | Gate | Status | Note |
|---|------|--------|------|
| I | Audit budget | ✅ | Two binaries, budget tracked by CI (`tokei` gate) |
| II | Isolation | ✅ | One agent per container; permission engine has no off switch |
| III | Authority | ✅ | Core writes `delegation_request`; only supervisor talks to the runtime socket |
| IV | Scope | ✅ | `control/scope.json` written + fsynced before `container create`; proptest on intersection |
| V | Boundaries | ✅ | Tools via MCP; first-party tools are a separate binary |
| VI | Events | ✅ | `out/events.db` written by the actor that acted; supervisor events in `supervisor.db` |
| VII | Fail closed | ✅ | Unknown exit ⇒ `safety`; gates expire on restart; scope error ⇒ no spawn |
| VIII | Dumb core | ✅ | No memory, evolution or evaluation in either binary |

**Post-design re-check:** ✅ — one deviation recorded below.

## Project structure

### Documentation (this feature)

```text
specs/001-microswarm/
├── spec.md
├── plan.md            # this file
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── session-db.sql
│   ├── supervisor-db.sql
│   ├── scope.schema.json
│   ├── events.schema.json
│   ├── delegation-tools.json
│   ├── team-skill.md
│   ├── agent-exit-codes.md
│   └── cli.md
└── tasks.md
```

### Source code

```text
Cargo.toml                         # workspace
crates/
├── ms-proto/                      # shared types: messages, events, scope, exit codes (no I/O)
│   └── src/{lib,message,event,scope,exit}.rs
├── ms-store/                      # SQLite open/WAL/migrations for session + supervisor dbs
│   └── src/{lib,session,supervisor,migrate}.rs
├── ms-agent/                      # bin: microswarm-agent (runs in container)
│   └── src/
│       ├── main.rs                # boot, exit-code mapping
│       ├── loop.rs                # poll → turn → write → ack
│       ├── model.rs               # provider-agnostic chat client (via proxy)
│       ├── mcp.rs                 # MCP client: stdio + HTTP
│       ├── permit.rs              # allow/ask/deny engine
│       ├── delegate.rs            # spawn/send/dismiss/list → delegation_request
│       └── emit.rs                # out/events.db writer
├── ms-supervisor/                 # bin: microswarmd (host)
│   └── src/
│       ├── main.rs
│       ├── config.rs              # microswarm/policy/ceiling/tools.toml
│       ├── skills.rs              # team-skill install + role registry
│       ├── delegation.rs          # delegation state machine, dismissal
│       ├── exit.rs                # exit classification (fail closed)
│       ├── router.rs              # channel → session resolution, inbound writes, outbound delivery
│       ├── sessions.rs            # session registry + lifecycle state machine
│       ├── runtime.rs             # container create/start/stop/watch via bollard
│       ├── scope.rs               # intersection + ceiling + control/scope.json writer
│       ├── policy.rs              # spawn policy resolution
│       ├── approvals.rs           # ask gates, expiry, re-issue
│       ├── deadlines.rs           # delegation circuit breaker
│       ├── proxy.rs               # per-session credential-injecting HTTP proxy
│       ├── recover.rs             # restart reconciliation
│       ├── events.rs              # merged stream for host plane
│       └── i18n.rs                # en / zh strings
├── ms-channels/                   # channel adapters behind one trait
│   └── src/{lib,cli,telegram,webhook}.rs
├── ms-tools/                      # bin: microswarm-tools (MCP server: fs, shell, fetch)
├── ms-cli/                        # bin: microswarm (trace, sessions, skills, doctor)
└── ms-bench/                      # bin: spawn/latency benchmarks for SC-001…SC-004
examples/
└── skills/code-review-team/       # reference team skill used by quickstart + integration tests
tests/
├── contract/                      # schema + SQL contract tests
├── integration/                   # spawn, delegate, dismiss, crash, restart
└── adversarial/                   # scope escape, forged delegation, socket reach
images/
└── agent.Containerfile            # FROM scratch + static agent + tools
```

**Structure decision:** a Cargo workspace with two security-relevant binaries (`ms-agent`, `ms-supervisor`) whose line budgets are tracked separately. Shared types live in `ms-proto`, which does no I/O so it can be read in isolation. Channels and the CLI are separate crates so they don't count toward the audited budgets.

## Architecture

### Message flow (single agent)

```text
channel ─▶ router ─▶ sessions/{id}/control/inbound.db ─▶ [agent container] ─▶ out/outbound.db ─▶ router ─▶ channel
                                                      │
                                                      ├─▶ out/events.db ─▶ supervisor event stream ─▶ host plane
                                                      └─▶ HTTP via proxy (credentials injected) ─▶ model / remote MCP
```

The agent is woken by the supervisor sending `SIGUSR1` after an inbound write, with a fallback poll every 500 ms. The supervisor tails `out/outbound.db` using SQLite's `data_version` pragma and a 100 ms poll — see research R-05.

### Delegation flow

```text
lead core: tool call spawn_member{role, task, scope_request}
  └─ permit.rs: spawn_member allowed as a tool? (permission engine)
  └─ writes outbound row kind=delegation_request
supervisor:
  └─ policy.rs: resolve spawn policy for role → allow | ask | deny
       ask  → approvals.rs → prompt user → wait
       deny → write inbound to lead: delegation_result{denied}
  └─ scope.rs: requested ∩ lead_scope ∩ ceiling → supervisor.db scope + control/scope.json (fsync)
  └─ runtime.rs: create container (labels, mounts, net=proxy-only), start
  └─ deadlines.rs: record deadline in supervisor.db
  └─ write inbound to lead: delegation_result{member_id}
  └─ write inbound to member: task
member core: turn(s) → outbound kind=reply
supervisor: route member reply → lead inbound kind=member_reply; clear deadline
lead core: dismiss_member{member_id} → outbound kind=dismiss
supervisor: graceful stop → session status=closed
```

### Container profile

Every agent container is created with:

- Read-only root filesystem from a `FROM scratch` image (agent + tools binaries + CA bundle).
- `/session/control` bind-mounted read-only from `sessions/{id}/control/` (scope + inbound DB).
- `/session/out` bind-mounted read-write from `sessions/{id}/out/` (agent outbound DB + events DB).
- Additional mounts exactly as listed in the authoritative scope, with their `ro` / `rw` mode.
- `/tmp` as a size-limited tmpfs.
- Network: attached to a per-session internal network whose only reachable endpoint is that session's proxy listener. No route to other containers, the host, or the internet except via the proxy.
- `--cap-drop=ALL`, `no-new-privileges`, non-root UID, seccomp default profile, pids limit, memory limit.
- No environment variables carrying secrets. Runtime metadata may include `MS_SESSION_ID`, `MS_ROLE`, `MS_PROXY_URL`, `MS_MODEL_PROVIDER`, `MS_MODEL_NAME`, `MS_MODEL_CREDENTIAL` and `MS_MCP_SERVERS`; these are non-secret selectors/descriptors, with credential fields containing only scoped credential names. `MS_MCP_SERVERS` carries launch metadata for granted servers, never credential values.
- `MS_KIND` is `lead` or `member`; `MS_ROLE` is the role name recorded as `agent_role` in events. The supervisor's authoritative scope and hash are persisted in `supervisor.db`; proxy and supervisor authorization decisions use that stored copy, never `control/scope.json`.

### Scope algebra

Computed per dimension, implemented in `ms-proto::scope` as pure functions so they can be property-tested:

- **Mounts:** for each requested `(host_path, mode)`, keep it only if some lead mount `(lp, lm)` has `host_path` canonically under `lp`; result mode is `min(mode, lm)` where `ro < rw`. Then drop anything under a ceiling path. Paths are canonicalized (symlinks resolved) before comparison.
- **Credentials:** set intersection of credential *names*; values never leave the supervisor.
- **Tools:** set intersection of MCP server ids, then of tool names within each server; permission policy per tool is `max_strictness(requested, lead)` where `allow < ask < deny`.

### Exit and restart policy

| Exit reason | Code | Supervisor action |
|---|---|---|
| `ok` | 0 | Session closed normally |
| `transient` | 75 | Restart, up to 3 times with 1s/2s/4s backoff |
| `config` | 78 | No restart; notify |
| `safety` | 77 | No restart; notify; `member_failed{safety}` to lead if member |
| anything else | — | Treated as `safety` |

### Supervisor restart

1. Open `supervisor.db`; load sessions marked `active`.
2. List containers with label `ms.managed=true`.
3. Adopt containers whose `ms.session` matches an active session with a directory on disk; stop the rest.
4. Mark sessions active-in-db-but-no-container as `crashed` and apply the exit policy as `transient`.
5. Mark all `pending` approvals `expired`; re-issue each as a new approval.
6. Re-arm delegation deadlines from stored `started_at + timeout`.
7. Resume tailing every adopted session's `out/outbound.db` and `out/events.db` from the stored cursors.

## Phases

- **Phase 0 — Research:** resolve the unknowns in [research.md](./research.md). Exit: every R-item has a decision.
- **Phase 1 — Design:** [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md). Exit: constitution re-check passes.
- **Phase 2 — Tasks:** [tasks.md](./tasks.md), ordered by user story so each story is independently shippable.

## Milestones mapped to stories

| Milestone | Stories | Ships |
|---|---|---|
| v0.1 | US1, US5 (tools) | Single agent, CLI channel, MCP tools, permission engine, events |
| v0.2 | US2, US3 | Swarm, scope, delegation tools, circuit breaker, exit policy |
| v0.2.1 | US4 | Supervisor restart reconciliation |
| v0.3 | US6, US5 (event stream) | `trace`, host-plane stream |

## Complexity tracking

| Deviation | Why needed | Simpler alternative rejected because |
|---|---|---|
| A third per-session file, `events.db`, beyond the two-file contract | Events are high-volume and append-only; mixing them into `outbound.db` would make the supervisor's delivery tail scan every event | One file with a `kind` column couples delivery latency to event volume; one writer per file is preserved either way |
| A per-session HTTP proxy in the supervisor | Credentials must never enter containers (FR-033) | An external vault (OneCLI-style) adds a second service against Principle I; can replace `proxy.rs` later behind the same contract |
