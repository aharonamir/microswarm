# MicroSwarm Constitution

**Version:** 1.0.0 · **Ratified:** 2026-10-07 · **Last amended:** 2026-10-07

MicroSwarm is a thin, containerized, multi-agent harness: open for extension, closed for attack. These principles bind every spec, plan, task and pull request. A plan that violates one must say so in its Complexity Tracking table and justify it, or it does not pass the gate.

---

## Core principles

### I. Small enough to audit

The agent core and the supervisor are each budgeted at **≤ 6,000 lines of Rust, excluding tests and generated code**. A reviewer must be able to read either binary end to end in an afternoon.

- No microservices. One supervisor process on the host; one agent process per container.
- A new dependency needs a one-line justification in the PR description. Dependencies that pull an async runtime other than Tokio, or a second TLS stack, are rejected.
- Exceeding a budget requires a constitution amendment, not a waiver.

*Rationale:* auditability is load-bearing for the security claim. If nobody can read the core, "closed for attack" is an assertion, not a property.

### II. Isolation first, permissions still on

Every agent — lead or member — runs in its own container with its own session. The permission engine (`allow` / `ask` / `deny`) also runs inside every container. Two layers; never a choice between them.

- No code path may run two agents in one container or one process.
- No code path may disable the in-container permission engine.

### III. The supervisor is the only authority

Only the supervisor creates containers, routes messages between agents, and computes scope. Agents *request*; the supervisor *decides*.

- No peer-to-peer networking between agent containers.
- No agent holds container-runtime credentials, the Docker socket, or any equivalent.
- A system ceiling — supervisor state, vault keys, other sessions' directories — is unreachable by every container regardless of any manifest or request.

### IV. Default deny, intersect, never inherit

A member starts with nothing. Its scope is `requested ∩ lead_scope ∩ system_ceiling`. Never a union. Never implicit inheritance.

- Every grant traces to either a reviewed team-skill manifest or an explicit delegation request.
- Scope is written to disk before the container starts and never changes while it runs.

### V. Protocol boundaries, not language boundaries

Extension happens by speaking a protocol to the core, never by linking into it.

- Tools are MCP servers. Skills are Markdown plus scripts. Models are HTTP. Channels write to `inbound.db`. Cognitive layers read the event stream.
- The core exposes no native plugin ABI. In-process extension, if ever added, is WebAssembly with capability-based imports and requires an amendment.
- The core implements no end-user tools itself. First-party tools ship as a separate MCP server binary.

### VI. Events at the source

Every turn, tool call, tool result, permission decision, delegation and lifecycle transition is emitted as a typed event by the component that performed it, at the moment it happened.

- Trajectory is never reconstructed from logs.
- Every event carries `session_id`, `root_session_id`, `agent_role` and a monotonically increasing `seq`.

### VII. Fail closed

When the system does not know, it denies.

- An unknown exit reason is treated as a safety exit: no retry.
- A pending `ask` gate never survives a supervisor restart; the user re-approves.
- A scope that fails to compute means no container, not a default container.
- A safety-rejected operation is never retried by any circuit breaker.

### VIII. The core stays dumb on purpose

Memory tiers, skill evolution, reward, evaluation, failure *interpretation* and behavioural baselining live on the host plane. The core runs and reports; it does not reflect.

---

## Additional constraints

- **Language and runtime:** Rust (stable), Tokio. The agent core compiles to a static musl binary for `x86_64` and `aarch64`.
- **Storage:** SQLite in WAL mode, one file per writer, one directory per session. No other database in v1.
- **Credentials:** never present in any container's environment, filesystem or memory. Injected at the supervisor's per-session proxy.
- **Bilingual surface:** user-visible supervisor messages (approval prompts, failure notices) ship with English and Chinese strings from day one.

## Development workflow

1. Spec-first. `spec.md` → `plan.md` (+ `research.md`, `data-model.md`, `contracts/`) → `tasks.md` → implementation.
2. Every plan passes the Constitution Check below before Phase 0 and again after Phase 1.
3. Contract tests are written and failing before the implementation that satisfies them.
4. Plan and code pass an adversarial review gate before merge.
5. Assumptions about upstream code (`agent-core`, crates) are verified against source and recorded in `research.md` with a reference, not asserted from memory.

### Constitution Check (copy into every plan)

| # | Gate | Question |
|---|------|----------|
| I | Audit budget | Does this keep core and supervisor within 6,000 lines each? |
| II | Isolation | Does any path share a container or process between agents, or disable the permission engine? |
| III | Authority | Does anything other than the supervisor spawn, route between agents, or compute scope? |
| IV | Scope | Is every grant explicit, intersected, and written before start? |
| V | Boundaries | Does any extension link into the core? |
| VI | Events | Does every new state transition emit a typed event at source? |
| VII | Fail closed | What happens on unknown input, unknown exit, or restart? Is the answer "deny"? |
| VIII | Dumb core | Does anything cognitive land inside the container? |

## Governance

This constitution supersedes conflicting guidance in any spec, plan or task. Amendments require a written rationale, a version bump (MAJOR for removed or redefined principles, MINOR for added principles, PATCH for wording), and an update to every dependent template. Compliance is checked at plan review and at code review.
