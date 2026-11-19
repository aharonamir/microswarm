# Execution Plan: Deferred Manual Acceptance Checks

**Spec:** [spec.md](./spec.md) · **Work order:** [tasks.md](./tasks.md) · **Source:** [`002-console`](../002-console/)

## Summary

This work order completes four manual acceptance checks deferred from 002-console. It requires operator interaction with the running console, Telegram for cross-channel checks, three first-time participants for T254, and a clean host with Docker and a real model for T255. Results are recorded in the existing acceptance and research documents.

No product code, database schema, API contract, or dependency changes are planned. If a check reveals product or documentation drift, capture the evidence and fix that drift before marking its task complete, following repository rules.

## Prerequisites

- Read [Console quickstart](../002-console/quickstart.md), this spec, and the task procedures before starting.
- Have the currently supported console build and a working Docker runtime available.
- For T237/T241, have an active Telegram-connected session and access to both the console and Telegram.
- For T254, schedule three first-time MicroSwarm operators and prepare a session with one pending approval.
- For T255, use a clean host, Docker, a real model, and a browser; collect environment details without secrets.

## Execution order

1. Review existing partial evidence and environment notes in the acceptance sheet.
2. Complete T237 and T241 while their Telegram-connected setup is available.
3. Run T254 with three participants and record anonymized results in R-C9.
4. Run T255 on a clean host and record all quickstart sections, adversarial result, and any fixes.
5. Reconcile the release checklist and report any remaining failures or drift.

## Evidence and safety

- Record exact outcomes, dates, environment, and failures in the designated evidence tables.
- Never record model/API credential values, startup tokens, cookies, or participant names.
- Do not substitute automated integration coverage for the required manual channel and clean-host checks.
- Keep a task unchecked if a prerequisite is unavailable or evidence is incomplete; note the blocker and what is still needed.

## Constitution check

This is a manual acceptance plan. It adds no executable surface, authority, runtime dependency, container behavior, or data store. Principle IX remains applicable to all console interactions and recorded evidence.

| Gate | Assessment |
|---|---|
| I–VIII | No implementation changes are planned; existing 002-console constraints remain in force. |
| IX | Manual operation verifies the console as an authenticated supervisor client; evidence must omit credentials and tokens. |

## Complexity tracking

| Deviation | Why | Simpler alternative rejected because |
|---|---|---|
| None | The plan uses existing quickstart and evidence documents. | Not applicable. |
