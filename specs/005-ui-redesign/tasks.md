# Tasks: Console UI redesign (005)

**Input:** [spec.md](./spec.md), [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [design/README.md](./design/README.md)

**Rules:**
- AGENTS.md applies: test first; red tests compile and carry `#[ignore = "red: T0xx — reason"]` (Rust) or `test.fixme` with the same text (Playwright).
- Run the four CI commands before every push, and `console-e2e` must also be green.
- Keep commits small, titled `<task ids>: <what>`.
- **Stop at every checkpoint.**

`[P]` marks a task that can run in parallel with its neighbours. Paths are relative to the repo root; `C` = `crates/ms-console`.

## Phase 0 — Setup

- [x] T301 Create branch `005-ui-redesign` from `main`; add this spec folder; add `specs/005-ui-redesign/` to AGENTS.md "Source of truth" — `AGENTS.md`
- [x] T302 Capture baseline screenshots of the current console at 1440/390 using the demo fixture (after T308), for before/after comparison — `docs/reviews/005-baseline/`

## Phase 1 — Foundation

- [x] T303 [P] Rewrite `styles.css` with the design tokens, agent colours and base components (buttons, pills, cards, labels). Add a contrast test that checks every text/background pair in design/README and every agent avatar pair at ≥ 4.5:1 — `C/web/styles.css`, `tests/e2e/console/contrast.spec.js`
- [x] T304 [P] Add the swarm mark (`img/mark.svg`) and favicon; replace 微 in `index.html` with the new shell markup (nav landmark, main, live region); serve `img/*` as `image/svg+xml` — `C/web/index.html`, `C/web/img/mark.svg`, `C/src/assets.rs`
- [x] T305 [P] `dom.js`: add `style` (CSSOM property assignment only) and `svg()` (createElementNS) helpers; a unit test asserts no `setAttribute('style')`, no `innerHTML`, no `insertAdjacentHTML` anywhere in `web/js` — `C/web/js/dom.js`, `tests/e2e/console/dom.spec.js`
- [x] T306 [P] `format.js`: local `HH:MM`, durations (`0.4 s`, `3 min 40 s`, `2 h 5 min`), relative "11 min left", short id `01ARZ3…FAW`; unit tests — `C/web/js/format.js`, `tests/e2e/console/format.spec.js`
- [x] T307 [P] Add all new i18n keys (step.*, decision.*, why.*, reach.*, phase.*, home.*, nav.*, dock.*) in en and zh, plus a key-parity test (same keys, same `{placeholders}`) — `C/web/i18n/`, `tests/e2e/console/i18n.spec.js`
- [x] T308 Extend `console_status_fixture` with these scenarios:
  - `demo`: lead + researcher + tester; folded reads; a reduced spawn; a pending `shell.run` approval; a denied `web.fetch`; one ended session; one telegram session;
  - `hostile`: every string field is hostile;
  - `load`: 2,000 events, 6 members.

  Notes:
  - The scenarios are selected with `--scenario <name>`. With no flag, the current behaviour stays, so the 002 e2e is unchanged.
  - Scenarios create `supervisor.db` with the real schema (the `ms-store` migrations, not the trimmed tables the fixture uses today), plus each session's `out/events.db`.
  - A stdin line `emit <session_role>` appends one live step and streams it, as the fixture's stdin loop does now.

  Files: `crates/ms-bench/src/bin/console_status_fixture.rs`, `tests/e2e/console/`
- [x] T309 App shell: 240 px nav / 64 px rail / phone top bar (FR-U01); router with the FR-U02 routes and redirects (`#/overview`→`#/home`, `#/trace/<id>`→trajectory, `#/members/<id>`→session drawer); keep the navigation-focus behaviour from `4f789f0` and port its test — `C/web/js/app.js`, `C/web/js/router.js`, `C/web/js/components/nav.js`
- [x] T310 Red e2e text scanner for FR-U16: on every page, outside `details` and `[data-raw]`, fail on event-type identifiers (`/\b[a-z]+\.[a-z_]+\b/` matched against the event type list), ISO timestamps, ULIDs over 8 characters, or JSON brackets — `tests/e2e/console/text-scan.spec.js`

## Phase 2 — Derivation (server, test-first)

- [ ] T311 Add red JSONL event fixtures and unit tests for sentences (one per row in contracts/step-sentences.md, folding, truncation, hostile args) — `C/tests/fixtures/*.jsonl`, `C/src/views/sentences.rs` tests
- [ ] T312 Implement `views/sentences.rs` and `views/mod.rs` (Agent colours/initials, helpers) — `C/src/views/`
- [ ] T313 Red tests for steps:
  - pairing call/decision/result;
  - approval attach (including two identical calls);
  - a missing result followed by an exit;
  - spawn plus `scope.narrowed` → reduced;
  - the three filters;
  - paging with `before_seq` and `after_seq`.

  Files: `C/src/views/steps.rs` tests
- [ ] T314 Implement `views/steps.rs` — `C/src/views/steps.rs`
- [ ] T315 Phases: red tests for the 4 rules and the degraded case, then implement — `C/src/views/phases.rs`
- [ ] T316 Timeline: red tests for spans (gap merge, waiting supersedes working), marks, the tick cap, stats and `since`, then implement — `C/src/views/timeline.rs`
- [ ] T317 Team: red tests for grouping by lead turn, anchors, the state mapping table and `more`, then implement — `C/src/views/team.rs`
- [ ] T318 Reach: red tests for every `NarrowedReason` word, derivation chips (struck/lowered), the lead variant and container words, then implement on top of `read::scope` — `C/src/views/reach.rs`
- [ ] T319 Home: red tests, then implement `HomeView` — `C/src/views/home.rs`
- [ ] T320 Extend the approval detail with `agent`, `sentence`, `brief`, `policy`, `facts`, `runs_in` and `spawn_preview` (scope intersection via `ms_proto`, nothing written) — `C/src/read.rs`, `C/src/views/`
- [ ] T321 Extend messages with `based_on_role` and `team_group_id` — `C/src/read.rs`
- [ ] T322 Add the GET routes in contracts/console-http-additions.md, plus the memo cache (LRU 32, keyed by last seq). Route tests cover shapes, `400` on bad ids, `404` on unknown ids, `421` on a bad Host, `401` without a cookie, and the path-containment check — `C/src/server.rs`, `C/tests/`
- [ ] T323 Performance test on the `load` fixture: the team view in < 150 ms, the first steps page in < 300 ms, and the timeline in < 300 ms, measured warm in release mode on CI (generous thresholds; the test prints the timings) — `C/tests/perf_views.rs`

**Checkpoint A:** stop and report:
- the JSON of `/team`, `/steps?limit=20`, `/timeline` and `/members/<tester>/reach` for the `demo` fixture;
- the perf timings;
- any event the sentence table couldn't describe.

## Phase 3 — US1 Session conversation (P1) 🎯

- [ ] T324 [P] Red e2e for US1 scenarios 1–6, including the dock staying in view at the 4 widths (SC-U05) — `tests/e2e/console/session.spec.js`
- [ ] T325 [P] Components: `avatar.js`, `pill.js` (status and decision pills, always with words), `step.js` (step row with icon, sentence with `<code>` args, duration and decision) — `C/web/js/components/`
- [ ] T326 `session.js`: header (title, helper, status pill, avatar stack, Trajectory, End session); conversation with You bubbles and Lead blocks; sender lines with `based_on_role` — `C/web/js/pages/session.js`
- [ ] T327 Team card at the `team_group_id` anchors: rows, expand with steps (first 20, "Show all N steps"), waiting-row styling, live tail with `after_seq` — `C/web/js/components/teamcard.js`
- [ ] T328 Approval dock: pinned in the layout above the composer, oldest first with "+N more", Approve and Deny through the 002 decide call, "Already decided on another channel", and the "Why it asks" link — `C/web/js/components/dock.js`
- [ ] T329 Composer moved into the dock area; keep 002's draft, caret, focus and scroll preservation and the in-flight lock; the ended-session state (FR edge case) — `C/web/js/pages/session.js`
- [ ] T330 Live updates: SSE events for the open root trigger throttled (1/s per view) refetches of the team, steps tail, approvals and status — `C/web/js/live.js`

## Phase 4 — US2 Team panel and member drawer (P1)

- [ ] T331 [P] Red e2e for US2 scenarios 1–5 (reduction words, derivation, drawer focus return, dismiss prefill, redirect) — `tests/e2e/console/member.spec.js`
- [ ] T332 `lanes.js` (shared with the trajectory): lanes, spans, ticks and marks positioned with CSSOM percentages. Team panel: lane strip (`since` = 15 min) and agent cards with reach lines and the reduced count; hidden below 1100 px, with the agent strip under the title in its place — `C/web/js/components/lanes.js`, `C/web/js/components/teampanel.js`
- [ ] T333 Member drawer: header, task box, tabs (Steps / What it can reach / Report), reach tab per design/README, report as plain text, nav collapse, conversation dimming with row highlight; full screen below 1100 px; Esc and Close return focus — `C/web/js/pages/member.js`, `C/web/js/components/{drawer,tabs}.js`

**Checkpoint B:** stop for owner review. Report:
- screenshots of the session (no drawer), the session with the tester drawer, and the phone session, at 1440/768/390/320, beside the baseline from T302;
- the US1/US2 e2e results.

Wait for confirmation before Phase 5.

## Phase 5 — US4 Approvals (P1), then US3 Trajectory (P2)

- [ ] T334 [P] Red e2e for US4 (tool and spawn variants, the decided-elsewhere message, 390 px full screen with a sticky 52 px footer) — `tests/e2e/console/approvals.spec.js`
- [ ] T335 `approvals.js`: queue tabs and items, decision layout (title, code, command block, three tiles, binding note, brief, Exact call disclosure, sticky footer), spawn variant with the derivation preview — `C/web/js/pages/approvals.js`
- [ ] T336 [P] Red e2e for US3 (selection sync lane ↔ row ↔ URL, filters, phases, paging past 200, export, raw disclosure) — `tests/e2e/console/trajectory.spec.js`
- [ ] T337 `trajectory.js`: header, filter segmented control, tiles, full lanes card with legend, horizontal scroll inside the card below 640 px — `C/web/js/pages/trajectory.js`
- [ ] T338 Steps card: phase headings, rows, filter, paging on scroll, selection in the URL (`/trajectory/<step_id>`) — `C/web/js/pages/trajectory.js`
- [ ] T339 Inspector: explanation box, facts list, actions, "Raw events (N)" `<details>` marked `data-raw`, Export JSON kept from 002 — `C/web/js/components/inspector.js`

**Checkpoint C:** stop and report screenshots of the approvals (tool and spawn) and trajectory (selected waiting step; Problems filter) at 1440 and 390, plus the e2e results.

## Phase 6 — US5 Home, US6 Host, phone and cleanup

- [ ] T340 [P] Red e2e for US5 (summary line, Needs-you inline decide, empty state, Running/Ended tables, start a session) — `tests/e2e/console/home.spec.js`
- [ ] T341 `home.js` per design/README; make it the default route — `C/web/js/pages/home.js`
- [ ] T342 `host.js`: settings layout with a section nav and policies in words; keep the 002 Set value dialog and its tests — `C/web/js/pages/host.js`
- [ ] T343 Phone layouts: agent status strip, Review bar, approval bottom sheet (focus trap, Esc, 52 px buttons) — `C/web/js/components/{sheet,agentstrip}.js`, `C/web/styles.css`
- [ ] T344 Remove `overview.js`, `sessions.js`, `trace.js` and their CSS once their e2e cases are ported; keep redirects; remove the double-border "wall" styles — `C/web/`

## Phase 7 — Polish

- [ ] T345 Accessibility:
  - keyboard paths for every page (Tab order, Enter/Space on rows, arrow keys in tabs and segmented controls);
  - `aria-live` for the dock and session status;
  - focus return from the drawer, sheet and dialog;
  - `prefers-reduced-motion`;
  - axe on every page at the 4 widths, with no horizontal page scroll (SC-U06).

  Files: `C/web/`, `tests/e2e/console/`
- [ ] T346 [P] zh: translate the new keys, check layouts with long zh and long en strings, and record native review notes — `C/web/i18n/zh.json`, `docs/reviews/`
- [ ] T347 Make T310's text scanner green on every page in both locales, using the `demo` and `hostile` fixtures (SC-U04) — `tests/e2e/console/text-scan.spec.js`
- [ ] T348 Security e2e: every new route under the 002 checks (Host, cookie, bad ids); the `hostile` fixture rendered as text on every new view; CSP violation listener with zero reports (SC-U08) — `tests/e2e/console/security.spec.js`
- [ ] T349 Docs:
  - design/README replaces `specs/002-console/design/README.md`, which then points here;
  - update the 002 quickstart screenshots/wording;
  - update the AGENTS.md checkpoint note for 005;
  - add the e2e timings for SC-U07.

  Files: `specs/`, `AGENTS.md`, `docs/`
- [ ] T350 Manual usability (owner-run): SC-U01–U03 with 3 first-time participants on the `demo` fixture; record anonymous results in `docs/reviews/console-human-acceptance.md` under "005"

**Checkpoint D:** stop and report: all automated SCs, CI and `console-e2e` green, and before/after screenshots. T350 stays open until the owner records results.

## Dependencies

```text
Phase 0 → Phase 1 (T303–T307 parallel; T308 before T302 and all e2e; T309 after T304/T305)
Phase 1 → Phase 2 (T311→T312, T313→T314, then T315–T321 in any order, T322 after all views, T323 last)
Checkpoint A → Phase 3 (T325 before T326–T328; T332 before T333) → Checkpoint B (owner OK)
→ Phase 5 (US4 before US3; T332's lanes.js reused in T337) → Checkpoint C
→ Phase 6 (T344 after T341–T343) → Phase 7 → Checkpoint D
```
