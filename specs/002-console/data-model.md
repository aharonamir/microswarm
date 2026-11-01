# Data Model: MicroSwarm Console

The console stores no MicroSwarm state. Everything below is a **view model**, computed per request from read-only sources, plus the console's own in-memory state.

## Sources

| Source | Opened as | Used for |
|---|---|---|
| `$MS_HOME/supervisor.db` | read-only | sessions, delegations, approvals, team_roles, supervisor_events, `sessions.scope_json` |
| `$MS_HOME/sessions/<id>/control/inbound.db` | read-only (WAL sidecars persist, R-13) | conversation: user messages, member replies and failures, delegation and approval results |
| `$MS_HOME/sessions/<id>/out/outbound.db` | read-only | conversation: lead replies |
| `$MS_HOME/sessions/<id>/out/events.db` | read-only, **untrusted** | activity list and trace only |
| `$MS_HOME/config/*.toml` | read-only | host view: channels, tools, limits, ceiling, lang |
| supervisor socket | ops | `chat`, `wait`, `approve`, `deny`, `creds_set`, `creds_list`, `skills_install`, `end_session`, `event_stream` |

## Console in-memory state

| Item | Shape | Lifetime |
|---|---|---|
| Startup token | 32 random bytes, hex; `used: bool`; `expires_at` (+10 min) | process |
| Console sessions | `cookie_id → {created_at, last_seen}`; 12 h idle expiry | process |
| In-flight chats | `session_id → {request_id, started_at}` | until the chat job finishes |
| Event ring | last 2,000 `ConsoleEvent` with `seq` (u64, monotonic) | process |
| Follow cursor | `{session_id → merged seq}` from the last `event_stream` event | process; reconnect resumes |

## View models

### SessionView
```
id, root_id, title, kind: lead|member, role, channel, channel_thread,
status_words: StatusWords, member_count, open_approvals: int,
created_at, last_active_at, closed_at?, close_reason?, in_flight: bool
```
`title` is the first user message of the session, truncated to 60 chars, or "New session" if there is none.

### StatusWords
A pure function of `supervisor.db` rows. It never reads agent events (FR-C12). The first matching row wins.

| Condition | Words (en) | 中文 | Tone |
|---|---|---|---|
| pending approval whose `root_session_id` is this tree | Waiting for you | 等待你的确认 | attention |
| `status='starting'` | Starting | 正在启动 | neutral |
| `status='busy'` | Working | 正在处理 | active |
| lead `ready` and any delegation `waiting_reply` | Waiting on the team | 等待团队成员 | active |
| `status='crashed'` | Restarting after a crash | 崩溃后正在重启 | warn |
| `status='ready'` and `now - last_active_at` > 5 min | Idle for N min | 已空闲 N 分钟 | neutral |
| `status='ready'` | Ready for your next message | 可以继续发送消息 | ok |
| `closed`, `close_reason='session_end'` | Ended by you / Ended | 已由你结束 / 已结束 | neutral |
| `closed`, `idle` | Ended · idle | 已结束 · 空闲超时 | neutral |
| `closed`, `dismissed` | Finished | 已完成 | neutral |
| `failed`, `safety` | Stopped for safety | 已出于安全原因停止 | danger |
| `failed`, other | Stopped · <reason> | 已停止 · <原因> | danger |

"Ended by you" is used when the last `session.closed` supervisor event has `data.by = "operator"`.

### ConversationItem
```
{ kind: "user" | "lead" | "team" | "approval",
  at, text?,               // user/lead: plain text, untrusted for lead
  via?: channel,           // user: shown if not "web"
  from_role?: string,      // lead item relaying a member
  team?: {event, role, detail},      // team markers
  approval?: ApprovalView }          // inline approval
```

### TeamMemberView
```
session_id, role, kind, status_words, reach_summary: string,   // e.g. "Reads /work, writes /work/target. Asks before running commands."
narrowed_count, ended_reason_words?
```
`reach_summary` is generated from the granted scope:
- **Mounts:** "Reads X" for `ro`, "Edits X" for `rw`.
- **Credentials:** "can search the web" when the credential set includes `web-search`; otherwise "uses N credentials".
- **Tools:** "asks before <tool group>" for any `ask` policy.

### ApprovalView
```
id, code, state: pending|approved|denied|timed_out|expired, kind: tool_call|spawn,
root_session_id, requesting_session_id, requesting_role,
expires_at, created_at, resolved_at?, reissued_as?,
tool_call?: { server, tool, arguments (JSON), argument_hash,
              parsed?: {kind, commands[]} },        // R-C6, console-computed
spawn?:     { role, task, requested: ScopeLite },
runs_in:    { mounts[], credential_names[], network: "proxy only" }   // requesting session's granted scope
```

### ScopeComparison (member)
```
member_id, lead_id, derived_from, source_ref, scope_hash,
rows: [ { dimension: mounts|credentials|tools|limits|delegation,
          requested: [...], lead: [...], ceiling: [...], granted: [...],
          narrowed: [{item, reason}] } ],
profile: ContainerProfile
```
- **requested:** `delegations.scope_request` for that member. For team-skill spawns without a lead request, it is the role template from `team_roles.scope`.
- **lead:** the lead's `sessions.scope_json`.
- **ceiling:** `ceiling.toml`.
- **granted / narrowed:** the member's `sessions.scope_json`.

### ContainerProfile
Produced by `ms_runtime::profile::container_profile(&Scope) -> ContainerProfile`. The runtime calls the same function when it builds the container spec.
```
filesystem: ["root read-only", "/session/control read-only", "/session/out writable", "/tmp <n> MB tmpfs", mounts…],
process:    ["all capabilities dropped", "no-new-privileges", "pids ≤ <n>", "memory ≤ <n> MB"],
network:    ["session-internal network", "reaches: session proxy only", "secrets injected at the proxy"]
```

### TraceView
Exactly the `microswarm trace --json` output (the shared `ms_runtime::trace::build`), plus `sessions[]` with role, status words and scope summary for grouping.

### HostView
```
doctor:   [{check, result: pass|warn|fail, detail}],      // ms_runtime::doctor::run
channels: [{kind, detail, enabled}],
tools:    [{id, transport, command_or_url, tools[]}],
skills:   [{name, version, hash, roles: [{role, spawn_policy, scope_summary}]}],
credentials: [{name, used_by: [roles]}],                   // names only, from creds_list + scopes
limits:   {...}, ceiling: {paths[], credentials[], tools[]}
```

### ConsoleEvent (SSE)
See `contracts/console-events.schema.json`.
```
id: console seq (SSE id)
event: supervisor | agent | status.changed | chat.reply | chat.pending | chat.error | resync | supervisor.down | supervisor.up
data: JSON
```
