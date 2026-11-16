# Implementation Plan: MicroSwarm Console

**Branch:** `002-console` · **Date:** 2026-11-01 · **Spec:** [spec.md](./spec.md) · **Design:** [design/README.md](./design/README.md)

## Summary

A new crate, `ms-console`, contains a small hyper HTTP server plus a no-build, vanilla-JS single-page app embedded in the binary. `microswarm console` (in `ms-cli`) starts it.

The server is a thin adapter:
- **Writes** go through existing supervisor socket ops (`chat`, `wait`, `approve`, `deny`, `creds_set`, `skills_install`) and one new op, `end_session`.
- **Reads** open `supervisor.db`, session databases and config read-only, the way the CLI already does.
- **Live updates** come from one `event_stream` follow connection, fanned out to browsers over SSE.

Before any console code, the plan frees line budget in `ms-supervisor`. It is at 5,993 / 6,000 (review 2026-10-30), so session-lifecycle helpers move into `ms-runtime`. The trace builder and doctor checks move into shared library code so the CLI and console use one implementation.

## Technical context

| | |
|---|---|
| **Language** | Rust (workspace toolchain); browser: ES2022 modules, no transpile |
| **Server** | `hyper` 1 http1 + `hyper-util` + `http-body-util` on Tokio — all already in the tree; no new runtime, no new TLS stack |
| **Storage access** | `rusqlite` read-only (`SQLITE_OPEN_READ_ONLY`), same as `ms-cli trace` |
| **Supervisor access** | Unix socket `$MS_HOME/supervisor.sock`, newline-delimited JSON (as `ms-channels::supervisor_rpc`) |
| **Frontend** | `crates/ms-console/web/`: `index.html`, `app.js` + modules, `styles.css`, `i18n/{en,zh}.json`, `fonts/` (IBM Plex Sans/Mono woff2, OFL). Embedded with `include_bytes!`; no npm, no bundler |
| **Live updates** | SSE (`text/event-stream`) from the console server; browser `EventSource` |
| **Testing** | Rust contract/integration tests for every endpoint and the security headers; a source lint for DOM sinks; optional Playwright + axe job for e2e/a11y (separate CI job, not in the core four commands) |
| **Targets** | Same as 001 (Linux x86_64/aarch64). Browser: current Chromium, Firefox, Safari |
| **Constraints** | SC-C05 budgets; loopback only; CSP with no inline script/style; no credential values anywhere |

## Constitution check

Requires amendment **1.1.0** (adds Principle IX; see `/.specify/memory/constitution-amendment-1.1.0.md`). Apply it in T206 before any console code.

| # | Gate | Status | Note |
|---|------|--------|------|
| I | Audit budget | ✅ after T212 | Supervisor headroom created by extracting lifecycle helpers to `ms-runtime`; console gets its own budget (IX) |
| II | Isolation | ✅ | Console never touches containers or the runtime socket |
| III | Authority | ✅ | All actions go through supervisor ops; console computes no scope, spawns nothing |
| IV | Scope | ✅ | Scope page displays stored values; no recomputation (FR-C50) |
| V | Boundaries | ✅ | Console is a separate process speaking the socket protocol |
| VI | Events | ✅ | `end_session` emits `session.closed` at source in the supervisor; console emits no MicroSwarm events |
| VII | Fail closed | ✅ | Bad Host/Origin/cookie → reject; supervisor down → explicit error, no stale-as-live; non-loopback → refuse to start |
| VIII | Dumb core | ✅ | Nothing added to the agent |
| IX | Operator surfaces | ✅ | Defined by the amendment; enforced by tests T216–T219 and lint T210 |

## Project structure

```text
specs/002-console/
├── spec.md · plan.md · research.md · data-model.md · quickstart.md · tasks.md
├── design/README.md                    # screens, tokens, copy (from the design canvas)
└── contracts/
    ├── console-http.md                 # REST + SSE API
    ├── console-events.schema.json      # SSE payloads
    └── supervisor-rpc-additions.md     # end_session; shared-library extractions

crates/
├── ms-runtime/src/
│   ├── lifecycle.rs      # + stop_members_for_lead, stop_idle_lead, graceful_stop_session (moved), end_session
│   ├── profile.rs        # NEW: container_profile() — single source for the container spec and the UI text
│   ├── doctor.rs         # NEW: structured doctor checks (moved from ms-cli)
│   └── trace.rs          # NEW: trace builder (moved from ms-cli/src/trace.rs)
├── ms-supervisor/src/service.rs        # dispatch gains one arm: "end_session"
├── ms-cli/src/main.rs                  # + "console" subcommand; doctor/trace call ms-runtime
└── ms-console/                         # NEW
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs        # run(home, listen) entry
    │   ├── server.rs     # hyper service, routing, headers
    │   ├── auth.rs       # startup token, cookie, Host/Origin/X-MS-Console checks
    │   ├── rpc.rs        # supervisor socket client (request/response + follow stream)
    │   ├── read.rs       # read-only views over supervisor.db + session files
    │   ├── status.rs     # status-word computation (data-model table)
    │   ├── chat.rs       # async chat/wait jobs, per-session in-flight lock
    │   ├── events.rs     # event_stream follower, ring buffer, SSE fan-out
    │   └── assets.rs     # embedded web/ files with content types
    ├── web/
    │   ├── index.html · styles.css · app.js
    │   ├── js/ (router.js, api.js, sse.js, dom.js, i18n.js, pages/*.js)
    │   ├── i18n/en.json · i18n/zh.json
    │   └── fonts/ (IBM Plex Sans 400/500/600, Plex Mono 400/500 woff2 + OFL.txt)
    └── tests/ (auth.rs, api_*.rs, sse.rs, xss.rs, no_secrets.rs)
tests/e2e/console/ (optional Playwright + axe; separate CI job)
scripts/loc-budget.sh   # + console Rust and web JS/CSS budgets
scripts/web-lint.sh     # NEW: fails on forbidden DOM sinks in web/
```

## Architecture

```text
Browser ──HTTP/SSE (127.0.0.1:8790, cookie)──▶ ms-console
                                             ├─ socket ops ─▶ microswarmd ─▶ containers
                                             ├─ 1× event_stream follow ◀─ microswarmd
                                             └─ read-only ─▶ supervisor.db, sessions/*/{control,out}/*.db, config/*.toml
```

**Login:** `microswarm console` generates a 32-byte random token (hex) and starts the server. It prints `http://127.0.0.1:8790/login?token=…`. `GET /login` checks the token in constant time, marks it used, sets the `ms_console` cookie (random 32-byte session id, server-side map, 12 h idle expiry) and redirects to `/` with no token in the URL. Restarting the console invalidates all cookies.

**Chat job:** `POST /api/v1/sessions/{id}/messages` takes the session's in-flight lock (in memory; the supervisor's own busy claim is the backstop), spawns a task that calls `chat`, and returns 202. When `chat` returns:
- **a reply:** publish `chat.reply`;
- **`pending`:** publish `chat.pending`, then call `wait` and publish the final reply;
- **an error:** publish `chat.error`.

The lock is released on completion. Two tabs see the same lock state through `GET /api/v1/sessions/{id}`'s `in_flight` field and through SSE.

**Event fan-out:** one task holds the `event_stream` follow connection, reconnecting with the last per-session cursor map. Each received event gets a console sequence number, goes into the ring buffer, and is broadcast to subscribers (`tokio::sync::broadcast`). Derived `status.changed` events are computed by re-reading the affected session rows from `supervisor.db` after each supervisor event.

**Rendering rule:** `web/js/dom.js` exports `el(tag, attrs, ...children)`. Children that are strings become `Text` nodes, and attributes are set with `setAttribute`, rejecting `on*` and `href`/`src` values with schemes other than relative or `https:`. This helper is the only way pages build DOM, and `scripts/web-lint.sh` enforces it.

## Phases

- **Phase 0 — Research:** R-C1…R-C6 in [research.md](./research.md). Exit: each has a decision with evidence.
- **Phase 1 — Setup and room:** amendment, crate skeleton, budgets and lint, supervisor extraction, `end_session`, shared trace/doctor/profile. Exit: the four CI commands green, budgets pass, no behaviour change in 001 tests.
- **Phase 2 — Foundational console:** server, auth, headers, RPC client, read views, status words, SSE relay, DOM helper, i18n, tokens. Exit: auth/headers/XSS/no-secrets tests green against stub pages.
- **Phases 3–8 — Stories US1…US6** in priority order. Each ends at a checkpoint with its acceptance scenarios passing.
- **Phase 9 — Polish:** zh review, a11y, the e2e job, docs, quickstart run.

## Complexity tracking

| Deviation | Why | Simpler alternative rejected because |
|---|---|---|
| New crate and process (`ms-console`) | Keeps HTTP, browser security and assets out of the audited supervisor | Adding HTTP to `microswarmd` breaks the supervisor budget and widens the authority's attack surface |
| Moving code from `ms-supervisor`/`ms-cli` into `ms-runtime` | Supervisor has 7 lines of headroom; trace and doctor need one shared implementation | Duplicating trace/doctor in the console would let the UI and CLI disagree |
| Vendored fonts (≈ 250 KB) | CSP `default-src 'self'`; offline hosts; matches design | System font stack is the fallback if the owner rejects vendoring (R-C4) |
