# Research: Deferred Manual Acceptance Checks

This package consolidates existing acceptance work; it makes no new product or architecture decisions. The original task IDs and criteria remain authoritative.

## Evidence reviewed

- [`specs/002-console/tasks.md`](../002-console/tasks.md): T237, T241, T254, and T255 are unchecked and explicitly deferred.
- [`specs/002-console/quickstart.md`](../002-console/quickstart.md) §§0–6 and Adversarial: procedures for the console start, chat, approval, team, scope, trace, host, and adversarial checks.
- [`specs/002-console/research.md`](../002-console/research.md) R-C9 and SC-C08: the usability check is pending; the criterion is that each first-time participant complete both tasks within 10 seconds.
- [`docs/reviews/console-human-acceptance.md`](../../docs/reviews/console-human-acceptance.md): records §0 as passed on 2026-11-14; §1 and §2 as partial. Telegram-origin history/cross-channel chat and tester-member/Telegram-decided approval paths remain unchecked. Its temporary-home run on an already provisioned host does not satisfy T255's clean-host and real-model requirements.
- [`docs/release-compliance-tests.md`](../../docs/release-compliance-tests.md): retains all four checks as required manual acceptance before 002-console acceptance can be used for release sign-off.

## Decisions

| Item | Decision | Evidence |
|---|---|---|
| Task identity | Retain T237, T241, T254, and T255 for traceability; carry their procedures in this work order. | The 002-console task list and release checklist already use these identifiers. |
| T237 channel condition | Use an active Telegram session; browser fixtures alone cannot establish cross-channel behavior. | Quickstart §1 and acceptance sheet note that Telegram-origin history and cross-channel send remain unchecked. |
| T241 channel condition | Resolve a second approval in Telegram, then inspect the console at 390 px. | Quickstart §2 and acceptance sheet identify this path as pending. |
| T254 pass criterion | Preserve SC-C08: each participant completes both tasks within 10 seconds. | `specs/002-console/research.md`, R-C9 and SC-C08. |
| T255 environment | Require a clean host, Docker, and a real model; the earlier fresh-home/fake-model evidence remains partial. | Acceptance sheet's T255 note and the release checklist's deferred task text. |
| Evidence location | Use the existing acceptance sheet; place T254 participant results in the original R-C9 section as well. | Existing 002-console task and release checklist references. |

No new measurements were performed while preparing this work order. No dependencies or implementation changes are proposed.
