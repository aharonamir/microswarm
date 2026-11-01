# Tasks: MicroSwarm Console

**Input:** [spec.md](./spec.md) · [plan.md](./plan.md) · [research.md](./research.md) · [data-model.md](./data-model.md) · [contracts/](./contracts/) · [design/README.md](./design/README.md) · [quickstart.md](./quickstart.md)

**Format:** `[ID] [P?] [Story] Description — path`. Tests come first: a test task's tests compile, are marked `#[ignore = "red: T2xx — reason"]`, and fail before their implementation task starts. Rules in `AGENTS.md` apply unchanged.

## Current execution status

- Not started. `001-microswarm` complete at `6108308` (2026-11-01).

## Open questions for the owner (do not block; defaults in brackets)

- **Q1** Mirror console-sent replies to the session's origin channel? [No, v1. Say so in the UI.]
- **Q2** Should the supervisor reject a second message while a session is `busy`? [No change in 002. The console enforces its own lock (R-C5).]
- **Q3** Vendor IBM Plex fonts (≈ 250 KB, OFL)? [Yes. Fallback is system fonts (R-C4).]
- **Q4** May `ms-console` depend on the `ms-agent` crate to show the parsed shell structure? [No direct dependency: T205 showed it brings model/MCP clients. Extract the pure parser into a small shared crate for both agent and console.]

---

## Phase 0 — Research (blocking)

- [x] T201 Verify R-C0: re-read the state and confirm the ops list, socket mode and current `loc-budget.sh` numbers; record them — `research.md`
- [x] T202 [P] Verify R-C1: dump the inbound/outbound kinds from a real two-member session; confirm the merge order and the member-reply rule — `research.md`
- [x] T203 [P] Verify R-C3: build a fixture home with 50 sessions and 20,000 events; measure `event_stream` follow CPU over 60 s; record the result and decide on T222 — `research.md`, `crates/ms-bench/`
- [x] T204 [P] Verify R-C7: list the exact functions and line counts to move; predict the supervisor budget after the move — `research.md`
- [x] T205 [P] Verify R-C6 and Q4: confirm `ms_agent::permit::evaluate_shell` is callable from another crate without pulling container-only dependencies — `research.md`

**Checkpoint A:** every ⚠ item in `research.md` has a recorded decision. **Stop and report.**

## Phase 1 — Setup and making room

- [x] T206 Apply constitution amendment 1.1.0 (Principle IX + gate row); add `specs/002-console/` to the "Source of truth" line in `AGENTS.md` and set the 002 checkpoint rule — `.specify/memory/constitution.md`, `AGENTS.md`
- [x] T207 [P] Update the stale status note at the top of `specs/001-microswarm/tasks.md` to "complete (2026-11-01)" — `specs/001-microswarm/tasks.md`
- [x] T208 Create the `crates/ms-console` skeleton (lib + empty `web/`) and add it to the workspace; add `microswarm console` to the `ms-cli` help, returning "not implemented" — `Cargo.toml`, `crates/ms-console/`, `crates/ms-cli/src/main.rs`
- [x] T209 [P] Extend `scripts/loc-budget.sh` with `ms-console` Rust ≤ 4,000 and `crates/ms-console/web` JS + CSS ≤ 3,500 — `scripts/loc-budget.sh`
- [x] T210 [P] Add `scripts/web-lint.sh` (forbidden DOM sinks; inline `<script>`/`style=` in `index.html`) and call it from CI — `scripts/web-lint.sh`, `.github/workflows/ci.yml`
- [x] T211 [P] Write red tests for the extractions: lifecycle behaviour unchanged (reuse 001 tests), trace JSON identical, doctor output snapshot, container profile ⇔ create body — `crates/ms-runtime/tests/`, `crates/ms-cli/tests/`
- [x] T212 Move the lifecycle helpers from `ms-supervisor/src/service.rs` to `ms-runtime/src/lifecycle.rs`; put before/after budget numbers in the commit message (gate: supervisor + shared ≤ 5,850) — `contracts/supervisor-rpc-additions.md` §2
- [x] T213 [P] Move the trace builder to `ms_runtime::trace`; the CLI keeps rendering — §3
- [x] T214 [P] Move the doctor checks to `ms_runtime::doctor`; the CLI prints them as before — §4
- [x] T215 Add `ms_runtime::profile::container_profile`, and derive the container create body from it — §5
- [x] T216 Write red tests for `end_session`: ends a lead with members; rejects a member id; rejects a closed session; denies pending approvals in the tree; idempotent retry; budget stays ≤ 6,000 — `tests/integration/c_end_session.rs`
- [x] T217 Implement `end_session` in `ms_runtime::lifecycle` plus one dispatch arm in `ms-supervisor` — §1

**Checkpoint B:** the four CI commands are green, the 001 suite is unchanged and green, `loc-budget.sh` passes with the new rows, and `web-lint.sh` passes. **Stop and report**, with the budget numbers.

## Phase 2 — Foundational console

### Tests first
- [ ] T218 [P] Auth tests: token is single-use and expires after 10 min; cookie attributes; no cookie → 401; wrong Host → 421; POST without Origin or with a mismatched Origin → 403; missing `X-MS-Console` → 403; wrong content type → 415; body > 64 KiB → 413; non-loopback `--listen` refuses to start — `crates/ms-console/tests/auth.rs`
- [ ] T219 [P] Header tests: exact CSP and the other FR-C08 headers on HTML, API and static responses — `crates/ms-console/tests/headers.rs`
- [ ] T220 [P] SSE tests: fan-out to 2 clients; `Last-Event-ID` replay within the ring; `resync` outside it; `supervisor.down`/`up` on socket loss; ping every 15 s — `crates/ms-console/tests/sse.rs`
- [ ] T221 [P] Status-word tests: every row of the data-model table, including "Ended by you" via `data.by` — `crates/ms-console/tests/status.rs`

### Implementation
- [ ] T222 (only if T203 fails SC-C03) Make the `event_stream` merge incremental with per-source high-water marks — `crates/ms-runtime/src/events.rs`
- [ ] T223 Server, routing, assets, headers and the `microswarm console` entry point — `crates/ms-console/src/{lib,server,assets}.rs`, `crates/ms-cli/src/main.rs`
- [ ] T224 Auth: token, console sessions, Host/Origin/header checks — `crates/ms-console/src/auth.rs`
- [ ] T225 [P] Supervisor RPC client: request/response, plus a follow stream with reconnect — `crates/ms-console/src/rpc.rs`
- [ ] T226 [P] Read-only views over `supervisor.db`, session dbs and config, with id validation before any path join — `crates/ms-console/src/read.rs`
- [ ] T227 [P] Status words — `crates/ms-console/src/status.rs`
- [ ] T228 Event follower, ring buffer, broadcast, SSE endpoint and derived `status.changed` — `crates/ms-console/src/events.rs`
- [ ] T229 [P] Frontend base: `index.html` (no inline script/style), `styles.css` tokens from `design/README.md`, `js/dom.js` (the only DOM builder), `js/router.js` (hash routes), `js/api.js` (adds `X-MS-Console`), `js/sse.js` (reconnect + resync), `js/i18n.js`; the global header and responsive frame — `crates/ms-console/web/`
- [ ] T230 [P] Vendor the fonts + OFL licence (per Q3) — `crates/ms-console/web/fonts/`
- [ ] T231 [P] Add `i18n/en.json` and `i18n/zh.json` with keys for every string in the design reference — `crates/ms-console/web/i18n/`
- [ ] T232 Adversarial tests: the fake model emits HTML, script, `javascript:` and data-URL payloads, and every API and SSE response carries them as inert JSON strings; a credential set via the API never appears in any response, SSE frame or log — `crates/ms-console/tests/{xss,no_secrets}.rs`

**Checkpoint C:** T218–T221 and T232 green. **Stop and report.**

## Phase 3 — US1: Chat with a lead (P1) 🎯 MVP

- [ ] T233 [P] [US1] Red API tests: sessions list and groups; session detail with `in_flight`; messages reconstruction and pagination (R-C1); POST message → 202 and SSE `chat.reply`; second send → 409; new session on channel `web`; `pending` → `wait` → final reply; end session → 202 and `session.closed` — `crates/ms-console/tests/api_sessions.rs`
- [ ] T234 [US1] Chat jobs with the per-session lock (FR-C22–C24) — `crates/ms-console/src/chat.rs`
- [ ] T235 [US1] Sessions and conversation endpoints — `crates/ms-console/src/read.rs`, `server.rs`
- [ ] T236 [US1] Sessions page: left rail with New session at the top; main column with header, status banner, labelled messages, team markers, the composer and its channel-aware helper text, End session with confirm; `aria-live` regions — `web/js/pages/sessions.js`
- [ ] T237 [US1] Quickstart §0–§1 against Docker and the fake model — `specs/002-console/quickstart.md`

## Phase 4 — US2: Approvals (P1)

- [ ] T238 [P] [US2] Red API tests: list by state; detail for tool_call (binding, args, parsed) and spawn (requested scope); approve/deny map to `approve`/`deny` with the code only; already-resolved → 409; expired → 410 — `crates/ms-console/tests/api_approvals.rs`
- [ ] T239 [US2] Approvals endpoints; parsed shell structure via `evaluate_shell` (R-C6) — `crates/ms-console/src/read.rs`, `server.rs`
- [ ] T240 [US2] Approvals page (queue + detail, phone layout with a sticky footer), inline approval cards in Sessions, and the header badge — `web/js/pages/approvals.js`, `web/js/pages/sessions.js`
- [ ] T241 [US2] Quickstart §2

**Checkpoint D (MVP):** US1 and US2 acceptance scenarios pass; quickstart §0–§2 pass; CI green. **Stop and wait for the owner** before US3.

## Phase 5 — US3: Live team (P2)
- [ ] T242 [P] [US3] Red tests: team view and reach summaries; `status.changed` within 500 ms of the supervisor commit (SC-C02, measured with the fixture from T203) — `crates/ms-console/tests/api_team.rs`
- [ ] T243 [US3] Team view, reach summaries and the latest-activity feed (agent events labelled as observations) — `crates/ms-console/src/read.rs`
- [ ] T244 [US3] "Who's working" panel and live updates; "Reconnecting…" state — `web/js/pages/sessions.js`

## Phase 6 — US4: Member scope (P2)
- [ ] T245 [P] [US4] Red tests: ScopeComparison equals the stored values; narrowed reasons; profile from `container_profile` — `crates/ms-console/tests/api_scope.rs`
- [ ] T246 [US4] Scope endpoints and the member page, including "Ask the lead to dismiss it" pre-filling the composer — `server.rs`, `web/js/pages/member.js`

## Phase 7 — US5: Trace (P2)
- [ ] T247 [P] [US5] Red test: the trace endpoint's events equal `microswarm trace --json` for the same fixture — `crates/ms-console/tests/api_trace.rs`
- [ ] T248 [US5] Trace endpoint and page: filters, selected-event JSON, copy, export — `web/js/pages/trace.js`

## Phase 8 — US6: Host (P3)
- [ ] T249 [P] [US6] Red tests: doctor parity with the CLI; credential names only; set-value never echoed — `crates/ms-console/tests/api_host.rs`
- [ ] T250 [US6] Host endpoint and page; credential dialog; skills install (path input) — `web/js/pages/host.js`

## Phase 9 — Polish
- [ ] T251 [P] `console-e2e` CI job: Playwright + axe at 1440 and 390 px on all pages; XSS check in a real browser; `MS_REQUIRE_RUNTIME=1` — `tests/e2e/console/`, `.github/workflows/console-e2e.yml`
- [ ] T252 [P] Native zh review of `i18n/zh.json` (same process as T099) — `docs/reviews/`
- [ ] T253 [P] Operator docs: starting the console, SSH-tunnel access, cross-channel behaviour (R-C8), the security model — `docs/console.md`
- [ ] T254 [P] Usability check for SC-C08 with 3 people; record it in `research.md` (R-C9)
- [ ] T255 Run quickstart §0–§6 and the adversarial section on a clean host; fix drift — `quickstart.md`
- [ ] T256 Adversarial review of `ms-console` against gates I–IX; record findings — `docs/reviews/`

**Checkpoint E:** all stories pass, SC-C01…C08 recorded, CI and `console-e2e` green. **Stop and report.**

## Dependencies

```text
Phase 0 ─▶ Phase 1 (T206–T217) ─▶ Phase 2 ─▶ US1 ─▶ US2 ─▶ [owner OK] ─▶ US3 ─▶ US4, US5 (parallel) ─▶ US6 ─▶ Polish
T212 before T216/T217 (budget). T213 before T247. T214 before T249. T215 before T245.
T203 decides whether T222 runs (before T228).
```
