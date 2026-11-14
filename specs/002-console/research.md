# Research: MicroSwarm Console

Phase 0 output. Verification on branch `002-console` is recorded below. The original source-read notes are retained where a later measurement corrected them.

## Phase 0 verification (2026-11-01)

### T201 / R-C0 — starting state

- `scripts/loc-budget.sh` printed `ms-agent + shared crates: 4443 / 6000`, `ms-supervisor + shared crates: 6000 / 6000`, `ms-runtime + shared crates: 3494 / 6000`. The draft's 5,993 supervisor figure is stale; there is **zero** current headroom. Source: `scripts/loc-budget.sh` (tokei code-line count).
- `crates/ms-supervisor/src/service.rs:31-39` binds `$MS_HOME/supervisor.sock` and sets mode `0o600`; lines 93 and 312-318 implement `event_stream`, `skills_install`, `creds_set`, `creds_list`, `approve`, `deny`, `chat`, `channel_message`, and `wait`. There is no `end_session` arm.
- `crates/ms-runtime/src/lib.rs:28-35` confirms `claim_session_busy` accepts both `ready` and `busy`; retain R-C5's console-side lock and Q2 default.

### T202 / R-C1 — conversation source and order

- With `MS_KEEP_TEST_HOME=1 MS_REQUIRE_RUNTIME=1`, `cargo test -p ms-supervisor --test us2-clean-slate -- --nocapture` passed and preserved `/tmp/microswarm-test-3886335-1793556538049372520`, a real lead plus two researcher member sessions. The lead had inbound kinds `user_message`, three `delegation_result`, `member_failed`; outbound kinds `delegation_request`, `dismiss`, `reply`. The two member sessions each had inbound `task` and outbound `reply` (the first also had inbound `control`). Timestamps across the two lead databases were distinct and interleaved; merge by `created_at`, with a fixed source/id tie-breaker.
- `cargo test -p ms-supervisor --test us2-send -- --nocapture` passed and preserved `/tmp/microswarm-test-3887433-1793556542945104995`. Its lead inbound `member_reply` at `18:09:03.725Z` held `role instructions reached model`; the next lead outbound `reply` at `18:09:03.784Z` held `fixture reply`. Source: the preserved `control/inbound.db` and `out/outbound.db` rows, queried read-only. These are distinct content. **Decision:** always show `member_reply` as its own “Lead · from <role>” item; do not attempt text de-duplication. The two-member transcript validates the kind set and merge; the member-reply transcript validates this rule.

### T203 / R-C3 — follower cost

- Added reproducible fixture runner `crates/ms-bench/src/bin/event_stream_cost.rs`: 50 session rows, 50 agent `events.db` files, 400 events per file (20,000 total). It calls the real `ms_runtime::events::stream` with `follow:true` and a sink for 60 seconds. Run: `cargo build --release -p ms-bench --bin event_stream_cost`, then `/usr/bin/time -f 'CPU user=%U s system=%S s elapsed=%e s maxRSS=%M KB' target/release/event_stream_cost 60`.
- Output: `elapsed=60.00s; stream=timed out as expected`; `CPU user=30.60 s system=0.68 s elapsed=60.03 s maxRSS=42948 KB`. CPU = `(30.60 + 0.68) / 60.03 = 52.1%` of one core for the follower alone, versus SC-C03's 5% combined target. `crates/ms-runtime/src/events.rs:72-229` confirms every 100 ms pass scans the full source history and merged ledger. **Decision: T222 is required** before SSE fan-out. The fixture measures the idle follower in isolation, so adding the console can only increase combined CPU.
- After T222, the same release fixture ran for 10 seconds: `elapsed=10.00s; stream=timed out as expected`; `/usr/bin/time` reported `CPU user=0.33 s system=0.14 s elapsed=10.03 s maxRSS=42944 KB`, or **4.7% of one core**. This short run meets 5% only for the isolated follower and leaves effectively no measured headroom for the console; retain one follower and remeasure the combined console path under T242.

### T204 / R-C7 — extraction budget and dependency boundary

- `crates/ms-supervisor/src/service.rs:124-308` contains `close_idle_leads`, `close_idle_members`, `stop_member_idle`, `stop_idle_lead`, `stop_members_for_lead`, `graceful_stop_session`. `tokei` counts **179 Rust code lines** in the full 124-308 region; the four `stop_*`/`graceful_*` helpers alone (183-308) are **122**, insufficient to meet the ≤5,850 T212 gate from today's 6,000. Move the idle query loops too.
- A direct copy is impossible: these functions use `ms_supervisor::config::ConfigSet` and `service::runtime_for`; `ms-runtime` cannot depend on `ms-supervisor` without a cycle. Pass a small runtime/idle settings value into `ms_runtime::lifecycle` instead. Budget prediction: 6,000 − 179 + approximately 10–20 service-side adapter lines = **5,831–5,841** supervisor + shared code lines, subject to T212's measured before/after check. Sources: `crates/ms-supervisor/src/config.rs:18`, `service.rs:1470-1472`, `crates/ms-runtime/src/lib.rs:305-310`, `scripts/loc-budget.sh`.

### T205 / R-C6 / Q4 — shell parser dependency

- `crates/ms-agent/src/lib.rs:9` exports `permit`; `permit.rs:156` exports `evaluate_shell`, and external integration test `crates/ms-agent/tests/permit.rs` imports it through `ms_agent::permit`. It is callable from another crate. `permit.rs:158-221` returns `parsed_structure` only for `Ask`; ordinary `Allow` commands and unparseable `Deny` commands have no `approval_prompt`. The UI must not claim a parsed structure in those cases.
- `crates/ms-agent/Cargo.toml` and `cargo tree -p ms-agent -e normal` show the library brings `rmcp` (including child-process and HTTP transports), `reqwest`, `hyper` and `tokio` (322 dependency-tree lines). These are the agent's model/MCP client dependencies, not needed to display a shell command. **Decision: no direct `ms-agent` dependency.** The Q4 default's concern was valid; implementation uses the dependency-free `crates/ms-shell` crate for both `ms-agent` and `ms-console`. The console recomputes display structure from the bound `shell.run` arguments; the supervisor's binding remains authoritative.

### Other R-C decisions

- **R-C2:** keep the Rust core gate and separate Playwright/axe job. `crates/ms-bench/Cargo.toml` already uses feature-gated optional performance tests; the console plan has no npm runtime dependency or asset build step.
- **R-C4 / Q3:** retain vendored Latin Plex fonts and system CJK fallback from `design/README.md`; no source finding contradicted this choice.
- **R-C5 / Q2:** retain the console in-memory lock; the exact `status IN ('ready','busy')` update is in `crates/ms-runtime/src/lib.rs:34`.
- **R-C8 / Q1:** retain no cross-channel mirroring. Telegram calls `wait` on pending replies (`crates/ms-channels/src/telegram.rs:206`); the supervisor socket `chat` reply is returned to the caller, and no Telegram broadcast exists in `service.rs`'s `chat` path.
- **R-C9:** the three-person usability check remains a Phase 9 human task against the implemented Sessions page.

---

## R-C0 — Starting state (source read, 2026-11-01)

- `001-microswarm`: all tasks checked (T001–T104). The status note at the top of `specs/001-microswarm/tasks.md` is dated 2026-10-29 and describes the US2 checkpoint; it is stale.
- Supervisor RPC (`crates/ms-supervisor/src/service.rs`, `dispatch`): `skills_install`, `creds_set`, `creds_list`, `approve`, `deny`, `chat`, `channel_message`, `wait`, plus `event_stream` on its own connection. There is no op to end a session.
- Socket: `$MS_HOME/supervisor.sock`, mode `0600`.
- Line budget (`docs/reviews/2026-10-30-agent-supervisor-review.md`): ms-agent 3,756; **ms-supervisor + shared 5,993 / 6,000**; ms-runtime 3,365.
- The CLI's `sessions` and `trace` read `supervisor.db` and session `events.db` read-only (`crates/ms-cli/src/{main,trace}.rs`).
- Security review residual risk 1: agent `events.db` is agent-writable, so treat its contents as untrusted observations.

## R-C1 — Rebuilding the conversation

**Decision:** read the lead session's `control/inbound.db` and `out/outbound.db` read-only and merge by `created_at`, then id. Show these kinds:

| Source | Kind | Shown as |
|---|---|---|
| inbound | `user_message` | "You" (with channel if not web) |
| inbound | `member_reply` | "Lead · from <role>" as a separate item |
| inbound | `member_failed` | Team marker "<role> ended · <reason in words>" |
| inbound | `delegation_result` | Team marker "<role> joined" / "couldn't start · <reason>" |
| inbound | `approval_result` | Team marker "You approved/denied <code>" |
| outbound | `reply` | "Lead" |
| supervisor.db | `approvals` where `root_session_id` = this lead | inline approval card at its `created_at`. This covers member requests, whose `approval_request` sits in the member's own outbound. The approval row, never the agent's outbound body, is what's displayed |
| outbound | `delegation_request`, `send_to_member`, `dismiss` | not shown (visible in the trace) |

**Verified in T202:** the de-duplication rule is unreliable; always show member replies as their own “Lead · from <role>” item.

## R-C2 — Testing the frontend without a build toolchain

**Decision:**
- **Core CI** (the four commands): Rust tests only. Endpoint contracts, headers, auth, SSE, XSS (the API returns agent HTML as inert JSON strings), and no-secrets checks. `scripts/web-lint.sh` greps `web/` for forbidden sinks (`innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `eval(`, `new Function`, `setTimeout("`, `setInterval("`) and for inline `<script>` or `style=` in `index.html`.
- **A separate `console-e2e` CI job:** Playwright (Chromium) plus `@axe-core/playwright` drives the embedded console server against controlled API/SSE fixtures at both required viewport sizes. The same job runs the fake-model Docker quickstart integration with `MS_REQUIRE_RUNTIME=1`; the browser tests exercise real HTTP/auth/static assets while avoiding model timing in accessibility checks.

**Rationale:** keeps SC-C06 (no npm runtime deps, no build step) while still testing behaviour and accessibility in a real browser.

## R-C3 — Event fan-out cost

**Finding:** `ms-runtime::events::stream` in follow mode rebuilds the merged ledger each 100 ms by reading every `supervisor_events` row and every session's `out/events.db`, then reconciling `event-stream.db`. Cost grows with total history.

**Decision after T203:** the console holds exactly one follower (FR-C40). The 50-session, 20,000-event measurement fails SC-C03; make the merge incremental in `ms-runtime` as T222. See the 52.1% CPU measurement above.

## R-C4 — Fonts under a strict CSP

**Decision:** vendor IBM Plex Sans (400/500/600) and IBM Plex Mono (400/500) as woff2 Latin subsets plus `OFL.txt` under `web/fonts/`, served from `'self'`. Chinese text uses the system CJK font (`"PingFang SC","Noto Sans CJK SC","Microsoft YaHei",sans-serif`); no CJK webfont.

**Fallback if the owner declines vendoring:** `system-ui` / `ui-monospace` stacks only. The layout must not depend on Plex metrics.

## R-C5 — One message in flight per session

**Finding:** `ms_runtime::claim_session_busy` updates `status='busy'` when the status is `'ready'` **or `'busy'`**. A second message while busy is therefore accepted by the supervisor, not rejected.

**Decision:** the console enforces one in-flight chat per session with its own in-memory lock (FR-C23) and returns 409 `session_busy`. Changing the supervisor's claim semantics is out of scope, because it would also affect CLI and Telegram behaviour. Note this as an open question for the owner (Q2 in tasks.md).

## R-C6 — Showing the parsed shell structure

**Finding:** the agent's `approval_request` body is `{approval_local_id, server, tool, args, binding}` (`crates/ms-agent/src/model.rs`). The `ParsedCommand` from `permit::evaluate_shell` is not stored, and `supervisor.db.approvals.payload` has `{binding, arguments}`.

**Decision:** extract the pure parser into `crates/ms-shell`, a dependency-free shared crate used by both `ms-agent` and `ms-console`. T205 showed a direct agent dependency would pull model/MCP transports; the shared crate keeps those dependencies out of the console. `ms-shell::parse_shell_command` recomputes structure from the bound `shell.run` arguments, including simple commands, while approval binding remains the stored supervisor authority. The implementation follows Q4’s stated default.

## R-C7 — Ending a session

**Decision:** add supervisor op `end_session {session_id}`. It is implemented in `ms-runtime::lifecycle`; the supervisor's dispatch adds one arm. Steps:
1. Validate that the id is a lead and not closed.
2. Expire that session tree's pending approvals as `denied`, with `resolved_at` and reason `session_end`.
3. Gracefully stop members, then the lead (existing `graceful_stop_session` with `grace_ms`).
4. Set `status='closed', close_reason='session_end'`.
5. Emit `session.closed` with `data.by = "operator"`.

Before adding it, move those helpers **and** the shared idle query loops from `service.rs` into `ms-runtime::lifecycle`. The four helpers alone free only 122 code lines; the full region is 179. T212 must measure the actual before/after budget and reach ≤5,850. See T204 evidence above.

## R-C8 — Cross-channel replies

**Finding:**
- Telegram calls `wait` immediately after a pending response (`crates/ms-channels/src/telegram.rs`), so approving from the console still delivers the follow-up reply to Telegram.
- The webhook adapter only calls `wait` when its own client posts `/approve` or `/deny`, so a console approval of a webhook-origin request leaves the follow-up reply for the console to show, not the webhook client.
- A message sent from the console into a Telegram session gets its reply over RPC to the console. It is not delivered to Telegram.

**Decision:** document both behaviours in the UI (US1 scenario 6) and in `docs/console.md`. Mirroring is out of scope (Q1).

## R-C9 — Usability check for SC-C08

**Decision:** three people unfamiliar with MicroSwarm, given the Sessions page with one session waiting on an approval. Task: "send the lead a message" and "say what the session is waiting for". Pass if all three do both within 10 s. Record the results here with the date.

**T252 native Simplified Chinese review (2026-11-09):** a native Simplified Chinese speaker reviewed the console catalog and reported nine blocking corrections plus six non-blocking suggestions. Applied all fifteen: use 主代理 consistently for lead references, restore the literal `/path/to/skill` example, correct ended-idle and ready status wording, simplify 成员, and apply the requested wording polish. The review sheet records the approved strings and conclusion; no reviewer identity was recorded.

**T252 follow-up (2026-11-14):** the same review process covered the three later chat error strings. The owner relayed native-speaker approval of the final plain-text wording in `docs/reviews/console-zh-native-review.md`; `主目录` remains the established term and inline backticks are omitted because translations render as text nodes. T254 still requires three first-time operators and remains a next-step check.


## Phase 5 verification (2026-11-01)

- **T242 / SC-C02:** `cargo test -p ms-console --test api_status -- --nocapture` reported `status.changed arrived in 298 µs after the supervisor event was written`; the assertion passed under 500 ms. This measures console event receipt and status derivation after the fake supervisor writes the event to the socket. It does not include the supervisor's database commit. The browser fixture below measures commit-to-DOM; the combined CPU fixture includes the 50-session merge, console HTTP server, and one follower without connected browser SSE clients.
- **T242 / SC-C03:** `cargo build --release -p ms-bench --bin console_event_stream_cost`; `/usr/bin/time -f 'CPU user=%U s system=%S s elapsed=%e s maxRSS=%M KB' target/release/console_event_stream_cost 60` reported `elapsed=60.00s; stream=timed out as expected`, `CPU user=1.20 s system=0.74 s elapsed=60.03 s maxRSS=44464 KB`, or **3.2% of one core** for the event merge plus one embedded console HTTP server and follower. This is below 5%, leaving 1.8 percentage points in the fixture. No browser SSE client was connected.
- **T222 decision:** required and completed before the console follower. The isolated measurement improved from 52.1% to 4.7%; the combined fixture measured 3.2%.
- **Runtime fixture check:** `MS_REQUIRE_RUNTIME=1 MS_TEST_RUNTIME=docker cargo test -p ms-supervisor --test us2-quickstart -- --nocapture` passed 5/5 in 1.81 s, and the parallel `us2-review-regressions` target passed 9/9 in 17.11 s after its fixture waits were made deterministic and runtime harnesses serialized. These cover existing fake-model CLI flows; they do not count as completing the console's manual quickstart steps T237, T241, or T255.

## Phase 9 acceptance evidence (2026-11-01)

| Criterion | Evidence | Result |
|---|---|---|
| SC-C01 | Playwright measured 131 ms from starting `microswarm console` through one-time login and first Sessions render on this host; target ≤ 2 s. | Pass locally |
| SC-C02 | 20 samples from a committed supervisor status/event transaction in SQLite to the rendered status banner had p95 26 ms. The fixture used the console's real Unix socket follower, HTTP API, and browser SSE path. The writer was a controlled supervisor fixture rather than a running `microswarmd`. | Pass in integration fixture |
| SC-C03 | The 60 s combined merge + embedded console HTTP server + one follower fixture used 3.2% of one core with 50 sessions / 20,000 events. No browser SSE client was connected. | Pass for measured fixture |
| SC-C04 | `cargo test --workspace` passed the console XSS and no-secrets suites; Playwright confirmed hostile agent text stayed inert in Chromium. | Pass |
| SC-C05 | `scripts/loc-budget.sh` passed: supervisor + shared 5,862/6,000; runtime + shared 4,047/6,000; console 2,134/4,000; web 594/3,500. | Pass |
| SC-C06 | `tests/e2e/console/package.json` contains only Playwright/axe dev dependencies; no runtime npm dependency or frontend build step. `scripts/web-lint.sh` passed; `crates/ms-console/src/assets.rs` embeds assets. | Pass |
| SC-C07 | Playwright + axe passed with zero violations for six page states (new session, selected session, approvals, trace, host, member) at 1440 px and 390 px. Browser interaction checks also verified chat-send locking and the approval code route. | Pass |
| SC-C08 | R-C9 defines the 3-person, 10-second usability protocol. No participant results have been collected. | Pending human study |

T252 native Chinese review and T255 clean-host full console quickstart are also pending; the Docker fake-model CLI quickstart fixture does not substitute for them.
