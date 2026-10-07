# Data Model: MicroSwarm

**Phase 1 output.** Authoritative DDL lives in [`contracts/session-db.sql`](./contracts/session-db.sql) and [`contracts/supervisor-db.sql`](./contracts/supervisor-db.sql); this document explains the entities, their relationships, and their state machines.

---

## On-disk layout

```text
$MS_HOME/
├── supervisor.db                 # writer: supervisor
├── config/
│   ├── microswarm.toml           # host config (models, channels, limits)
│   ├── tools.toml                # registered MCP servers
│   ├── policy.toml               # spawn policy per role, default tool permissions
│   └── ceiling.toml              # system ceiling (paths/creds never grantable)
├── skills/<name>/SKILL.md        # skills, incl. team skills
└── sessions/<session_id>/
    ├── inbound.db                # writer: supervisor
    ├── outbound.db               # writer: agent
    ├── events.db                 # writer: agent
    └── scope.json                # writer: supervisor, before start; immutable after
```

`session_id` is a ULID. Session directories are retained after teardown for `retention_days` (default 30).

---

## Entities

### Session

| Field | Type | Notes |
|---|---|---|
| `id` | ULID | Primary key; directory name |
| `root_id` | ULID | Lead's id for members; equals `id` for a lead |
| `parent_id` | ULID? | Lead id for members; null for a lead |
| `role` | string | `lead` or a team-skill role name |
| `kind` | enum | `lead` \| `member` |
| `channel` | string? | Channel binding; leads only |
| `channel_thread` | string? | Channel-native conversation id; leads only |
| `status` | enum | see state machine |
| `container_id` | string? | Current container; null when not running |
| `restart_count` | int | Transient restarts used |
| `created_at`, `closed_at` | timestamp | |
| `close_reason` | enum? | `dismissed`, `session_end`, `idle`, `failed`, `safety`, `orphaned` |

**Relationships:** a lead has 0..N members (`parent_id`). A member has exactly one lead. Members have no members (FR-048).

**State machine:**

```text
            ┌──────────── restart (transient, < limit) ───────────┐
            ▼                                                     │
pending ─▶ starting ─▶ ready ⇄ busy ─▶ stopping ─▶ closed         │
   │          │          │      │                                 │
   │          └──────────┴──────┴─▶ crashed ──────────────────────┘
   │                                  │
   └─ scope/spawn error ─▶ failed ◀───┘ (safety / config / unknown / limit)
```

- `pending` — row written, `scope.json` not yet fsynced.
- `starting` — container created; waiting for `agent.ready`.
- `ready` / `busy` — idle vs. processing a turn (from agent events).
- `stopping` — graceful stop signalled; grace window running.
- `crashed` — exited non-zero; exit policy decides restart or `failed`.
- `closed` / `failed` — terminal; directory retained.

### Scope

Serialized to `scope.json` (schema: [`contracts/scope.schema.json`](./contracts/scope.schema.json)).

| Field | Type | Notes |
|---|---|---|
| `session_id` | ULID | |
| `derived_from` | enum | `config` (lead) \| `team_skill` \| `delegation` |
| `source_ref` | string | Skill name + version, or delegation id |
| `mounts[]` | `{host_path, container_path, mode: ro\|rw}` | Canonicalized host paths |
| `credentials[]` | string | Credential **names** only |
| `tools[]` | `{server, tools[] \| "*", policy: allow\|ask\|deny}` | |
| `limits` | `{memory_mb, pids, tmp_mb, max_turns, max_tokens}` | |
| `narrowed[]` | `{dimension, item, reason}` | What intersection removed |
| `hash` | sha256 | Of the canonical JSON excluding `hash` |

**Invariant:** for a member, every item in `mounts`, `credentials`, `tools` is contained in the lead's scope and outside the ceiling. Checked at write time and asserted by the adversarial suite.

### Delegation

| Field | Type | Notes |
|---|---|---|
| `id` | ULID | |
| `lead_id` | ULID | |
| `member_id` | ULID? | Set once spawned |
| `role` | string | |
| `task` | text | Initial task sent to the member |
| `scope_request` | JSON | As asked by the lead |
| `policy` | enum | `allow` \| `ask` \| `deny` (resolved) |
| `approval_id` | ULID? | When `ask` |
| `timeout_s` | int | Per-reply deadline |
| `waiting_since`, `deadline_at` | timestamp | Supervisor wall clock, persisted; re-armed on restart (a monotonic clock does not survive a restart) |
| `status` | enum | see below |
| `failure_reason` | enum? | `denied`, `limit`, `scope_empty`, `spawn_error`, `timeout`, `crashed`, `safety` |

**State machine:**

```text
requested ─▶ awaiting_approval ─▶ spawning ─▶ active ⇄ waiting_reply ─▶ dismissed
     │               │                │          │           │
     └─▶ denied ◀────┘                └──────────┴───────────┴─▶ failed
```

`waiting_reply` arms the deadline; a member reply returns to `active` and clears it.

### Approval

| Field | Type | Notes |
|---|---|---|
| `id` | ULID | |
| `code` | string | 4-char user-facing code |
| `root_session_id` | ULID | Channel it is shown on |
| `requesting_session_id` | ULID | Lead or member that asked |
| `kind` | enum | `tool_call` \| `spawn` |
| `payload` | JSON | Tool + args, or role + task + scope request |
| `status` | enum | `pending` → `approved` \| `denied` \| `timed_out` \| `expired` |
| `created_at`, `resolved_at` | timestamp | |
| `reissued_as` | ULID? | Set when expired on restart and re-issued |

### Message (`inbound.db`, `outbound.db`)

| Field | Type | Notes |
|---|---|---|
| `id` | integer | Autoincrement, per file |
| `kind` | enum | see below |
| `body` | JSON | Kind-specific |
| `created_at` | timestamp | |

**Acknowledgement:** there is no `acked_at` column, because the agent may not write `inbound.db` (single writer). Instead the agent records the last processed inbound id in `outbound.db.cursor` within the same transaction as the reply. A message is acknowledged when `outbound.cursor.last_inbound_id >= message.id`. On restart the agent resumes from that cursor.

**Inbound kinds:** `user_message`, `approval_result`, `delegation_result`, `member_reply`, `member_failed`, `task` (members), `control` (`shutdown`).

**Outbound kinds:** `reply`, `approval_request`, `delegation_request`, `send_to_member`, `dismiss`.

### Event (`events.db`, `supervisor.db.supervisor_events`)

Schema: [`contracts/events.schema.json`](./contracts/events.schema.json).

| Field | Type |
|---|---|
| `seq` | integer, strictly increasing per file |
| `ts` | RFC 3339 |
| `ts_mono` | integer ns |
| `session_id`, `root_session_id` | ULID |
| `agent_role` | string |
| `source` | `agent` \| `supervisor` |
| `type` | string from the closed set in the schema |
| `data` | object, per type |

### Team skill

Markdown with YAML front-matter. Format: [`contracts/team-skill.md`](./contracts/team-skill.md). Parsed by the supervisor at install time into `supervisor.db.team_roles`; the Markdown body is mounted read-only into member containers of that role as their instructions.
