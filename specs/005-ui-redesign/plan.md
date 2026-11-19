# Implementation Plan: Console UI redesign

**Branch:** `005-ui-redesign` from `main@b7fcb49` · **Spec:** [spec.md](./spec.md)

## Summary

This feature rebuilds the console frontend around the conversation, inline team, pinned approvals and a lane trajectory. It also adds read-only view endpoints in `ms-console` that turn existing events into steps, phases, lanes and reach. The supervisor, agent and event schemas are not touched.

## Technical context

- **Server:** `crates/ms-console` (Rust, 2,258 lines today; outside the line budgets). New modules: `src/views/{mod,agents,steps,sentences,phases,timeline,team,reach,home}.rs`. `read.rs` stays for 002 routes; shared helpers move to `views/mod.rs` as they are needed.
- **Client:** `crates/ms-console/web`, vanilla ES modules, no build step, no new dependency.
  - New: `js/components/{avatar,pill,step,lanes,dock,drawer,tabs}.js`.
  - Pages: `home.js` (replaces overview), `session.js` (replaces sessions), `member.js` (as a drawer), `trajectory.js` (replaces trace), `approvals.js`, `host.js`.
  - `styles.css` is rewritten with the tokens in design/README.
- **Tests:**
  - Rust unit tests per view module, using event fixtures in `crates/ms-console/tests/fixtures/*.jsonl`;
  - Rust route tests in the existing style;
  - Playwright e2e in `tests/e2e/console/`, extended with a populated `demo` scenario from `console_status_fixture`;
  - axe on every page at 1440/768/390/320.
- **Performance target:** SC-U07, with a memo cache per root session (LRU 32).
- **Constraints:** Principle IX; CSP unchanged; `dom.js` is the only DOM writer; i18n for every string; AGENTS.md rules (test first, four CI commands, small commits, stop at checkpoints).

## Constitution check

| Principle | Effect |
|---|---|
| Supervisor is the only authority | No new operations or writes; the views are read-only (FR-U20) |
| Never trust container-writable files | `events.db` is opened read-only, and values are treated as untrusted text and bounded (≤ 120 chars in sentences, ≤ 20 KB reports) |
| IX: operator surface | Loopback, cookie, CSP and DOM rules unchanged; new routes get the same security e2e |
| Line budgets | Unaffected (`ms-console` isn't budgeted; no supervisor/agent/runtime changes) |

## Structure

```
crates/ms-console/src/
  views/mod.rs        Agent assignment, time and seq helpers, memo cache
  views/sentences.rs  event(s) → StepSentence  (contracts/step-sentences.md)
  views/steps.rs      pairing, folding, decisions, filters, paging
  views/phases.rs     phase derivation
  views/timeline.rs   spans, marks, stats
  views/team.rs       TeamGroup/MemberRow, anchors
  views/reach.rs      Reach from scope rows + profile (reuses read::scope data)
  views/home.rs       HomeView
  server.rs           route additions (GET only)
crates/ms-console/web/
  index.html          new shell markup, swarm mark, favicon link
  styles.css          rewritten (tokens + components)
  js/dom.js           + style option, + svg helper (createElementNS)
  js/format.js        time/duration/short id formatting (FR-U16)
  js/components/*.js
  js/pages/{home,session,member,trajectory,approvals,host}.js
  i18n/{en,zh}.json   + step.*, decision.*, why.*, reach.*, phase.*, home.*
  img/mark.svg        favicon/brand
```

## Phasing

Each phase ends green on the four CI commands plus `console-e2e`.

1. **Foundation:** tokens, shell, brand, dom.js, format.js, i18n scaffolding, the demo fixture, and the FR-U16 text scanner (red until the pages land).
2. **Derivation:** view modules and routes, test-first against JSONL fixtures. **Checkpoint A:** stop and report the API outputs for the demo session.
3. **US1 + US2:** session conversation, Team card, dock, Team panel, member drawer. **Checkpoint B:** stop for owner review with screenshots at the four widths.
4. **US4 + US3:** approvals, then trajectory. **Checkpoint C.**
5. **US5 + US6:** home, host, phone, redirects, removing old pages.
6. **Polish:** a11y, zh review, performance, docs. **Checkpoint D:** stop for the human usability runs (SC-U01–U03).

## Risks

- **Pairing gaps:** a missing `tool.result` (crash). The step stays open as "failed" if `agent.exit` follows, else as "working"; a fixture covers it.
- **Approval-to-call matching:** match by session + argument hash + time order; a fixture includes two identical calls.
- **Live update cost:** throttle refetches to 1/s per view and fetch the steps tail with `after_seq`.
- **Visual drift from the boards:** Checkpoint B screenshot review against `design/boards`.
