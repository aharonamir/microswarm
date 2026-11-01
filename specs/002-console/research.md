# Research: MicroSwarm Console

Phase 0 output. Items marked **⚠ verify** carry a decision taken from a source read on 2026-11-01 (repo `main` at `6108308`). Confirm each against source before the task that depends on it, and record the evidence here.

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
| inbound | `member_reply` | "Lead · from <role>" only if the lead's next `reply` doesn't already restate it; else a Team marker |
| inbound | `member_failed` | Team marker "<role> ended · <reason in words>" |
| inbound | `delegation_result` | Team marker "<role> joined" / "couldn't start · <reason>" |
| inbound | `approval_result` | Team marker "You approved/denied <code>" |
| outbound | `reply` | "Lead" |
| supervisor.db | `approvals` where `root_session_id` = this lead | inline approval card at its `created_at`. This covers member requests, whose `approval_request` sits in the member's own outbound. The approval row, never the agent's outbound body, is what's displayed |
| outbound | `delegation_request`, `send_to_member`, `dismiss` | not shown (visible in the trace) |

**⚠ verify:** the `member_reply` de-duplication rule against real transcripts. If it is unreliable, always show member replies as their own "Lead · from <role>" item.

## R-C2 — Testing the frontend without a build toolchain

**Decision:**
- **Core CI** (the four commands): Rust tests only. Endpoint contracts, headers, auth, SSE, XSS (the API returns agent HTML as inert JSON strings), and no-secrets checks. `scripts/web-lint.sh` greps `web/` for forbidden sinks (`innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `eval(`, `new Function`, `setTimeout("`, `setInterval("`) and for inline `<script>` or `style=` in `index.html`.
- **A separate `console-e2e` CI job:** Playwright (Chromium) plus `@axe-core/playwright`, driving the console against the fake model and Docker. It is not part of the four required commands, mirroring the runtime job's opt-in pattern.

**Rationale:** keeps SC-C06 (no npm runtime deps, no build step) while still testing behaviour and accessibility in a real browser.

## R-C3 — Event fan-out cost

**Finding:** `ms-runtime::events::stream` in follow mode rebuilds the merged ledger each 100 ms by reading every `supervisor_events` row and every session's `out/events.db`, then reconciling `event-stream.db`. Cost grows with total history.

**Decision:** the console holds exactly one follower (FR-C40). Measure SC-C03 with 50 sessions and 20,000 events (T221). If it fails, make the merge incremental in `ms-runtime` (per-source high-water marks for each session's `events.db` and for `supervisor_events.seq`) as T222. **⚠ verify** by measurement.

## R-C4 — Fonts under a strict CSP

**Decision:** vendor IBM Plex Sans (400/500/600) and IBM Plex Mono (400/500) as woff2 Latin subsets plus `OFL.txt` under `web/fonts/`, served from `'self'`. Chinese text uses the system CJK font (`"PingFang SC","Noto Sans CJK SC","Microsoft YaHei",sans-serif`); no CJK webfont.

**Fallback if the owner declines vendoring:** `system-ui` / `ui-monospace` stacks only. The layout must not depend on Plex metrics.

## R-C5 — One message in flight per session

**Finding:** `ms_runtime::claim_session_busy` updates `status='busy'` when the status is `'ready'` **or `'busy'`**. A second message while busy is therefore accepted by the supervisor, not rejected.

**Decision:** the console enforces one in-flight chat per session with its own in-memory lock (FR-C23) and returns 409 `session_busy`. Changing the supervisor's claim semantics is out of scope, because it would also affect CLI and Telegram behaviour. Note this as an open question for the owner (Q2 in tasks.md).

## R-C6 — Showing the parsed shell structure

**Finding:** the agent's `approval_request` body is `{approval_local_id, server, tool, args, binding}` (`crates/ms-agent/src/model.rs`). The `ParsedCommand` from `permit::evaluate_shell` is not stored, and `supervisor.db.approvals.payload` has `{binding, arguments}`.

**Decision:** for `tool == "shell.run"`, the console computes the display structure by calling `ms_agent::permit::evaluate_shell(args.command)`, a pure function. `ms-console` depends on the `ms-agent` library crate for that alone. The console never trusts a structure supplied by the agent. If the dependency on `ms-agent` is unwanted, the fallback is to omit the parsed view and show only the bound arguments.

## R-C7 — Ending a session

**Decision:** add supervisor op `end_session {session_id}`. It is implemented in `ms-runtime::lifecycle`; the supervisor's dispatch adds one arm. Steps:
1. Validate that the id is a lead and not closed.
2. Expire that session tree's pending approvals as `denied`, with `resolved_at` and reason `session_end`.
3. Gracefully stop members, then the lead (existing `graceful_stop_session` with `grace_ms`).
4. Set `status='closed', close_reason='session_end'`.
5. Emit `session.closed` with `data.by = "operator"`.

Before adding it, move `stop_members_for_lead`, `stop_idle_lead`, `stop_member_idle` and `graceful_stop_session` from `service.rs` into `ms-runtime::lifecycle` to free supervisor budget. **⚠ verify** that the exact move frees at least 150 lines, and record the before/after `loc-budget.sh` output.

## R-C8 — Cross-channel replies

**Finding:**
- Telegram calls `wait` immediately after a pending response (`crates/ms-channels/src/telegram.rs`), so approving from the console still delivers the follow-up reply to Telegram.
- The webhook adapter only calls `wait` when its own client posts `/approve` or `/deny`, so a console approval of a webhook-origin request leaves the follow-up reply for the console to show, not the webhook client.
- A message sent from the console into a Telegram session gets its reply over RPC to the console. It is not delivered to Telegram.

**Decision:** document both behaviours in the UI (US1 scenario 6) and in `docs/console.md`. Mirroring is out of scope (Q1).

## R-C9 — Usability check for SC-C08

**Decision:** three people unfamiliar with MicroSwarm, given the Sessions page with one session waiting on an approval. Task: "send the lead a message" and "say what the session is waiting for". Pass if all three do both within 10 s. Record the results here with the date.
