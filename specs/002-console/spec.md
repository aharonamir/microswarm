# Feature Specification: MicroSwarm Console (web frontend)

**Feature branch:** `002-console`
**Created:** 2026-11-01
**Status:** Draft — ready for planning
**Depends on:** `001-microswarm` complete (T001–T104 checked, 2026-11-01)
**Design reference:** [`design/README.md`](./design/README.md) (screens, tokens, copy)

**Input:** "Add a web frontend for operating MicroSwarm: chat with a session's lead, decide approvals, see who is working and what each container can reach, read the trace, and check the host. The web channel exists only as a webhook today; there is no UI."

---

## Context

MicroSwarm today is operated from the CLI (`microswarm chat|sessions|trace|approve|deny|skills|tools|creds|doctor`), Telegram, and a loopback-only `POST /messages` webhook. The supervisor exposes a Unix-socket RPC (`chat`, `channel_message`, `wait`, `approve`, `deny`, `skills_install`, `creds_set`, `creds_list`, `event_stream`), and the CLI reads `supervisor.db` and session files read-only for `sessions` and `trace`.

The console is an operator tool on the same host. It must not become a second authority: it does what the CLI can do, through the same socket, and reads the same state read-only.

---

## User scenarios & testing *(mandatory)*

### User Story 1 — Chat with a session's lead in the browser (Priority: P1) 🎯 MVP

The operator runs `microswarm console`, opens the printed one-time link, sees their sessions with a plain-language status each, opens one, reads the whole conversation, and messages the lead. They can also start a new session from the browser.

**Why this priority:** this is the gap the user felt first. Without it the console is a dashboard you can't act from.

**Independent test:** start the console against a home with one active Telegram session and the fake model. Open the link, open the session, send "hello", and see the lead's reply appear without reloading. Start a new session from the "New session" button and get a reply.

**Acceptance scenarios:**

1. **Given** the console is running, **When** the operator opens the printed link, **Then** a session cookie is set, the token leaves the URL, and the Sessions page shows every non-closed lead session with its title, channel, member count and status in words.
2. **Given** a session is open, **When** the page loads, **Then** the conversation shows the user's messages and the lead's replies from all channels in order, each labelled with sender and time. Member findings relayed by the lead are labelled "Lead · from <role>".
3. **Given** a session is open, **When** the operator sends a message, **Then** it appears immediately as "You", a "Lead is working…" indicator shows, and the lead's reply replaces the indicator when it arrives, without a page reload.
4. **Given** a message to that session is already in flight, **When** the operator sends another, **Then** the composer is disabled with "The lead is still answering your last message" and nothing is queued twice.
5. **Given** the operator presses **New session** and sends a first message, **Then** a new lead session is created on the `web` channel and opens.
6. **Given** a session belongs to Telegram, **When** the operator replies from the console, **Then** the lead receives it and the reply shows in the console. It is **not** mirrored to Telegram in v1, and the page says so under the composer.
7. **Given** the operator presses **End session** and confirms, **Then** members stop first, then the lead, and the session moves to "Ended today" with reason "Ended by you".

---

### User Story 2 — Decide approvals (Priority: P1)

The operator sees every pending approval, from any session and any agent, and approves or denies it knowing exactly what will run and where.

**Why this priority:** approvals block the team. The console's value over Telegram is showing the exact call and the scope it runs in.

**Independent test:** with a tester member waiting on `shell.run`, open Approvals, see the exact server, tool, arguments and binding hash, approve, and see the session continue.

**Acceptance scenarios:**

1. **Given** pending approvals exist, **When** any page loads, **Then** the top bar shows the pending count, and the Sessions list marks affected sessions "Waiting for you".
2. **Given** a tool-call approval, **When** it is opened, **Then** it shows the requesting role and member id, the root session, why it asks (the policy and where it came from), and the exact call: server, tool, arguments and `binding.argument_hash`. It also shows the parsed shell structure when present, and the requesting container's mounts, credential names and network.
3. **Given** a spawn approval, **When** it is opened, **Then** it shows the role, the task, and the requested mounts, credential names and tools. It notes that anything outside the lead's scope is narrowed before start.
4. **Given** the operator approves, **Then** the console calls `approve` with the code, the item moves to Decided, and the waiting channel (Telegram, CLI, or the console itself) receives the follow-up reply.
5. **Given** an approval expired on supervisor restart, **Then** it shows under Expired with "Sent again as <new code>".
6. **Given** the approval is shown inline in a session, **Then** Approve and Deny work there too, with the same result.
7. **Given** a 390 px wide screen, **Then** the approvals list and detail are usable with ≥ 44 px targets and no horizontal scroll.

---

### User Story 3 — See who's working, live (Priority: P2)

The session page shows the lead and each member with a status in words, what each can reach in one line, and the latest activity, all updating live.

**Independent test:** while a member is busy, its status changes from "Working" to "Waiting for you" within one second of the approval request, without a reload. Kill the console server's SSE connection, and the page reconnects and catches up.

**Acceptance scenarios:**

1. **Given** a session with members, **Then** the "Who's working" panel lists the lead and its members (closed ones collapsed with their end reason), each with a status from the status table in `data-model.md`.
2. **Given** an event arrives (spawn, status change, approval, exit), **Then** the affected rows and the status banner update within 500 ms p95 of the supervisor writing it.
3. **Given** the browser loses the event connection, **When** it reconnects with `Last-Event-ID`, **Then** missed events replay, or the page reloads its snapshot if the gap is too old.

---

### User Story 4 — Inspect what a member can reach (Priority: P2)

For any member, the operator sees how its scope was computed: requested ∩ what the lead holds ∩ never-grantable = granted, per dimension, with each narrowed item and reason, plus the container profile.

**Independent test:** for a member whose request included a path outside the lead's scope, the page shows that path struck through with "not in lead scope", and the granted column equals the member's stored `scope_json`.

**Acceptance scenarios:**

1. **Given** a member session, **Then** the page shows rows for mounts, credentials (names only), tools with policy, limits, and delegation (always "none" for members). The columns are Requested (the delegation's `scope_request`, or the team-skill role template), Lead holds (the lead's stored scope), Never grantable (the ceiling) and Granted (the member's stored scope).
2. **Given** `narrowed` entries exist, **Then** each is shown in Granted with its reason in words.
3. **Given** the page shows the container profile (filesystem, process, network), **Then** that text is generated from the same runtime code that builds the container, never hand-written in the frontend.

---

### User Story 5 — Read the trace (Priority: P2)

The operator reads a session's full event tree in the browser: the lead, then members, each event with time, description, decision and duration, plus the raw JSON of a selected event.

**Acceptance scenarios:**

1. **Given** a root session, **Then** the trace shows the same sessions and events as `microswarm trace <id> --json`, grouped lead-first, members indented.
2. **Given** filter buttons (All, Tools, Decisions, Delegation, Exits), **Then** filtering is client-side and instant.
3. **Given** an event is selected, **Then** its JSON is shown verbatim (escaped) and can be copied. **Export JSON** downloads the whole trace.

---

### User Story 6 — Check the host (Priority: P3)

The operator sees doctor results, channels, MCP servers, team-skill roles with spawn policy, credential names, and limits and ceiling. They can set a credential value without ever seeing one.

**Acceptance scenarios:**

1. **Given** the Host page, **Then** doctor checks show pass, warn or fail using the same checks as `microswarm doctor`.
2. **Given** **Set a value** for a credential name, **Then** the value goes to `creds_set`. No API response, event or page ever contains a credential value, and the field clears after submit.
3. **Given** limits, ceiling, channels and tools, **Then** they are read-only, with "edit on the host" guidance.

---

### Edge cases

- **Supervisor not running:** every page shows "The supervisor isn't running. Start it with `microswarmd`." and retries every 3 s. No stale data is shown as live.
- **Session ends while open:** the composer disables and shows the end reason.
- **Chat RPC times out (90 s)** with no reply: show "No reply yet. The lead may still be working — it will appear here when it arrives." The reply appears later from the session files or events.
- **Approval already resolved elsewhere** (Telegram, CLI): the approve call returns "already resolved"; the UI refreshes the item and says "Already decided on another channel".
- **Approval expired between load and click:** show "This request expired", refresh the list.
- **Agent output contains HTML, scripts or `javascript:` links:** shown as literal text, never executed or linked.
- **Very long sessions:** messages paginate (newest 100, "Show earlier").
- **Two browser tabs:** both receive events. Sending from one disables the composer in both (the in-flight state comes from the server).
- **Language toggle:** switches all console strings between en and zh without reload. Agent text is shown as-is.

---

## Requirements *(mandatory)*

### Functional requirements

**Process and access**

- **FR-C01:** The console MUST run as its own host process started by `microswarm console`, separate from `microswarmd`.
- **FR-C02:** It MUST act on MicroSwarm only through supervisor socket operations, and read only `supervisor.db`, session files and config files, all opened read-only.
- **FR-C03:** It MUST NOT connect to the container runtime socket, read credential values, or write any MicroSwarm state file.
- **FR-C04:** It MUST bind to loopback only (default `127.0.0.1:8790`). A non-loopback address MUST be refused at startup with guidance to use an SSH tunnel or a TLS-terminating proxy.
- **FR-C05:** Every request except `GET /login` MUST carry a valid session cookie obtained by exchanging a one-time startup token. The token is printed once by `microswarm console`, single-use, and expires after 10 minutes. The cookie is `HttpOnly; SameSite=Strict; Path=/`.
- **FR-C06:** The server MUST reject requests whose `Host` header is not `127.0.0.1:<port>` or `localhost:<port>`, as a DNS-rebinding defence.
- **FR-C07:** State-changing requests MUST be POST with `Content-Type: application/json`, an `Origin` equal to the console origin, and the header `X-MS-Console: 1`. Otherwise reject with 403.
- **FR-C08:** Every response MUST carry: `Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, and `Cache-Control: no-store` on `/api/*`.

**Untrusted content**

- **FR-C10:** All agent-originated content (replies, event data, tool arguments, member findings) MUST be treated as untrusted data. The frontend MUST render it only as text nodes and MUST NOT use `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `eval`, `new Function`, or string-based timers anywhere in `web/`.
- **FR-C11:** URLs inside agent content MUST NOT be turned into links in v1.
- **FR-C12:** Agent events MUST NOT drive any authorization or state decision in the console. Status words are computed from `supervisor.db`; agent events only feed the activity list and the trace (security review residual risk 1).

**Sessions and chat (US1)**

- **FR-C20:** List lead sessions grouped "Your sessions" (not closed) and "Ended today" (closed in the last 24 h), each with title, channel, member count and status words.
- **FR-C21:** The conversation MUST be reconstructed from the lead's `control/inbound.db` (`user_message`, `member_reply`, `member_failed`, `approval_result`, `delegation_result`) and `out/outbound.db` (`reply`, `approval_request`), ordered by time, paginated by 100.
- **FR-C22:** Sending MUST call supervisor `chat` with `session_id` for an existing session, or `channel:"web"` and a new `thread_id` (ULID) for a new session. It runs asynchronously; the result is pushed over SSE as `chat.reply`, `chat.pending` or `chat.error`.
- **FR-C23:** At most one in-flight chat per session across all tabs. A second send returns 409 `session_busy`.
- **FR-C24:** When `chat` returns `pending: true`, the console MUST call `wait` with the returned `session_id` and `after_id` and push the final reply when it arrives.
- **FR-C25:** **End session** MUST call the new supervisor op `end_session` (see `contracts/supervisor-rpc-additions.md`). Members stop first, then the lead, with `close_reason = "session_end"` and `data.by = "operator"`.

**Approvals (US2)**

- **FR-C30:** List approvals by state: pending, decided (last 24 h), and expired (with `reissued_as`).
- **FR-C31:** Show, for `tool_call`: requesting session and role, root session, `binding` (server, tool, `argument_hash`), the arguments, the parsed shell structure when present in the approval request body, and the requesting session's granted mounts, credential names and network. For `spawn`: role, task and requested scope.
- **FR-C32:** Approve and Deny MUST call `approve` / `deny` with the code and nothing else. The console MUST NOT construct, alter or re-bind approval payloads.
- **FR-C33:** Approvals MUST also render inline in the session conversation of their root session.

**Live updates (US3)**

- **FR-C40:** The console server MUST hold exactly one `event_stream` (`follow: true`) connection to the supervisor and fan it out to browsers over SSE (`GET /api/v1/events`).
- **FR-C41:** SSE events MUST carry a console sequence `id`. The server keeps a ring buffer of the last 2,000. A reconnect with `Last-Event-ID` inside the buffer replays from it; outside it, the server sends `resync` and the client reloads its snapshot.
- **FR-C42:** Status words MUST follow the table in `data-model.md`. A status change MUST reach open pages within 500 ms p95 of the supervisor commit.

**Scope, trace, host (US4–US6)**

- **FR-C50:** The member scope API MUST return requested, lead, ceiling, granted and narrowed exactly as stored. It MUST NOT recompute an intersection in the console.
- **FR-C51:** The container profile MUST come from a function in `ms-runtime` that is also used to build the container spec, with a test asserting both agree.
- **FR-C52:** The trace API MUST produce the same events as `microswarm trace --json`. The trace builder moves from `ms-cli` into a shared library used by both.
- **FR-C53:** The host API MUST reuse the doctor checks that `microswarm doctor` runs. The checks move to a shared library function returning structured results.
- **FR-C54:** Setting a credential MUST call `creds_set`. Credential values MUST never appear in any console response, SSE event, log line or error message.

**Language, accessibility, layout**

- **FR-C60:** All console UI strings MUST exist in English and Chinese, switchable at runtime. The choice persists in `localStorage`, guarded, and defaults to `[home] lang`.
- **FR-C61:** Pages MUST meet WCAG 2.1 AA: text contrast ≥ 4.5:1, every control reachable and operable by keyboard, a visible focus ring, labelled inputs, `aria-live="polite"` for the status banner and new replies, and targets ≥ 44 px on touch layouts.
- **FR-C62:** Every page MUST work from 390 px to 1440 px+. Side panels stack below the main column under 900 px.

### Key entities

See `data-model.md`: SessionView, StatusWords, ConversationItem, TeamMemberView, ApprovalView, ScopeComparison, ContainerProfile, TraceView, HostView, ConsoleEvent.

---

## Success criteria *(mandatory)*

- **SC-C01:** From `microswarm console` to the Sessions page rendered: ≤ 2 s on the reference host.
- **SC-C02:** Supervisor event commit to the browser DOM update: p95 ≤ 500 ms with 50 sessions and 20,000 stored events.
- **SC-C03:** With the console attached and the system idle, combined supervisor and console CPU ≤ 5 % of one core, measured over 60 s with 50 sessions and 20,000 events.
- **SC-C04:** The adversarial suite finds 0 executed scripts or followed `javascript:` URLs from agent content, and 0 credential values in any response, event or log.
- **SC-C05:** `scripts/loc-budget.sh` passes: `ms-supervisor` + shared ≤ 6,000; `ms-runtime` + shared ≤ 6,000; `ms-console` Rust ≤ 4,000; `crates/ms-console/web` JS + CSS ≤ 3,500 lines.
- **SC-C06:** Zero npm runtime dependencies and no frontend build step. The binary embeds the assets.
- **SC-C07:** An automated axe check finds no serious or critical violations on all six pages at 1440 px and 390 px.
- **SC-C08:** A first-time operator finds where to chat with the lead and what the session is waiting for within 10 seconds of opening a session. Validated with 3 people, recorded in `research.md`.

---

## Assumptions

- Single operator per host; the console is not multi-user and has no roles.
- The operator who starts `microswarm console` is the same Unix user that owns `$MS_HOME` and the supervisor socket (mode 0600).
- Docker remains the supported runtime (owner decision 2026-10-29).
- Remote access is by SSH tunnel or a TLS proxy the operator runs. The console never terminates TLS itself in v1.

## Out of scope (v1)

- Mirroring console-sent replies to the session's original channel.
- Editing configuration files, tools or policies from the browser (read-only; credentials are write-only).
- Talking to members directly (lead-only, R-10).
- Markdown or HTML rendering of agent output.
- Multi-user accounts, roles, OAuth or OIDC.
- Host-plane analytics, cost charts, memory.
