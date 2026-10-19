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

**T031 implementation evidence (2026-10-15):** `ms-agent::mcp` uses `rmcp 3.5.1` `TokioChildProcess` for stdio and the reqwest-backed streamable HTTP transport. `cargo tree --locked --workspace --edges normal,build --target x86_64-unknown-linux-musl` and the corresponding aarch64 target resolve Tokio 1.53.3, rmcp 3.5.1, reqwest 0.13.5, and rustls 0.23.46. Neither target graph contains another async runtime or a TLS implementation other than rustls. `futures-executor` is a generic executor; it is not an I/O runtime. `schannel` and `security-framework` exist in `Cargo.lock` only for other target platforms and are absent from both supported Linux musl graphs. The original dependency gate scanned the entire lockfile and therefore false-flagged these irrelevant platform-specific packages; `scripts/dep-gate.sh` now checks target-reachable normal/build dependencies for both supported targets. The T031 tests use a fake stdio MCP peer for initialization, filtered `tools/list`, and allowed `tools/call`, plus HTTP proxy/credential validation. The actual stripped x86_64 musl `microswarm-agent` release binary is 4,598,808 bytes. The T007 isolated feature delta remains 6,902,384 bytes (baseline 385,720 bytes, rmcp client-feature spike 7,288,104 bytes); the Rust source line budget is separately enforced.

**HTTP MCP routing detail:** an agent sends streamable-HTTP traffic to `MS_PROXY_URL/mcp/<server-id>` and places only the scoped credential name in `X-MS-Credential`. The host-side proxy resolves `<server-id>` to the HTTPS upstream from `tools.toml`; the upstream URL is never the agent's network destination. Stdio process metadata and remote upstream URLs contain no credential values. The proxy must reject unknown server ids and credentials outside the session scope (T041).

---

## R-03 — Container runtime interface

**Decision:** Docker Engine API over the local Unix socket via `bollard`; Podman supported through its Docker-compatible socket.

**Rationale:** one client covers both runtimes; container labels and the events stream (`/events`) give push notification of exits (FR-050) and restart reconciliation (FR-035).

**Alternatives:** shelling out to `docker` CLI — slower, harder to parse, no event stream. containerd gRPC directly — lower level, more code. Firecracker microVMs — stronger isolation, much higher cold start; reconsider for a hardened tier later.

---

## R-04 — Cold start budget

**Decision:** `FROM scratch` image with static musl binaries; image pre-pulled at install; containers created with `--network` pre-made per session; no shell, no init system. Agent opens SQLite and emits `agent.ready` before any model call.

**Rationale:** SC-001 counts spawn request → `agent.ready`. The dominant costs are runtime create/start and network attach, not the Rust process. Per-session network creation is done in parallel with `scope.json` write.

**Measurements (2026-10-15):** 20 sequential samples per runtime on the reference x86_64 host. Docker 29.1.3 (rootful) and Podman 5.7.0 (`podman info` reports `rootless=true`) used the same cached image, `alpine@sha256:5b10f432ef3da1b8d4c7eb6c487f2f5a8f096bc91145e68878dd4a5019afde11`, an internal network per sample, and `/bin/sh -c 'printf READY'`. Timings include CLI process overhead; p95 is nearest-rank over 20 samples (the maximum).

| Runtime | Network create | Container create on new internal network | Start/attach to first byte | Network create through first byte | Start/attach on pre-created internal network |
|---|---:|---:|---:|---:|---:|
| Docker 29.1.3 | p50 20.15 / p95 21.48 ms | p50 33.73 / p95 39.16 ms | p50 92.56 / p95 104.59 ms | p50 207.23 / p95 228.76 ms | p50 88.42 / p95 97.12 ms |
| Podman 5.7.0 rootless | p50 18.84 / p95 21.20 ms | p50 30.76 / p95 35.69 ms | p50 124.12 / p95 145.99 ms | p50 177.05 / p95 213.66 ms | p50 131.05 / p95 144.14 ms |

The measurement starts the CLI operation with a monotonic timer; `start --attach` ends at the first stdout byte. The combined network-create-through-first-byte number also includes process and command sequencing overhead and should not be reconstructed by summing component percentiles. This is a runtime smoke measurement, not the agent's `agent.ready` latency (the agent is still a stub). SC-001 is specified as spawn request → first `agent.ready`, p50 ≤ 150 ms and p95 ≤ 400 ms (`spec.md`, SC-001, line 208); these results indicate the runtime portion is plausible but do not verify that outcome.

**Decision:** retain one internal network per session; do not pool networks. Network creation is about 19–20 ms p50 on both runtimes and can overlap with `scope.json` writing. Reusing networks offers only a small startup saving while requiring additional controls to preserve the no-peer-network boundary (plan.md, Constraints, line 25). The slower rootless Podman start path still leaves the measured runtime-only p95 below 400 ms; verify the full SC-001 path with the actual agent before claiming the target.

**T008 outcome:** rootless Podman and Docker were both measured on the reference host, and the network-pooling decision is final. The Phase 0 research checkpoint is closed.

---

## R-05 — Waking the agent and tailing outbound

**Decision:** supervisor writes `inbound.db`, then sends `SIGUSR1` to the container's PID 1 via the runtime's kill API. Agent also polls every 500 ms as fallback. Supervisor tails each `outbound.db` and `events.db` by checking `PRAGMA data_version` every 100 ms and reading rows past a stored cursor.

**Rationale:** no IPC channel, no socket shared with the container, no inotify across bind mounts (unreliable on some setups). `data_version` is cheap.

**Alternatives:** inotify — unreliable over overlay/bind mounts; Unix socket into container — a second channel to secure.

---

## R-06 — Credential injection

**Decision:** supervisor runs one HTTP proxy listener per active session on the session's internal network. The agent sends model requests to `MS_PROXY_URL` and remote-MCP requests to `MS_PROXY_URL/mcp/<server-id>`, with an optional `X-MS-Credential: <name>` header containing a scoped name only. The proxy resolves MCP ids from host `tools.toml`, checks the name against the authoritative scope JSON and hash stored in `supervisor.db`, strips the header, injects the secret, and forwards over TLS. Every supervisor authorization decision uses that stored scope, never the agent-visible `control/scope.json` file.

**Rationale:** credentials never enter the container (FR-033). Per-session listeners mean the proxy identifies the caller by which listener received the request — no token to steal.

**Alternatives:** a shared proxy authenticating by bearer token — the token itself becomes a credential inside the container. External vault service — violates Principle I for v1.

**Amendment (2026-10-18):** persist both `sessions.scope_hash` and `sessions.scope_json` in `supervisor.db`; proxy and supervisor decisions must read this authoritative copy. The container's read-only scope file is for agent boot validation only.

## R-13 — Read-only session control mount with SQLite WAL

**Question:** can the agent read `control/inbound.db` in WAL mode through a read-only bind mount?

**Experiment (2026-10-18):** on Podman 5.7.0, mounted a temporary control directory with `:ro` into Alpine Linux and queried a SQLite database with the Alpine SQLite 3.53.4 client using `file:/control/inbound.db?mode=ro`. With a supervisor writer connection keeping the WAL sidecars present, SQLite reported `wal` and read the committed row (`1`); the query exited 0. With the database closed first (no `-wal`/`-shm` sidecars), the same read-only bind mount failed with SQLite error 14 (`unable to open database file`).

**Decision (amended 2026-10-18):** every supervisor open of `control/inbound.db` sets SQLite `SQLITE_FCNTL_PERSIST_WAL=1`, then initializes the WAL/SHM sidecars before returning the session connections. Session creation must complete this step before writing `control/scope.json` and before container create. The agent opens the database read-only and never changes journal mode or initializes schema. Error code 14 (`SQLITE_CANTOPEN`) at agent boot exits transient 75 because the supervisor can restore/open the sidecars on restart. Keep the supervisor's inbound writer connection alive through container teardown; persistent sidecars also let a read-only agent open succeed after a supervisor process closes the database or restarts.

**Evidence:** `ms-store` uses `rusqlite 0.40.2` with the bundled SQLite library (`crates/ms-store/Cargo.toml`, `Cargo.lock`); `SQLITE_FCNTL_PERSIST_WAL` is applied in `crates/ms-store/src/migrate.rs`. The regression `inbound_wal_remains_readable_after_close_and_supervisor_restart` in `crates/ms-store/tests/store_behavior.rs` verifies both sidecars survive close and that a read-only agent sees rows after close and after reopening the supervisor. Agent boot maps a SQLite CANTOPEN (error 14) while opening `control/inbound.db` to transient exit 75 (`crates/ms-agent/src/boot.rs`; `inbound_sqlite_cantopen_exits_with_transient_code` in `crates/ms-agent/tests/boot.rs`). The runtime adversarial test remains necessary to verify read-only bind-mount write denial and missing-sidecar behavior under the actual container mount.`

---

## R-07 — Shell command evaluation

**Decision (amended 2026-10-18):** the shell classifier defaults to `ask`. It returns `allow` only when the lexer parses the command and, outside quotes, finds none of newline, `; & | < > ` $ ( ) { }`, and the executable basename is not an interpreter or wrapper (`sh`, `bash`, `dash`, `zsh`, `ksh`, `fish`, `python*`, `perl`, `ruby`, `node`, `php`, `lua`, `env`, `eval`, `exec`, `xargs`, `nohup`, `timeout`, `nice`, `sudo`, `su`, `doas`, `busybox`). Parseable commands outside that narrow safe subset return `ask`; prompts include parsed structure and raw command. Unparseable input is `deny`, with a structured tool error requesting a simpler rewrite and no approval prompt.

**Rationale:** parseable complex commands follow agent-core's shell AST policy. Denying unparseable input follows Constitution VII (“When the system does not know, it denies”) and avoids asking the user to approve an opaque string (OWASP ASI09); the model can retry with simpler syntax. The container remains the OS security boundary, while shell evaluation is an additional policy gate. The classifier governs shell syntax, not program behavior: a simple-looking command can invoke execution features built into programs (for example `awk system()`, `git -c core.sshCommand`, `tar --to-command`, or `find -exec`). The container remains the security boundary, and `shell.run` keeps its default policy at `ask`. Do not expand the interpreter/wrapper set into a general program deny-list.

**Verified 2026-10-07:** agent-core commit [`8345a27`](https://github.com/openJiuwen-ai/agent-core/tree/8345a2730d6e4fad33ee5760a37b506855461049) implements a tree-sitter Bash parser with a conservative fallback in `openjiuwen/harness/security/permission_engine/toolguard/shell_ast.py`. It classifies parseable complex input as `too_complex` and risky input that cannot be analyzed as `parse_unavailable`; `tool_policy.py` raises both to `ask` by default and applies an `interpreter_sink` shell guard to pipelines. MicroSwarm adopts the parser structure checks and interpreter-sink escalation, while retaining `deny` for unparseable input per Constitution VII. T027 covers both branches, the conservative allow subset, operator detection outside quotes, and interpreter/wrapper names, including versioned Python executables.

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
