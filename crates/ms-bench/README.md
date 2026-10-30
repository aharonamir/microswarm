# Performance benchmark

The `performance` integration target measures the four Phase 9 success criteria
against a Docker runtime and a deterministic local model endpoint. It is behind
the `measure` feature so normal workspace tests do not create containers.

```sh
cargo build --locked --release --workspace --bins
cargo build --locked --release --target x86_64-unknown-linux-musl -p ms-agent -p ms-tools
docker build --tag microswarm-agent:dev --file images/agent.Containerfile .
MS_REQUIRE_RUNTIME=1 \
MS_TEST_RUNTIME=docker \
MS_TEST_AGENT_IMAGE=microswarm-agent:dev \
MS_TEST_SUPERVISORD="$PWD/target/release/microswarmd" \
MS_BENCH_RUNNER=local \
MS_BENCH_AGENT_IMAGE=microswarm-agent:dev \
MS_BENCH_AGENT_BINARY="$PWD/target/x86_64-unknown-linux-musl/release/microswarm-agent" \
MS_BENCH_OUTPUT="$PWD/benchmark-report.json" \
cargo test --locked --release -p ms-bench --features measure --test performance \
  collect_phase9_performance_report -- --exact --nocapture
target/release/ms-bench validate benchmark-report.json
```

`MS_BENCH_SAMPLES` controls fresh-session startup and RSS samples (default 10,
minimum 3). `MS_BENCH_SWARM_SAMPLES` controls warm single-agent and warm
lead-plus-four-member turn comparisons (default 3, minimum 1).

SC-001 starts its monotonic clock immediately before the host chat RPC and stops
when the session's `agent.ready` event is persisted. SC-002 reads the static
musl agent file and Docker's image size. SC-003 waits for the initial fake-model
reply, then reads `VmRSS` for the container's host PID from
`/proc/<pid>/status` while the agent is idle. SC-004 warms both setups
before timing a turn, then subtracts the fake model server's response-generation
time; the four-member case waits for all four `send_to_member` results.

The report includes runner, revision, sample-derived values, and all four
metrics. `validate` prints each metric against the specification target. CI
builds the base revision and the change on the same `ubuntu-latest` host and
fails if any metric grows by more than 20% relative to that base revision.
