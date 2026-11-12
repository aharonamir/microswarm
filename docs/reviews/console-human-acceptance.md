# Console human acceptance run sheet

Use this sheet to complete T237, T241, T252, T254, and T255. Record only anonymized participant labels and environment details. Never include credential values, tokens, or personal names.

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
| 0 — Start | Partial | Operator report, 2026-11-12; environment not provided | Startup worked. Non-loopback refusal occurred but omitted SSH/TLS guidance; fixed in code, needs recheck. |
| 1 — Chat with a lead | No | Operator report, 2026-11-12; environment not provided | New web session was created and the composer cleared, but no reply appeared. No model was configured. The console now reports the setup step; repeat §1 after configuring a model. |
| 2 — Approvals |  |  |  |

## T255 — clean-host quickstart and adversarial section

On a clean host, run [quickstart.md](../../specs/002-console/quickstart.md) §§0–6 and the adversarial section. Record the OS, runtime, browser, model type (fake or real), date, and each failed step. Do not include credentials, startup tokens, or cookie values.

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
