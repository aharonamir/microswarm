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

**Verified 2026-10-07:** source review pinned to openJiuwen agent-core commit [`8345a27`](https://github.com/openJiuwen-ai/agent-core/tree/8345a2730d6e4fad33ee5760a37b506855461049). `openjiuwen/harness/security/permission_engine/models.py` defines the `allow`, `ask`, and `deny` levels. Its `PermissionsSection` describes tool defaults, per-tool `tools`, and parameterized `rules`; it also includes separate `file_guard`, `net_guard`, and `approval_overrides` sections.

**MicroSwarm decision:** port tool defaults and per-tool rules into `policy.toml` and `scope.tools`. Do not port file guard or network guard because mounts and the credential proxy enforce those boundaries at the OS and proxy layers. Defer approval overrides (“always allow”) in v1 because persistent grants conflict with session-scoped scope. These decisions preserve the permission vocabulary while using MicroSwarm's policy and scope contracts.

Evidence: [agent-core `models.py` at 8345a27](https://github.com/openJiuwen-ai/agent-core/blob/8345a2730d6e4fad33ee5760a37b506855461049/openjiuwen/harness/security/permission_engine/models.py) and [`tool_policy.py`](https://github.com/openJiuwen-ai/agent-core/blob/8345a2730d6e4fad33ee5760a37b506855461049/openjiuwen/harness/security/permission_engine/toolguard/tool_policy.py).

---

## R-02 — MCP client in Rust

**Decision:** use the official Rust MCP SDK (`rmcp`) for the client, behind a thin `mcp.rs` adapter, supporting stdio and streamable HTTP.

**Rationale:** MCP is the extension boundary (Principle V). Hand-rolling JSON-RPC and capability negotiation would spend line budget on protocol code that a maintained crate already covers.

**Alternatives:** hand-written JSON-RPC client — smaller dependency tree, but takes on protocol-version tracking. Kept as fallback if the SDK's dependency footprint breaks the binary-size target.

**Verified 2026-10-07:** `cargo info rmcp` resolved `rmcp 3.5.1` (MSRV Rust 1.88). Its published README documents client-side stdio (`transport-child-process`) and streamable HTTP (`transport-streamable-http-client-reqwest`) in `~/.cargo/registry/src/*/rmcp-3.5.1/README.md` lines 32–39 and 57–60. The source exposes `TokioChildProcess` in `src/transport.rs` and `StreamableHttpClientTransportConfig::custom_headers` in `src/transport/streamable_http_client.rs`, which supports the scoped `X-MS-Credential` proxy header.

A throwaway x86_64 musl crate enabled `client`, `transport-child-process`, `transport-streamable-http-client-reqwest`, and `reqwest`, with `default-features = false`. `cargo tree --locked --features mcp --target x86_64-unknown-linux-musl` resolved 157 package entries: Tokio 1.53.2, reqwest 0.13.5, hyper 1.12.0, and rustls 0.23.45. It includes `futures-executor 0.3.34`, a generic executor without an independent I/O runtime; it does not include another async runtime or a non-rustls TLS stack. The dependency gate therefore checks runtime crates, not generic executors.

With release `lto = "fat"`, `codegen-units = 1`, and `strip = true`, the baseline static binary was 385,720 bytes and the client-feature build was 7,288,104 bytes, a 6,902,384-byte increase. This remains below the 30 MB core binary target. Evidence: `/tmp/ms-rmcp-spike/Cargo.toml`, `/tmp/ms-rmcp-spike/Cargo.lock`, and the `cargo tree`/`stat` commands recorded in the Phase 0 work session.

**Decision:** use `rmcp = 3.5.1` with default features disabled and only the client stdio and reqwest-backed streamable-HTTP features above. The measured footprint fits the target, and the dependency tree has one runtime (Tokio) and one TLS stack (rustls).

---

## R-03 — Container runtime interface

**Decision:** Docker Engine API over the local Unix socket via `bollard`; Podman supported through its Docker-compatible socket.

**Rationale:** one client covers both runtimes; container labels and the events stream (`/events`) give push notification of exits (FR-050) and restart reconciliation (FR-035).

**Alternatives:** shelling out to `docker` CLI — slower, harder to parse, no event stream. containerd gRPC directly — lower level, more code. Firecracker microVMs — stronger isolation, much higher cold start; reconsider for a hardened tier later.

---

## R-04 — Cold start budget

**Decision:** `FROM scratch` image with static musl binaries; image pre-pulled at install; containers created with `--network` pre-made per session; no shell, no init system. Agent opens SQLite and emits `agent.ready` before any model call.

**Rationale:** SC-001 counts spawn request → `agent.ready`. The dominant costs are runtime create/start and network attach, not the Rust process. Per-session network creation is done in parallel with `scope.json` write.

**Docker measurement (2026-10-07; partial):** 20 sequential samples on the reference host, Docker 29.1.3 rootful, using the already-cached `alpine:latest`, an internal network per sample, and `/bin/sh -c 'printf READY'`. Timings include Docker CLI process overhead. `network create`: p50 19.82 ms / p95 26.91 ms; `container create` attached to that network: p50 32.22 ms / p95 36.15 ms; `docker start --attach` to first output byte: p50 89.63 ms / p95 96.76 ms; network create through first byte: p50 141.57 ms / p95 153.29 ms. On one pre-created internal network, start to first byte was p50 87.64 ms / p95 96.61 ms. This is a runtime smoke measurement, not the agent's `agent.ready` latency (the agent is still a stub).

**Provisional decision:** do not pool networks on this Docker evidence. Network creation adds about 20 ms p50 and can overlap with scope writing; it is small relative to container create/start, while sharing networks would need careful preservation of the no-peer-network boundary.

**Status (2026-10-11):** the rootless Podman measurement is outstanding. Per the user's instruction, T008 remains blocked until the user unblocks it; do not infer runtime readiness from installation status. The Phase 0 checkpoint remains open. Once unblocked, repeat the benchmark with rootless Podman and finalize the network-pooling decision before starting US1.

**⚠ verify:** complete the rootless Podman measurement on the reference host; if the result changes the pooling decision, update this section before implementation.

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

**Decision:** shell evaluation distinguishes parseable commands from unparseable input. Parseable but complex commands (including pipes, command substitution, interpreter sinks, or redirections outside mounts) escalate to `ask`. The approval prompt shows the parsed command structure as well as the raw command. Unparseable input is `deny`, with a structured tool error explaining that parsing failed and asking the model to rewrite the command more simply; it does not produce an approval prompt.

**Rationale:** parseable complex commands follow agent-core's shell AST policy. Denying unparseable input follows Constitution VII (“When the system does not know, it denies”) and avoids asking the user to approve an opaque string (OWASP ASI09); the model can retry with simpler syntax. The container remains the OS security boundary, while shell evaluation is an additional policy gate.

**Verified 2026-10-07:** agent-core commit [`8345a27`](https://github.com/openJiuwen-ai/agent-core/tree/8345a2730d6e4fad33ee5760a37b506855461049) implements a tree-sitter Bash parser with a conservative fallback in `openjiuwen/harness/security/permission_engine/toolguard/shell_ast.py`. It classifies parseable complex input as `too_complex` and risky input that cannot be analyzed as `parse_unavailable`; `tool_policy.py` raises both to `ask` by default and applies an `interpreter_sink` shell guard to pipelines. MicroSwarm adopts the parser structure checks and interpreter-sink escalation, while retaining `deny` for unparseable input per Constitution VII. Tests are assigned to T027 for both branches.

Evidence: [agent-core `shell_ast.py` at 8345a27](https://github.com/openJiuwen-ai/agent-core/blob/8345a2730d6e4fad33ee5760a37b506855461049/openjiuwen/harness/security/permission_engine/toolguard/shell_ast.py) and [`tool_policy.py`](https://github.com/openJiuwen-ai/agent-core/blob/8345a2730d6e4fad33ee5760a37b506855461049/openjiuwen/harness/security/permission_engine/toolguard/tool_policy.py).

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
