# Quickstart: MicroSwarm

Validates the feature end to end. Each section maps to a user story and doubles as the manual acceptance script.

## Prerequisites

- Linux host, x86_64 or aarch64
- Rootless Podman ≥ 4 or Docker ≥ 24, socket reachable by your user
- A model endpoint (OpenAI-compatible or Anthropic-compatible) and its API key

## 0 · Install

```console
$ cargo install --path crates/ms-cli --path crates/ms-supervisor
$ podman pull ghcr.io/microswarm/agent:0.1
$ microswarm init --lang en
$ microswarm creds set model-key        # paste key, Ctrl-D
$ microswarm doctor
✓ runtime socket   podman 5.2 rootless
✓ agent image      ghcr.io/microswarm/agent:0.1  (27.4 MB)
✓ model            example-model via proxy  (412 ms)
✓ disk             $MS_HOME writable, 41 GB free
$ microswarmd &
```

## 1 · Single agent (US1)

```console
$ microswarm chat
you › list the files in my work folder
agent › README.md, src/, Cargo.toml
```

Verify:

```console
$ microswarm sessions
01JB… lead  ready  cli  containers: 1
$ ls ~/.microswarm/sessions/01JB…/
events.db  inbound.db  outbound.db  scope.json
$ podman ps --filter label=ms.session=01JB… --format '{{.Names}}'
ms-01jb…
```

Isolation check — the agent cannot reach outside its mounts:

```console
you › read ~/.ssh/id_ed25519
agent › I can't access that path.
$ microswarm trace 01JB… --events tool.result | grep -c '"ok":false'
1
```

## 2 · Swarm (US2)

```console
$ microswarm skills install examples/skills/code-review-team
$ microswarm chat
you › review the auth module and run its tests
⚠ [lead] wants to start a member 'tester'
  task:   run cargo test for src/auth
  mounts: /work ro · /work/target rw
  tools:  shell.run, fs.read
  credentials: none
  /approve K7QF   /deny K7QF
you › /approve K7QF
agent › Researcher found two issues; tests pass (14/14). Details: …
```

Verify scope narrowing and teardown:

```console
$ microswarm trace 01JB…
01JB… lead  ready
├─ 01JC… researcher  closed(dismissed)  scope: 1 mount(ro) · 3 tools · 1 cred
└─ 01JD… tester      closed(dismissed)  scope: 2 mounts · 2 tools · 0 creds
$ jq '.mounts, .narrowed' ~/.microswarm/sessions/01JD…/scope.json
$ podman ps --filter label=ms.root=01JB… --format '{{.Labels}}' | wc -l
1          # only the lead remains
```

## 3 · Failure containment (US3)

```console
$ microswarm chat
you › ask the researcher to summarise https://example.com/large
$ podman kill --signal KILL $(podman ps -q --filter label=ms.role=researcher)
agent › The researcher stopped unexpectedly; I'll retry it once…
$ microswarm trace 01JB… --events container.exited,container.restart,delegation.failed
```

Safety exit is never retried: run `tests/fixtures/safety-member` and confirm `container.restart` count is `0` and the lead received `member_failed{reason: safety}`.

## 4 · Supervisor restart (US4)

```console
# with a lead, one member running, and an approval pending:
$ pkill -9 microswarmd && microswarmd &
$ microswarm trace 01JB… --events container.adopted,approval.expired,approval.reissued
```

Expect: every running container adopted, zero `container.orphan_stopped` for matched sessions, the old approval `expired`, a new code sent to the channel.

## 5 · Add a tool without rebuilding (US5)

```console
$ microswarm tools add weather --stdio "python3 /opt/tools/weather_mcp.py"
$ # grant it in [lead.scope].tools, then:
$ microswarm chat
you › what's the weather in Herzliya?
agent › 27°C, clear.
```

No `cargo` command was run.

## 6 · Trace (US6)

```console
$ microswarm trace 01JB… --json | npx ajv validate -s specs/001-microswarm/contracts/events.schema.json -d /dev/stdin
```

## Performance smoke (SC-001…SC-003)

```console
$ cargo run -p ms-bench -- spawn --n 100
spawn→agent.ready  p50 118 ms  p95 302 ms
image 27.4 MB  binary 11.8 MB  idle RSS 14 MB
```

Figures above are illustrative targets, not measurements.
