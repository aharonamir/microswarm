# Tasks: Deferred Manual Acceptance Checks

**Input:** [spec.md](./spec.md) · [plan.md](./plan.md) · [research.md](./research.md)

**Evidence:** [Console human acceptance sheet](../../docs/reviews/console-human-acceptance.md) · [release compliance checklist](../../docs/release-compliance-tests.md)

**Status:** All four tasks are open. Their original IDs are retained from `specs/002-console/tasks.md`.

## Phase 0 — Prepare

- [ ] Confirm the operator has read the console quickstart and this work order; review the partial evidence and identify any available Telegram-connected test session — `specs/002-console/quickstart.md`, `docs/reviews/console-human-acceptance.md`
- [ ] Confirm T254 participant scheduling and T255 clean-host/Docker/real-model prerequisites; do not mark tasks complete if prerequisites are missing — `docs/reviews/console-human-acceptance.md`

## Phase 1 — Complete the deferred checks

- [ ] **T237 — Console chat across channels:** Verify the active Telegram session's history in the console; send a console message; verify the busy-send lock and resulting reply. Recheck §0 only as needed to establish the environment. Record date, environment, result, and drift — [quickstart §0–§1](../002-console/quickstart.md), [acceptance sheet](../../docs/reviews/console-human-acceptance.md)
- [ ] **T241 — Approvals across channels:** Use a `code-review-team` tester approval; resolve one request in the console and a separate request in Telegram; verify the cross-channel notice and usability at 390 px. Record date, environment, result, and drift — [quickstart §2](../002-console/quickstart.md), [acceptance sheet](../../docs/reviews/console-human-acceptance.md)
- [ ] **T254 — Three-person usability:** Give three first-time operators the Sessions page with a session waiting for one approval. Without guidance, ask each to send the lead a message and say what the session is waiting for. Record anonymous P1/P2/P3 time-to-both-tasks and completion results in R-C9 and the acceptance sheet; assess against the existing 10-second criterion — [SC-C08/R-C9](../002-console/research.md), [acceptance sheet](../../docs/reviews/console-human-acceptance.md)
- [ ] **T255 — Clean-host console acceptance:** On a clean host with Docker and a real model, run quickstart §§0–6 and Adversarial. Record OS, runtime, browser, model type, date, every section's result, and failures/fixes. Do not include credentials, startup tokens, cookies, or personal names — [quickstart](../002-console/quickstart.md), [acceptance sheet](../../docs/reviews/console-human-acceptance.md)

## Checkpoint A — Manual acceptance complete

Stop and report when T237, T241, T254, and T255 have complete evidence. If a check fails or cannot run, report the observed evidence and leave that task open. Reconcile the release checklist only for tasks whose acceptance criteria are met.
