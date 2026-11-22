# Quickstart: Console UI redesign

## 0. Build and seed a demo home

```bash
cargo build --workspace --bins
export MS_HOME=$(mktemp -d)
# terminal 1 (keep it open; type "emit tester" later to stream a live step)
target/debug/console_status_fixture "$MS_HOME" --scenario demo     # prints READY
# terminal 2
MS_HOME=<same dir> target/debug/microswarm console --listen 127.0.0.1:8790   # prints the login link
```

Open the printed `/login?token=…` link.

## 1. Home (US5)

- The headline reads "1 thing needs you. 2 sessions are running."
- "Needs you" shows the tester's `cargo test -p auth` with Deny and Approve.
- The Running table shows two sessions, each with a one-line "Latest" and no raw event names.

## 2. Session (US1)

Open "Audit the auth module". Check:
- The Team card sits under the lead's first reply, with researcher "Reported" and tester "Waiting for you".
- Expanding the researcher shows sentences ("Read `refresh.rs` and 5 more", "Sent its report to the lead · 2 findings").
- The dock above the composer shows the tester's request. Scroll the conversation: the dock stays put.
- Type a draft and type `emit tester` in terminal 1: the draft, caret and scroll position are unchanged.
- Approve in the dock (on a disposable fixture, the decision RPC is acknowledged; use a real supervisor to verify member continuation): within 2 s the dock clears and the tester row changes.

## 3. Member drawer (US2)

Click the researcher in the Team panel. Check:
- The reach tab says it reads one folder, with no network and no credentials.
- "Asked for, but reduced" shows two rows with reasons in words.
- The derivation explains what the lead requested, holds, and the host allows.
- Close returns focus to the researcher's card.

## 4. Approvals (US4)

Open Approvals. Check the three tiles, the binding note, the task quote and the sticky footer. At 390 px, the decision is full screen with 52 px buttons.

## 5. Trajectory (US3)

Click Trajectory. Check:
- The tiles show "Waiting for you" in seal colour.
- Clicking the striped tester span selects the row and the URL updates.
- The inspector says why it stopped.
- The Problems filter shows the denied `web.fetch`, with "denied by policy" and its rule.

## 6. Phone

At 390 px:
- the session shows the agent strip;
- the Review bar opens the bottom sheet;
- Esc closes it and returns focus.

## 7. Hostile and load

```bash
target/debug/console_status_fixture "$MS_HOME" --scenario hostile
target/debug/console_status_fixture "$MS_HOME" --scenario load
```

- **Hostile:** every view shows the hostile strings as text, and the browser console shows no CSP reports.
- **Load:** the session is interactive in under 1 s, and the trajectory pages after 200 steps.

## 8. Automated

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo build --workspace --bins && cargo test --workspace
cd tests/e2e/console && npm ci && npx playwright test
```

## 9. Checkpoint D evidence

See [screenshots, automated criteria and timings](../../docs/reviews/005-checkpoint-d/README.md) and [language review notes](../../docs/reviews/005-checkpoint-d/language-review.md). T350 remains owner-run; record its three participant results in `docs/reviews/console-human-acceptance.md` under “005”.
