# MicroSwarm release compliance tests

Run this checklist against the exact commit proposed for each version. A version is not signed off until every required automated gate is green, the applicable manual checks pass, and the evidence below is recorded. Do not put API keys, credential values, startup tokens, cookies, or personal names in the record.

## Release record

- Version/tag:
- Commit:
- Date and operator:
- CI run links (CI, runtime, performance, console E2E):
- Manual test host / OS / runtime / browser:
- Model type used (fake or real; never record credentials):
- Exceptions and linked follow-up issues:

## Required automated checks

Run the repository's four local pre-push commands in this order:

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo build --workspace --bins`
- [ ] `cargo test --workspace`

Then verify all release CI gates on this commit:

- [ ] Dependency policy: `scripts/dep-gate.sh` passes for both musl targets.
- [ ] Web security lint: `scripts/web-lint.sh` passes.
- [ ] Line budgets: `scripts/loc-budget.sh` passes for `ms-agent`, `ms-supervisor`, `ms-runtime`, `ms-console`, and console web assets.
- [ ] CI Rust job is green, including `cargo test --workspace --all-targets` and the locked x86_64 musl release build for `ms-agent` and `ms-tools`.
- [ ] Runtime job is green. It builds the base and Python-enabled agent images and runs the workspace tests with `MS_REQUIRE_RUNTIME=1`; no required runtime tests may be skipped.
- [ ] Performance job is green. Review the current and baseline artifacts and confirm the workflow's 20% regression gate passes.
- [ ] Console E2E job is green. It builds the agent image, runs the Docker fake-model quickstart integration with `MS_REQUIRE_RUNTIME=1`, and runs the browser/axe suite at desktop and phone viewports.

CI workflow sources: [ci.yml](../.github/workflows/ci.yml) and [console-e2e.yml](../.github/workflows/console-e2e.yml). The workflow is authoritative if these steps change.

## Required product acceptance

- [ ] Run [MicroSwarm quickstart](../specs/001-microswarm/quickstart.md) sections 0–6 on the supported Docker runtime. Verify agent isolation and approval, team delegation and scope, failure containment, supervisor restart/recovery, adding a tool, and trace schema validation.
- [ ] Run [Console quickstart](../specs/002-console/quickstart.md) sections 0–6 and its adversarial section on a clean host with Docker and a real model. Record each section's result and any drift in the [console human acceptance sheet](reviews/console-human-acceptance.md).
- [ ] Confirm the console's security and operational behavior: loopback-only listener, one-time login token and secure cookie, Host/Origin/console-header checks, credential values absent from responses/logs, agent text rendered inertly, approvals mediated by the supervisor, and no runtime socket access. Automated adversarial tests and browser checks must also be green.
- [ ] For material console UX changes, complete the three-person first-time operator check described under T254 below; record anonymized results in `specs/002-console/research.md`.
- [ ] Confirm all required GitHub checks are green after the release commit is pushed.

## Deferred manual checks

These checks are intentionally still TODO at the owner's request. They remain open in `specs/002-console/tasks.md`; complete and record them before using the 002-console acceptance work as release sign-off. The automated console quickstart integration is useful evidence, but does not replace these manual checks.

- [ ] **T237 — Console chat, Quickstart §1:** Repeat the section with an active Telegram session. Verify Telegram-origin history, a console-sent message, busy-send lock, and the reply. Record results in the [acceptance sheet](reviews/console-human-acceptance.md).
- [ ] **T241 — Console approvals, Quickstart §2:** Use a `code-review-team` tester approval; then resolve another approval from Telegram and verify the console's “Already decided on another channel” notice at 390 px. Record results in the [acceptance sheet](reviews/console-human-acceptance.md).
- [ ] **T254 — Three-person usability:** Give three first-time operators the Sessions page with one pending approval. Without guidance, ask each to send the lead a message and identify what the session is waiting for. Record anonymized P1/P2/P3 times in seconds and whether both tasks were completed, in `specs/002-console/research.md`.
- [ ] **T255 — Clean-host console acceptance:** On a clean host, run Console quickstart §§0–6 and the adversarial section. Record OS, runtime, browser, model type, date, each section's result, and any failures in the [acceptance sheet](reviews/console-human-acceptance.md). A fresh home on an already provisioned host or a fake-model run alone does not satisfy this task.

T237, T241, and T255 overlap with portions of the Console quickstart acceptance above; keep both the task evidence and the release record so the exact feature acceptance remains traceable.
