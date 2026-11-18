# Feature Specification: Deferred Manual Acceptance Checks

**Feature branch:** `004-manual-checks`  
**Status:** Ready to execute; all four checks remain open  
**Source feature:** [`002-console`](../002-console/spec.md)  
**Work order:** [tasks.md](./tasks.md)

## Context

The 002-console implementation and automated checks are complete, but four owner-deferred manual acceptance tasks remain. This spec carries their execution into a small, resumable work order without changing console behavior or treating automated browser/runtime coverage as a substitute for operator testing.

The original task IDs (T237, T241, T254, T255) are retained so existing acceptance evidence and release records remain traceable. Partial observations are in the [console human acceptance sheet](../../docs/reviews/console-human-acceptance.md). Release sign-off remains tracked by the [release compliance checklist](../../docs/release-compliance-tests.md).

## Goals

- Complete the remaining manual chat and approval checks with real channel interactions.
- Measure first-time operator success against SC-C08's existing 10-second usability criterion.
- Run the full console quickstart and adversarial section on a clean host with Docker and a real model.
- Record enough environment and outcome detail to resume or audit the checks without recording secrets or personal names.

## Out of scope

- New product behavior, code changes unrelated to observed acceptance drift, or automated test work.
- Repeating already complete T252, the native Simplified Chinese review.
- Marking a check complete based only on an automated fixture, browser test, or previously partial run.

## Acceptance scenarios

### T237 — Console chat across channels

Given an active Telegram-origin session and the console quickstart environment, the operator verifies that Telegram messages and lead replies appear in the console, sends a message from the console, observes the in-flight lock when attempting a second send, and sees the lead's reply. Record the outcome and any quickstart drift in the acceptance sheet.

### T241 — Approvals across channels

Given a `code-review-team` session with a tester approval, the operator verifies the request and resolves one approval in the console. The operator then creates another approval, resolves it from Telegram, and confirms the console reports “Already decided on another channel.” At 390 px, the approval view remains usable without horizontal scrolling. Record the outcome and any drift in the acceptance sheet.

### T254 — First-time operator usability

Given three people who have not used MicroSwarm, each is shown the Sessions page with one session waiting for an approval. Without guidance, each must send the lead a message and identify what the session is waiting for. Record anonymous participant labels, time to complete both tasks, and whether both were completed. The existing SC-C08 pass criterion is both tasks completed within 10 seconds for each participant; report failures without altering the criterion.

### T255 — Clean-host acceptance

Given a clean host with Docker and a real model configured, the operator runs Console quickstart sections 0–6 and its Adversarial section. Record host OS, runtime, browser, model type, date, each section's result, and failures or fixes. A fresh MicroSwarm home on an already provisioned host or a fake-model-only run is partial evidence and does not complete this task.

## Constraints

- Follow the console's operator-surface security rules in Principle IX. Do not expose credential values, startup tokens, cookies, or personal names in notes or captured output.
- Keep partial and failed outcomes visible. Mark a task complete only after its stated scenario and evidence are complete.
- Record T237, T241, and T255 in `docs/reviews/console-human-acceptance.md`; record T254 participant results in `specs/002-console/research.md` under R-C9 and in the acceptance sheet.
