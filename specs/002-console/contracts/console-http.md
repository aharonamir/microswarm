# Contract: Console HTTP API

Base: `http://127.0.0.1:8790` (configurable port, loopback only). All `/api/v1/*` responses are `application/json; charset=utf-8` unless noted. Every response carries the headers in FR-C08.

## Request rules (all routes except `GET /login` and static assets after login)

| Check | Failure |
|---|---|
| `Host` ∈ {`127.0.0.1:<port>`, `localhost:<port>`} | `421 Misdirected Request` `{"error":"host_not_allowed"}` |
| Cookie `ms_console` maps to a live console session | `401` `{"error":"login_required"}`. HTML routes return the plain login-help page instead |
| POST: `Content-Type: application/json` | `415` |
| POST: `Origin` equals `http://<Host>` | `403` `{"error":"origin_mismatch"}` |
| POST: header `X-MS-Console: 1` | `403` `{"error":"missing_console_header"}` |
| Body ≤ 64 KiB | `413` |

Errors use `{"error": "<code>", "message": "<human text, en>"}`. The frontend maps `error` codes to i18n strings.

When the supervisor is unreachable, the response is `503 {"error":"supervisor_down"}`.

## Auth

| Method | Path | Behaviour |
|---|---|---|
| GET | `/login?token=<hex>` | If the token is valid and unused: mark it used, set `Set-Cookie: ms_console=<id>; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200`, and return `303 Location: /`. Otherwise return `401` with the help page ("Run `microswarm console` again for a new link") |
| POST | `/api/v1/logout` | Drop the console session; `204` |

## Static

`GET /`, `/app.js`, `/js/*`, `/styles.css`, `/i18n/*.json`, `/fonts/*` serve embedded assets with correct content types, and `Cache-Control: no-cache` plus an `ETag`. Unknown paths return `404`. SPA routing uses the URL hash (`#/sessions/<id>`), so no server fallback is needed.

## Overview

`GET /api/v1/overview` → `200`
```json
{ "supervisor": "running", "runtime": "docker 29.1.3", "containers": 4,
  "pending_approvals": 2, "lang": "en", "console_version": "0.2.0" }
```

## Sessions and chat

| Method | Path | Request | Response |
|---|---|---|---|
| GET | `/api/v1/sessions?group=active\|ended` | — | `200 {"sessions":[SessionView]}`. `ended` means closed within the last 24 h |
| GET | `/api/v1/sessions/{id}` | — | `200 {"session":SessionView,"team":[TeamMemberView],"banner":{"words","detail","tone","approval_id?"}}`; `404` |
| GET | `/api/v1/sessions/{id}/messages?before=<cursor>&limit=100` | — | `200 {"items":[ConversationItem],"next_before":"<cursor>\|null"}` |
| POST | `/api/v1/sessions` | `{"text":"..."}` (1–8,000 chars) | `202 {"session_id":null,"request_id":"<ulid>","thread_id":"<ulid>"}`. The session id arrives in `chat.reply`/`chat.pending` `data.session_id` |
| POST | `/api/v1/sessions/{id}/messages` | `{"text":"..."}` | `202 {"request_id":"<ulid>"}`; `409 {"error":"session_busy"}`; `409 {"error":"session_closed"}` |
| POST | `/api/v1/sessions/{id}/end` | `{}` | `202 {"request_id"}`. The result arrives as SSE `supervisor` events with `session.closed`. `409` if not a lead or already closed |

`id` must match `^[0-9A-HJKMNP-TV-Z]{26}$`, otherwise `400`. The id is never used to build a path before this check.

## Approvals

| Method | Path | Response |
|---|---|---|
| GET | `/api/v1/approvals?state=pending\|decided\|expired` | `200 {"approvals":[ApprovalView]}` (decided = last 24 h) |
| GET | `/api/v1/approvals/{id}` | `200 {"approval":ApprovalView}`; `404` |
| POST | `/api/v1/approvals/{code}/approve` | `200 {"decision":"approved"}`; `409 {"error":"already_resolved"}`; `410 {"error":"expired"}`; `404` |
| POST | `/api/v1/approvals/{code}/deny` | same as approve, with `"denied"` |

`code` must match `^[A-Z0-9]{4}$`. The console sends only `{"op":"approve"|"deny","code":...}` to the supervisor (FR-C32).

## Members, trace, host

| Method | Path | Response |
|---|---|---|
| GET | `/api/v1/members/{id}/scope` | `200 {"scope":ScopeComparison}`; `404`; `400` if it is a lead (a lead's page shows only its granted scope plus its profile: `GET /api/v1/sessions/{id}/scope`) |
| GET | `/api/v1/sessions/{id}/scope` | `200 {"granted":Scope,"profile":ContainerProfile}` |
| GET | `/api/v1/sessions/{root}/trace` | `200 {"sessions":[...],"events":[...]}`, identical in events to `microswarm trace <root> --json` |
| GET | `/api/v1/host` | `200 {"host":HostView}` |
| POST | `/api/v1/credentials/{name}` | `{"value":"..."}` → `204`. `name` must match the credential-name pattern. The value is never echoed, logged or included in errors |
| POST | `/api/v1/skills/install` | `{"path":"/abs/path"}` → `200 {name, version, roles}`; errors as the supervisor returns them (P3) |

## Events (SSE)

`GET /api/v1/events` with `Accept: text/event-stream`, plus `Last-Event-ID` on reconnect.

- **Response:** `200`, `Content-Type: text/event-stream`, and a `: ping` comment every 15 s.
- **Frame:**
  ```
  id: <console seq>
  event: <type>
  data: <one-line JSON, see console-events.schema.json>
  ```
- **On reconnect:** if `Last-Event-ID` is still in the ring, every later frame is replayed. Otherwise a single `event: resync` is sent first, and the client must refetch its snapshot.
- **Supervisor socket loss:** sends `supervisor.down`, then `supervisor.up` once it reconnects.
