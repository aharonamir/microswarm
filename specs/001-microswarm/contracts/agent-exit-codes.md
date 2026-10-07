# Contract: agent exit codes and supervisor exit policy

The agent core MUST end every process lifetime by writing an `agent.exit` event and then exiting with one of these codes. The supervisor classifies by exit code, and cross-checks against the last `agent.exit` event when present.

| Reason | Exit code | Agent emits when | Supervisor action |
|---|---|---|---|
| `ok` | `0` | Received `control:shutdown`, flushed, exited cleanly | Mark `closed` |
| `transient` | `75` (`EX_TEMPFAIL`) | Model/proxy unreachable after in-turn retries; SQLite busy beyond timeout; MCP server crashed and could not restart | Restart with backoff 1 s / 2 s / 4 s, max 3; then `failed` |
| `safety` | `77` (`EX_NOPERM`) | Permission engine returned `deny` on an action the turn cannot proceed without; scope invariant violated at boot; `max_turns`/`max_tokens` exceeded | **Never restart.** Mark `failed`, `close_reason=safety` |
| `config` | `78` (`EX_CONFIG`) | `scope.json` missing/invalid/hash mismatch; required MCP server not in scope; model not configured | Never restart. Mark `failed`; notify operator |

## Classification rules (fail closed)

1. Exit code not in the table → `unknown` → treated as `safety`.
2. Killed by signal (OOM-kill `137`, `SIGSEGV`) → `transient` only if the runtime reports `OOMKilled=true`; any other signal → `safety`.
3. Exit `75` but the last `agent.exit` event says `safety` → `safety` (the stricter of the two wins).
4. A crash during the grace window of a `dismiss` or `session_end` → `closed`, not restarted.
5. A restart always resumes from `outbound.cursor.last_inbound_id`; the unacknowledged message is re-processed exactly once per restart.

## What the lead sees

For members, every non-`ok` terminal outcome writes to the lead's `inbound.db`:

```json
{ "kind": "member_failed",
  "body": { "member_id": "01J…", "delegation_id": "01J…", "reason": "safety" } }
```

`reason` ∈ `timeout | crashed | safety | config | spawn_error`. `crashed` means transient retries were exhausted.

## What the user sees

For leads, a terminal non-`ok` outcome sends a bilingual notice on the session's channel, e.g.:

> ⚠ The agent stopped because it was about to do something its permissions don't allow. Nothing was retried. Session `01J…`.
> ⚠ 智能体已停止：它即将执行权限不允许的操作。系统未进行重试。会话 `01J…`。
