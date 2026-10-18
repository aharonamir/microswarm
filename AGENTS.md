# MicroSwarm project rules

- **Source of truth:** `.specify/memory/constitution.md` and `specs/001-microswarm/` (spec, plan, research, data model, contracts, tasks). `tasks.md` is the work order; `research.md` records decisions with evidence.
- **Test first:** red tests must compile and use `#[ignore = "red: T0xx — reason"]`; remove the ignore in the commit that makes the test pass. Runtime integration tests skip with a printed reason when no container runtime is reachable; `MS_REQUIRE_RUNTIME=1` makes a skip fail.
- **Before every push, run in this order:** `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace --bins`; `cargo test --workspace`. After pushing, confirm CI is green.
- **Security invariants:** the supervisor never trusts files writable by a container; authoritative scope lives in `supervisor.db`; `control/` is read-only to the agent; scope intersection never widens; shell classification defaults to ask and unparseable commands are denied; unknown exit reasons map to safety with no retry.
- **Line budget:** `ms-agent` and `ms-supervisor` are each limited to 6,000 lines; check with `scripts/loc-budget.sh`.
- **Changes:** keep commits small, titled `<task ids>: <what>`. Add no dependency without a one-line justification in the commit message.
- **Checkpoints:** stop and report at every checkpoint in `tasks.md`. Do not start US2 until the owner confirms the US1 checkpoint.
