# Research: MicroSwarm

**Phase 0 output.** Each item records the question, the decision, the rationale, and the alternatives rejected. Items marked **⚠ verify** are decisions taken on the best current knowledge that must be confirmed against source before the task that depends on them starts.

---

## R-01 — Which parts of agent-core to port

**Decision:** port the *functional surface* of `openjiuwen.harness` that maps onto Principle VIII, and nothing else.

| agent-core (Python) | MicroSwarm | Ported? |
|---|---|---|
| `DeepAgent.invoke/stream/run_one_round` | `ms-agent::loop` (one turn per inbound message) | ✅ shape only |
| `harness/tools/` (shell, filesystem, web) | `microswarm-tools` MCP server | ✅ as separate binary |
| `harness/tools` MCP integration | `ms-agent::mcp` | ✅ |
| `security/` tiered permission policy, shell AST validation | `ms-agent::permit` | ✅ policy model; shell parsing via R-07 |
| `agent_teams` leader/teammate, message bus, task DB | `delegate.rs` + supervisor routing | ✅ semantics, not transport |
| `agent_teams` worktree isolation | container + scoped mounts | Replaced |
| `agent_teams/external` (Claude Code, Codex, CLI adapters) | — | ❌ v1 |
| `rails/` behavioural rails | permission engine only | Partial |
| memory, skills self-evolution, `auto_harness`, RSI | host plane | ❌ (Principle VIII) |
| `subagents/` built-ins (code, explore, plan, research…) | team skills (Markdown roles) | Replaced |
| cron tools | — | ❌ v1 |

**Rationale:** the review of agent-core 0.1.17 found the harness public surface is small (eight exported symbols) and that the valuable parts for this product are the loop, tool dispatch, permission tiers and team semantics. Everything else is cognitive or orchestration-of-other-harnesses.

**⚠ verify:** confirm the permission tier vocabulary and rule format in `harness/security/` so `permit.toml` stays compatible enough to import existing rules.

---

## R-02 — MCP client in Rust

**Decision:** use the official Rust MCP SDK (`rmcp`) for the client, behind a thin `mcp.rs` adapter, supporting stdio and streamable HTTP.

**Rationale:** MCP is the extension boundary (Principle V). Hand-rolling JSON-RPC and capability negotiation would spend line budget on protocol code that a maintained crate already covers.

**Alternatives:** hand-written JSON-RPC client — smaller dependency tree, but takes on protocol-version tracking. Kept as fallback if the SDK's dependency footprint breaks the binary-size target.

**⚠ verify (blocks v0.1):** current `rmcp` release, its transitive dependencies (must be Tokio + rustls only), and stdio + streamable-HTTP client support. Measure binary-size delta with LTO.

---

## R-03 — Container runtime interface

**Decision:** Docker Engine API over the local Unix socket via `bollard`; Podman supported through its Docker-compatible socket.

**Rationale:** one client covers both runtimes; container labels and the events stream (`/events`) give push notification of exits (FR-050) and restart reconciliation (FR-035).

**Alternatives:** shelling out to `docker` CLI — slower, harder to parse, no event stream. containerd gRPC directly — lower level, more code. Firecracker microVMs — stronger isolation, much higher cold start; reconsider for a hardened tier later.

---

## R-04 — Cold start budget

**Decision:** `FROM scratch` image with static musl binaries; image pre-pulled at install; containers created with `--network` pre-made per session; no shell, no init system. Agent opens SQLite and emits `agent.ready` before any model call.

**Rationale:** SC-001 counts spawn request → `agent.ready`. The dominant costs are runtime create/start and network attach, not the Rust process. Per-session network creation is done in parallel with `scope.json` write.

**⚠ verify:** measure create+start on Docker and Podman on the reference host; if network attach dominates, pre-create a small pool of internal networks.

---

## R-05 — Waking the agent and tailing outbound

**Decision:** supervisor writes `inbound.db`, then sends `SIGUSR1` to the container's PID 1 via the runtime's kill API. Agent also polls every 500 ms as fallback. Supervisor tails each `outbound.db` and `events.db` by checking `PRAGMA data_version` every 100 ms and reading rows past a stored cursor.

**Rationale:** no IPC channel, no socket shared with the container, no inotify across bind mounts (unreliable on some setups). `data_version` is cheap.

**Alternatives:** inotify — unreliable over overlay/bind mounts; Unix socket into container — a second channel to secure.

---

## R-06 — Credential injection

**Decision:** supervisor runs one HTTP proxy listener per active session on the session's internal network. The agent sends model and remote-MCP requests to `MS_PROXY_URL` with a `X-MS-Credential: <name>` header. The proxy checks the name is in the session's `scope.json` credential set, strips the header, injects the secret, and forwards over TLS.

**Rationale:** credentials never enter the container (FR-033). Per-session listeners mean the proxy identifies the caller by which listener received the request — no token to steal.

**Alternatives:** a shared proxy authenticating by bearer token — the token itself becomes a credential inside the container. External vault service — violates Principle I for v1.

---

## R-07 — Shell command evaluation

**Decision:** the permission engine evaluates shell tool calls by tokenizing with a POSIX shell lexer; any command containing pipes to interpreters, command substitution, or redirections to paths outside mounts escalates to `ask`. Unparseable input is `deny`.

**Rationale:** mirrors the shell-AST approach in agent-core's security layer while staying small; the container already bounds blast radius, so the lexer is a second layer, not the boundary.

**⚠ verify:** choose between `shell-words` (tokenize only) and a fuller parser; check what agent-core's shell AST validation flags so rule parity is achievable.

---

## R-08 — Event sequencing across writers

**Decision:** each session's `events.db` is written only by its agent, with a per-session `seq`. Supervisor events (spawn, scope, approval, exit) go to `supervisor.db.supervisor_events` with their own `seq`. The merged host-plane stream orders by `(ts_mono, source, seq)` and carries both `session_id` and `root_session_id`.

**Rationale:** preserves one writer per file; supervisor and agent events about the same session are joinable by id without either writing the other's file.

---

## R-09 — Approval prompts on messaging channels

**Decision:** approvals render as a channel message with a short code (`/approve 4F2K` / `/deny 4F2K`); channels that support buttons render buttons carrying the same code. Approval timeout default 10 minutes, resolving as `deny`.

**Rationale:** works on every channel including plain text; short codes prevent approving the wrong gate when several are pending.

---

## R-10 — Who speaks to the user

**Decision (v1):** only the lead speaks to the user. A member that hits an `ask` gate produces an approval routed by the supervisor to the **root session's channel**, prefixed with the member's role and id.

**Rationale:** resolves PRD open question Q1 for v1 with the smallest surface. Members never get a channel; the user always knows which agent is asking.

---

## R-11 — Idle timeout default

**Decision:** members 15 minutes idle; leads 24 hours idle. Both configurable.

**Rationale:** members are task-scoped and ephemeral; leads hold the user's session.

---

## R-12 — Skill format

**Decision:** skills are Markdown with YAML front-matter plus optional scripts; team skills add a `roles:` block. Skills mount read-only at `/skills/<name>`. Skills packaged as Python modules are not supported.

**Rationale:** resolves PRD Q5; keeps the extension boundary at protocol and file level.
