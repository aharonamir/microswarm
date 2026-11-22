# Contract: Console HTTP additions (005)

All routes inherit 002's request rules (`specs/002-console/contracts/console-http.md`): Host allowlist, cookie, `X-MS-Console` on POST, errors as `{"error","message"}`, and `503 supervisor_down`. Every route here is **GET and read-only**. Path ids must pass `valid_session_id`; otherwise the response is `400 invalid_session_id`. An unknown session returns `404 not_found`.

Times are RFC 3339 UTC on the wire. The client formats them as local `HH:MM` (FR-U16). Durations are integer milliseconds.

## Home

`GET /api/v1/home` → `200 HomeView` (data-model §HomeView)

```json
{ "summary": {"needs_you": 1, "running": 2},
  "needs_you": [{"approval_id":"01…","code":"K7QF","session_id":"01…","session_title":"Audit the auth module",
                 "agent":{"id":"01…","kind":"member","role":"tester","colour_index":2,"initial":"T"},
                 "sentence":{"key":"step.shell.ask","args":{"command":"cargo test -p auth"}},
                 "runs_in":{"folders":[{"path":"/work","mode":"ro"}],"network":"none","credentials":[],"tools_ask":["shell.run"]},
                 "expires_at":"2026-11-19T12:17:00Z"}],
  "running": [...], "ended_today": [...],
  "system": {"containers":5,"approvals_today":3,"supervisor_started_at":"…"} }
```

## Team

`GET /api/v1/sessions/{id}/team` → `200 { "groups": [TeamGroup], "agents": [Agent] }`

`{id}` may be the root or any member; the response always describes the root session. `agents` lists every agent in the root session (lead first) for avatars and colours.

## Steps

`GET /api/v1/sessions/{id}/steps?agent=<id>&after_seq=<n>&before_seq=<n>&limit=<1..200>&filter=everything|decisions|problems`

→ `200 { "steps": [Step], "phases": [Phase], "next_before_seq": n|null, "last_seq": n }`

- Defaults: all agents of the root, newest 200, `filter=everything`.
- `after_seq` returns steps strictly after that merged seq, oldest first, for the live tail.
- `phases` covers only the phases touched by the returned steps.

## Timeline

`GET /api/v1/sessions/{id}/timeline?since=<rfc3339>` → `200 Timeline` (data-model §Timeline)

`since` crops the range for the session-page strip. Omit it for the full session.

## Step detail

`GET /api/v1/sessions/{id}/steps/{step_id}` → `200 { "step": Step, "position": n, "total": n, "explanation": Explanation, "events": [raw event objects] }`

`position` is the one-based position in the complete derived history; `total` counts the complete history, independent of the currently loaded page or filter.

`events` is the only place raw event JSON appears outside Export.

## Member reach

`GET /api/v1/members/{id}/reach` → `200 Reach` (data-model §Reach)

`GET /api/v1/sessions/{id}/reach` returns the same shape for a lead. For a lead, `derivation` is `null` and `reduced` is empty.

## Member report

`GET /api/v1/members/{id}/report` → `200 { "text": string|null, "at": rfc3339|null, "status": "done"|"partial"|"blocked"|null }`

`text` is the member's last `delegation.reply` text, truncated to 20 KB. `status` is parsed from a leading `Status:` line when present. It is rendered as plain text.

## Approval detail (extended)

`GET /api/v1/approvals/{id}` gains these fields. Existing fields are unchanged.

```json
{ "approval": { "...002 fields...": "…",
  "agent": {"id":"…","kind":"member","role":"tester","colour_index":2,"initial":"T"},
  "sentence": {"key":"step.shell.ask","args":{"command":"cargo test -p auth"}},
  "brief": {"text":"Run the auth test suite and report failures. No file changes.","from":"lead","at":"…"},
  "policy": {"rule":"shell.run = ask","source":"team_skill","source_ref":"code-review-team@1.2.0"},
  "facts": [{"key":"parse.simple"},{"key":"parse.program","args":{"program":"cargo"}},{"key":"parse.no_pipes"}],
  "runs_in": ReachSummary,
  "spawn_preview": Reach|null } }
```

`spawn_preview` is filled only for spawn approvals. It is computed with `ms_proto` scope intersection over the request, the lead scope and the ceiling, without writing anything.

## Messages (extended)

`GET /api/v1/sessions/{id}/messages` items gain two optional fields:
- `based_on_role`: set when the lead message was produced in the turn that received a member `delegation.reply`;
- `team_group_id`: set on a `kind:"team"` anchor item where a TeamGroup is rendered. This replaces 002's `team.detail` text, which stays for compatibility.

## Caching

The steps, timeline and team views are memoised per root session, keyed by the newest merged seq, with at most 32 sessions in an LRU. A request whose key matches is served from memory.

## Security tests (required for every route above)

- Hostile strings in titles, tasks, roles, tool names, args and reports come back as JSON strings and render as text (e2e `hostile` fixture).
- A `Host` outside the allowlist gives `421`; a missing cookie gives `401`.
- Bad ids (`../`, overlong, lowercase) give `400`. Nothing reads outside `$MS_HOME/sessions/<valid id>/out/events.db`.
