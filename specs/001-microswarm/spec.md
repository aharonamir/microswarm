# Feature Specification: MicroSwarm — thin containerized multi-agent harness

**Feature branch:** `001-microswarm`
**Created:** 2026-10-07
**Status:** Draft
**Input:** "A minimal agent harness built in Rust on the functional surface of agent-core, following the NanoClaw philosophy, messaging-native, able to run a single agent or a swarm, with one container per agent."

---

## User scenarios & testing *(mandatory)*

### User Story 1 — Talk to a single isolated agent (Priority: P1)

An operator installs MicroSwarm on a host, connects one channel, and messages it. The message is handled by one agent running in its own container. The agent can use tools, and the agent can only see what its container mounts. The operator gets a reply on the same channel.

**Why this priority:** this is the two-file contract end to end. Nothing else works without it, and on its own it is already a usable product — NanoClaw's baseline with a thinner runtime.

**Independent test:** send a message on the CLI channel; verify a reply arrives, a container labelled with the session id ran, and the session directory contains `control/inbound.db`, `control/scope.json`, `out/outbound.db` and `out/events.db`.

**Acceptance scenarios:**

1. **Given** a fresh install with a model configured, **When** the operator sends "hello" on a channel, **Then** a reply arrives on the same channel and exactly one agent container exists for that session.
2. **Given** a running session, **When** the operator sends a second message, **Then** the same warm container handles it without a new spawn.
3. **Given** a session whose agent calls a tool, **When** the tool's permission policy is `ask`, **Then** the operator receives an approval prompt on the channel naming the tool and its arguments, and the tool runs only after approval.
4. **Given** a session, **When** the agent tries to read a host path outside its mounts, **Then** the read fails at the OS level and a `tool.result` event with an error is emitted.

---

### User Story 2 — A lead delegates work to isolated members (Priority: P1)

A user asks for something large. The lead decides to delegate, calls `spawn_member` with a role, a task and a requested scope. The supervisor evaluates the spawn policy, computes the member's scope as the intersection of the request, the lead's scope and the system ceiling, writes it to disk, and starts a separate container. The lead sends the member work, the member replies, the lead dismisses it, and the member container is torn down.

**Why this priority:** per-member isolation is the product's differentiator. Without it MicroSwarm is NanoClaw.

**Independent test:** with a team skill that declares a `researcher` role, ask a question that triggers delegation; verify two containers ran, the member's `control/scope.json` is a strict subset of the lead's, and after dismissal only the lead container remains.

**Acceptance scenarios:**

1. **Given** spawn policy `allow` for a role declared in a team skill, **When** the lead calls `spawn_member`, **Then** a new container with a new session id starts and the lead receives a member handle.
2. **Given** spawn policy `ask`, **When** the lead calls `spawn_member`, **Then** the user sees the role and the concrete requested scope (mounts, credentials, tools) and no container exists until they approve.
3. **Given** spawn policy `deny`, **When** the lead calls `spawn_member`, **Then** the call fails, a `delegation.denied` event is emitted, and no container is created.
4. **Given** a lead whose scope does not include `/data/finance`, **When** it requests a member with read access to `/data/finance`, **Then** the computed member scope excludes that path and a `scope.narrowed` event records what was dropped.
5. **Given** a running member, **When** the lead calls `dismiss_member`, **Then** the member's in-flight turn is allowed to flush, its container stops, its session directory is retained, and only the lead container remains.
6. **Given** a dismissed `researcher`, **When** the lead spawns `researcher` again, **Then** it gets a new session id, an empty history and freshly computed scope.

---

### User Story 3 — Failures are contained and visible (Priority: P2)

A member crashes mid-task, or times out, or exits because a safety check rejected an operation. The lead is never left waiting forever; it receives a structured failure it can reason about. Transient crashes are retried; safety exits never are.

**Why this priority:** without bounded failure the swarm hangs or, worse, retries straight past its own guardrails.

**Independent test:** kill a member container mid-turn; verify the lead's inbound receives a `member_failed` message within the delegation timeout and that a restart is attempted only for a transient exit reason.

**Acceptance scenarios:**

1. **Given** a member that exits with reason `transient`, **When** the supervisor sees the exit, **Then** it restarts the container and the unacknowledged inbound message is re-processed, up to the retry limit.
2. **Given** a member that exits with reason `safety`, **When** the supervisor sees the exit, **Then** it does not restart, and the lead receives a `member_failed` with reason `safety`.
3. **Given** a member that exits with no recognizable reason code, **When** the supervisor sees the exit, **Then** it treats it as `safety`.
4. **Given** a delegation with a 120-second deadline, **When** the member has not replied by the deadline, **Then** the lead receives `member_failed` with reason `timeout` and the member is torn down.
5. **Given** a lead (not a member) that exhausts its transient retries, **When** the limit is reached, **Then** the user receives a failure notice on their channel.

---

### User Story 4 — The supervisor restarts without losing the swarm (Priority: P2)

The supervisor process crashes or is upgraded. On restart it reattaches to the agent containers that are still running, rebuilds its session map, resumes delegation deadlines, and asks the user again for any approval that was pending.

**Why this priority:** the supervisor is the single point of authority; its restart must be boring.

**Independent test:** with a lead and one member running and an `ask` gate pending, `kill -9` the supervisor and start it again; verify both containers are re-adopted, no container is orphaned, and the user receives a fresh approval prompt.

**Acceptance scenarios:**

1. **Given** running containers labelled with session ids, **When** the supervisor restarts, **Then** it adopts every container whose session exists on disk and stops any that it cannot match.
2. **Given** a pending `ask` gate at crash time, **When** the supervisor restarts, **Then** the old gate is marked `expired` and a new prompt is sent to the user.
3. **Given** a delegation deadline recorded before the crash, **When** the supervisor restarts, **Then** the deadline is still enforced from its original start time.

---

### User Story 5 — Extend without rebuilding (Priority: P2)

A developer adds a new tool by writing an MCP server in any language and registering it. A skill author adds a team skill by writing Markdown with a role manifest. A host-plane developer subscribes to the event stream. None of them touches the Rust core.

**Independent test:** register a Python MCP server exposing one tool, start a session, and confirm the agent can call it, with no rebuild of either binary.

**Acceptance scenarios:**

1. **Given** a new MCP server registered in `tools.toml`, **When** a session starts whose scope includes it, **Then** its tools appear in the agent's tool list.
2. **Given** a team skill declaring roles and per-role scope, **When** it is installed, **Then** its roles become spawnable under `allow` policy.
3. **Given** a host-plane consumer, **When** it reads the event stream, **Then** it receives every event for every session with `seq` strictly increasing per session and no gaps.

---

### User Story 6 — Read the swarm's trajectory (Priority: P3)

An operator wants to know what happened in a session: which members ran, what each was allowed to do, every tool call, every decision. They run `microswarm trace <session>` and get the tree.

**Independent test:** after a delegation session, `microswarm trace` prints a tree rooted at the lead with each member as a child, each with its scope and its tool calls in order.

**Acceptance scenarios:**

1. **Given** a completed swarm session, **When** the operator runs `microswarm trace <root_session_id>`, **Then** the output includes every member session, its scope, and every event in `seq` order.
2. **Given** the same session, **When** queried with `--json`, **Then** output validates against `contracts/events.schema.json`.

---

### Edge cases

- **Lead requests a member while at the member limit.** Spawn fails with `delegation.denied` reason `limit`.
- **A member tries to spawn a member.** Members cannot delegate in v1; `spawn_member` is not in a member's tool list, and a forged call is rejected by the supervisor.
- **Scope intersection is empty.** The spawn still succeeds with an empty scope only if the request asked for nothing; otherwise it fails with reason `scope_empty` rather than producing a useless member.
- **User never answers an `ask` gate.** The gate expires after the approval timeout and resolves as `deny`.
- **Disk full while writing `control/scope.json`.** No container is started.
- **Container runtime unavailable.** Spawns fail closed; the lead receives `member_failed` reason `spawn_error`.
- **Two messages arrive for the same session concurrently.** They are appended to `control/inbound.db` in order; the core processes them sequentially.
- **A member writes to `out/outbound.db` after dismissal began.** Writes during the grace window are delivered; the container is force-stopped after the window.
- **Clock skew.** Deadlines are computed and enforced only by the supervisor, persisted as start time plus duration; containers never compare clocks.

---

## Requirements *(mandatory)*

### Functional requirements

**Modes and lifecycle**

- **FR-001:** The system MUST implement single-agent mode as swarm mode with zero members — one lifecycle state machine for both.
- **FR-002:** A lead container MUST persist for the duration of its session.
- **FR-003:** A member container MUST be torn down when the lead dismisses it, when the lead's session ends, or when its idle timeout elapses.
- **FR-004:** Every spawn MUST be a clean slate: new session id, empty history, freshly computed scope. There is no reattachment to a dismissed member.
- **FR-005:** Teardown MUST be graceful: signal, allow the in-flight turn to flush, force-stop after a bounded grace window.
- **FR-006:** Teardown MUST NOT delete the session directory.

**Runtime core**

- **FR-010:** The core MUST read one message from inbound, run one turn, write the reply to outbound, and acknowledge the message only after the turn completes.
- **FR-011:** The core MUST dispatch tool calls to MCP servers over stdio or HTTP and MUST NOT implement end-user tools itself.
- **FR-012:** The core MUST evaluate every tool call against the container's permission policy (`allow` / `ask` / `deny`) before dispatch.
- **FR-013:** The core MUST emit a typed event for every turn, model call, tool call, tool result, permission decision, delegation request and lifecycle transition.
- **FR-014:** The core MUST exit with a declared reason code (`ok`, `transient`, `safety`, `config`) via exit status and a final `agent.exit` event.
- **FR-015:** The core MUST NOT contain memory tiers, skill evolution, reward computation or evaluation logic.

**Storage**

- **FR-020:** Each session MUST have its own directory containing `control/inbound.db`, `control/scope.json`, `out/outbound.db` and `out/events.db`. `control/` MUST be bind-mounted read-only into the agent container; `out/` MUST be bind-mounted read-write.
- **FR-021:** Each SQLite file MUST have exactly one writer: the supervisor writes `inbound.db`; the agent writes `outbound.db` and `events.db`.
- **FR-022:** All SQLite files MUST use WAL mode.
- **FR-023:** A container MUST be able to reach only its own read-only `control/`, read-write `out/`, and the mounts its scope grants. It MUST NOT be able to modify the control database or scope file.

**Supervisor**

- **FR-030:** Only the supervisor MAY create, stop or inspect containers.
- **FR-031:** All lead↔member messages MUST route through the supervisor; containers MUST have no network path to each other.
- **FR-032:** The supervisor MUST enforce a system ceiling no scope can exceed: no supervisor state, no vault material, no other session's directory, no container-runtime socket.
- **FR-033:** Credentials MUST NOT be present in any container. Outbound model and tool HTTP traffic MUST pass through a per-session supervisor proxy that injects credentials permitted by that session's scope.
- **FR-034:** The supervisor MUST label every container with session id, root session id, role and spawn time.
- **FR-035:** On restart, the supervisor MUST rebuild its session map from container labels and session directories, and MUST stop containers it cannot match to a session.
- **FR-036:** The supervisor MUST persist pending approvals, delegation deadlines and in-flight spawns in `supervisor.db` (SQLite, WAL).
- **FR-037:** On restart, pending approvals MUST be marked expired and re-issued to the user; they MUST NOT auto-resume.

**Delegation and scope**

- **FR-040:** Delegation MUST be exposed to the lead as tool calls (`spawn_member`, `send_to_member`, `dismiss_member`, `list_members`) evaluated by the same permission engine as other tools.
- **FR-041:** `spawn_member` MUST be evaluated against a spawn policy `allow` / `ask` / `deny`, resolved per role.
- **FR-042:** An `ask` prompt MUST show the role, the task summary and the concrete requested scope.
- **FR-043:** `dismiss_member` MUST NOT be policy-gated.
- **FR-044:** A member's scope MUST start empty and be computed as `requested ∩ lead_scope ∩ system_ceiling`, independently for mounts, credentials and tools.
- **FR-045:** Mount intersection MUST also intersect access mode: a member cannot get `rw` on a path the lead holds `ro`.
- **FR-046:** Any part of a request removed by intersection MUST be recorded in a `scope.narrowed` event.
- **FR-047:** `control/scope.json` MUST be written and fsynced before the container starts, and MUST NOT change while it runs. The authoritative scope and hash MUST also be persisted in `supervisor.db`; supervisor and proxy decisions MUST use that stored copy.
- **FR-048:** Members MUST NOT be able to spawn members in v1.
- **FR-049:** Teams MAY be formed at runtime or declared by a team skill with a per-role scope manifest.

**Failure handling**

- **FR-050:** The supervisor MUST learn of container exit from runtime events, not by polling.
- **FR-051:** Exit reason `transient` MUST be retried up to a configured limit; `safety`, `config` and unknown reasons MUST NOT be retried.
- **FR-052:** Every delegation MUST carry a deadline. On deadline, member crash or spawn error, the supervisor MUST write a `member_failed` message to the lead's inbound.
- **FR-053:** When a lead exhausts retries, the user MUST be notified on their channel.

**Extension and observability**

- **FR-060:** Tools MUST be addable by registering an MCP server, with no rebuild of either binary.
- **FR-061:** Team skills MUST be Markdown with YAML front-matter declaring roles and per-role scope.
- **FR-062:** The supervisor MUST expose the merged event stream to host-plane consumers in `seq` order per session.
- **FR-063:** The CLI MUST provide `microswarm trace <session>` rendering the session tree.
- **FR-064:** User-facing supervisor messages MUST be available in English and Chinese.

### Key entities

- **Session** — a unit of work with an id, a role, a parent (for members) and a directory on disk.
- **Agent** — the core process inside one container, bound to one session.
- **Scope** — the computed mounts, credential grants and tool allowlist for one session.
- **Delegation** — a lead's request to spawn a member, with its policy outcome, deadline and status.
- **Approval** — a pending `ask` decision awaiting the user.
- **Message** — an entry in `inbound.db` or `outbound.db`.
- **Event** — a typed, sequenced record of something that happened.
- **Team skill** — a Markdown manifest declaring roles and their scope.

---

## Success criteria *(mandatory)*

### Measurable outcomes

- **SC-001:** Agent container cold start (spawn request → first `agent.ready` event) p50 ≤ 150 ms, p95 ≤ 400 ms on the reference host, excluding image pull.
- **SC-002:** Agent core static binary ≤ 30 MB; container image ≤ 40 MB.
- **SC-003:** Idle agent resident memory ≤ 25 MB.
- **SC-004:** A swarm of a lead plus four members adds ≤ 10% end-to-end latency per delegated turn versus the same work in one container, excluding model time.
- **SC-005:** Supervisor restart re-adopts 100% of matchable containers and leaves 0 orphans in the restart test suite.
- **SC-006:** In the adversarial suite, 0 cases where a member obtains a mount, credential or tool outside `requested ∩ lead_scope ∩ ceiling`.
- **SC-007:** 0 cases where a `safety` or unknown exit is retried.
- **SC-008:** Agent core and supervisor each ≤ 6,000 lines of Rust excluding tests.
- **SC-009:** A new MCP tool goes from registration to first successful call in under 5 minutes with no rebuild.

---

## Assumptions

- Single host in v1; no cross-host scheduling.
- Linux host with Docker Engine available and its socket reachable by the supervisor. v0.1 supports Docker; Podman support is deferred until host-proxy access is verified without allowing direct egress.
- One user/operator per install; multi-tenancy is out of scope.
- Skills are Markdown plus scripts; skills packaged as Python modules are out of scope.
- Model providers are reached over HTTP with OpenAI-compatible or Anthropic-compatible APIs.

## Out of scope (v1)

- Embeddable SDK.
- A2A between swarms (roadmap v0.4).
- In-process plugins and WASM extensions (roadmap v0.4).
- Host-plane cognitive layers beyond the event stream and `trace` (roadmap v0.3).
- Members spawning members.
- Members speaking directly to the user.
- Supervisor high availability.
