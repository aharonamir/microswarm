# MicroSwarm project rules

- **Source of truth:** `.specify/memory/constitution.md`, `specs/001-microswarm/`, `specs/002-console/`, `specs/004-manual-checks/`, and `specs/005-ui-redesign/`. Each spec's `tasks.md` is its work order; `research.md` records decisions with evidence.
- **Test first:** red tests must compile and use `#[ignore = "red: T0xx — reason"]`; remove the ignore in the commit that makes the test pass. Runtime integration tests skip with a printed reason when no container runtime is reachable or the required agent image is missing; the runtime CI job must build the image, and `MS_REQUIRE_RUNTIME=1` makes a skip fail.
- **Before every push, run in this order:** `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace --bins`; `cargo test --workspace`. After pushing, confirm CI is green.
- **Security invariants:** the supervisor never trusts files writable by a container; authoritative scope lives in `supervisor.db`; `control/` is read-only to the agent; scope intersection never widens; shell classification defaults to ask and unparseable commands are denied; unknown exit reasons map to safety with no retry.
- **Line budget:** `ms-agent` and `ms-supervisor` are each limited to 6,000 lines; the extracted `ms-runtime` adapter has its own 6,000-line cap. Check all three with `scripts/loc-budget.sh`.
- **Changes:** keep commits small, titled `<task ids>: <what>`. Add no dependency without a one-line justification in the commit message.
- **Checkpoints:** stop and report at every checkpoint in `tasks.md`. For 002-console, stop at every checkpoint in `specs/002-console/tasks.md`; after Checkpoint D, wait for owner confirmation before US3. For 005-ui-redesign, stop at A (T323), B (T333), C (T339), and D (T349); T350 usability is owner-run and stays open until results are recorded.

## Skill Usage Priority
- **Visual and design quality takes precedence**: When Impeccable's design rules conflict with modern-web-guidance's technical recommendations, defer to Impeccable's visual intent.
- **modern-web-guidance operates at the implementation layer**: Adopt its native API and best-practice suggestions only when they do **not compromise the visual style, motion, or layout intent already established by Impeccable**.
