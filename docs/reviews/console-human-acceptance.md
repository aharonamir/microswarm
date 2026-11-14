# Console human acceptance run sheet

Use this sheet to complete T237, T241, T252, T254, and T255. Record only anonymized participant labels and environment details. Never include credential values, tokens, or personal names.

## Next checks

- [ ] T237: repeat §1 with an active Telegram session and verify cross-channel history, busy-send lock, and the reply.
- [ ] T241: repeat §2 with a `code-review-team` tester approval, then resolve another approval from Telegram and verify the console notice at 390 px.
- [x] T252: native Simplified Chinese follow-up for all three later chat-error strings; approved 2026-11-14 in the review sheet.
- [ ] T254: recruit three first-time operators, time both tasks for each, and record anonymized P1/P2/P3 results in R-C9.
- [ ] T255: run §§0–6 and Adversarial on a clean host with Docker and a real model, recording every section below.

## T252 — native Simplified Chinese

Review every row in [console-zh-native-review.md](./console-zh-native-review.md) as a native Simplified Mandarin speaker. Record corrections beside the affected key. Mark the reviewer conclusion and add the review date there.

## T254 — first-time operator usability

Recruit three people who have not used MicroSwarm. Open a session waiting for an approval at the Sessions page. Give each person these two tasks without guidance:

1. Send the lead a message.
2. Say what the session is waiting for.

Pass each participant only if both tasks are completed within 10 seconds. Use anonymous labels and record results in `specs/002-console/research.md`.

| Participant | New to MicroSwarm? | Seconds to both tasks | Completed both? | Notes |
|---|---|---:|---|---|
| P1 |  |  |  |  |
| P2 |  |  |  |  |
| P3 |  |  |  |  |

## T237 and T241 — console quickstart sections 0–2

Run [quickstart.md](../../specs/002-console/quickstart.md) §§0–2 against Docker and the fake model. For §1, use an active Telegram session if one is available; do not substitute a mocked API/browser fixture for the channel behavior. Record failed steps and fixes in the quickstart or task notes.

### Automated integration evidence (not a manual quickstart pass)

On 2026-11-01, `MS_REQUIRE_RUNTIME=1 MS_TEST_RUNTIME=docker MS_TEST_AGENT_IMAGE=microswarm-agent:dev cargo test -p ms-supervisor --test console-quickstart -- --nocapture` passed. The integration test starts the real `microswarmd` and fake-model agent, launches `microswarm console`, authenticates with the one-time startup token, confirms the actual session appears in `/api/v1/sessions`, approves its pending tool request through the authenticated console endpoint, then sends a follow-up through the console chat endpoint and confirms it appears in session history after reaching the fake model. It verifies live supervisor socket paths for session reads, chat, and approval decisions. It does not run the quickstart UI with an active Telegram session or replace the manual §0–§2 run below.

| Section | Passed? | Environment/date | Drift or fix |
|---|---|---|---|
| 0 — Start | Pass | Ubuntu 26.04.1, Docker 29.1.3, Chromium, fresh temporary home, fake model; 2026-11-14 | One-time login reached Sessions at `/`, reused link returned 401, and non-loopback refusal included SSH/TLS guidance. Corrected the quickstart's stale URL and response wording. |
| 1 — Chat with a lead | Partial | Same environment, 2026-11-14 | A new web session received its fake-model reply and End session moved it under Ended today with reason and trace link. Fixed the login-root selection and overlapping render bugs. Telegram-origin history and cross-channel send remain to check. |
| 2 — Approvals | Partial | Same environment, 2026-11-14 | A lead shell approval appeared in the badge, banner, queue, and detail; approving through the console moved it to Decided and the lead replied. Fixed the false “another channel” notice for locally decided approvals. The tester-member and Telegram decision paths remain to check. |

## T255 — clean-host quickstart and adversarial section

On a clean host, run [quickstart.md](../../specs/002-console/quickstart.md) §§0–6 and the adversarial section. Record the OS, runtime, browser, model type (fake or real), date, and each failed step. Do not include credentials, startup tokens, or cookie values.

The 2026-11-14 run above used a fresh temporary home on an already provisioned host. It is partial evidence and does not satisfy the clean-host or real-model requirement here.

| Section | Passed? | Drift or fix |
|---|---|---|
| 0 — Start |  |  |
| 1 — Chat with a lead |  |  |
| 2 — Approvals |  |  |
| 3 — Live team |  |  |
| 4 — Member scope |  |  |
| 5 — Trace |  |  |
| 6 — Host |  |  |
| Adversarial |  |  |
