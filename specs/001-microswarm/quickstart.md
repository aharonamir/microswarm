# Quickstart: MicroSwarm

Validates the feature end to end. Each section maps to a user story and doubles as the manual acceptance script.

## Prerequisites

- Linux host, x86_64 or aarch64
- Docker Engine ≥ 24, socket reachable by your user (v0.1 runtime path validated with Docker 29.1.3)
- A model endpoint (OpenAI-compatible or Anthropic-compatible) and its API key
- Rust 1.99 and Python 3 with `venv`

## 0 · Install

```console
$ cargo install --path crates/ms-cli
$ cargo install --path crates/ms-supervisor
$ docker build -t microswarm-agent:dev -f images/agent.Containerfile .
$ mkdir -p "$HOME/microswarm-work/src" "$HOME/microswarm-work/tests" "$HOME/microswarm-work/target"
$ printf '%s\n' 'pub fn authorize(role: &str) -> bool { role == "admin" }' > "$HOME/microswarm-work/src/auth.rs"
$ printf '%s\n' '#!/bin/sh' 'test -f /work/src/auth.rs && echo "1 test passed"' > "$HOME/microswarm-work/tests/test.sh"
$ microswarm init --lang en
$ microswarmd & MS_DAEMON_PID=$!
$ microswarm doctor
✓ configuration   $MS_HOME
✓ runtime socket  unix:///var/run/docker.sock
✓ agent image     microswarm-agent:dev
✓ model           example-model via session proxy
✓ credential      model-key is present on the host
```

In the init wizard, choose the model endpoint, enter the API key when prompted,
and mount the absolute path to `~/microswarm-work` at `/work` as read-only.
Before starting the supervisor, set `lead.scope.delegation` in
`~/.microswarm/microswarm.toml` to `{ enabled = true, max_members = 2 }` and add
`{ host = "/absolute/path/to/microswarm-work/target", path = "/work/target", mode = "rw" }`
to `lead.scope.mounts`. The example test script runs inside the scratch agent
image with `/bin/sh`.

## 1 · Single agent (US1)

```console
$ microswarm chat
you › list the files in my work folder
⚠ agent wants to use microswarm-tools/fs.list
you › /approve K7QF
agent › README.md, src/, Cargo.toml
```

Verify:

```console
$ microswarm sessions
01JB… lead  ready  cli  containers: 1
$ find ~/.microswarm/sessions/01JB…/ -maxdepth 2 -type f
control/inbound.db  control/scope.json  out/events.db  out/outbound.db
$ docker ps --filter label=ms.session=01JB… --format '{{.Names}}'
ms-agent-01JB…
```

Isolation check — the agent cannot reach outside its mounts:

```console
you › read /etc/hostname using the file tool
⚠ agent wants to use microswarm-tools/fs.read
you › /approve <code>
agent › The file tool cannot read outside /work.
$ microswarm trace 01JB… --json | jq '[.[] | select(.type == "tool.result" and .data.ok == false)] | length'
1
```

## 2 · Swarm (US2)

```console
$ microswarm skills install examples/skills/code-review-team
$ microswarm chat
you › review the auth module and run its tests
⚠ [lead] wants to start a member 'tester'
  task:   run /bin/sh /work/tests/test.sh
  mounts: /work ro · /work/target rw
  tools:  shell.run, fs.read
  credentials: model-key
  /approve K7QF   /deny K7QF
you › /approve K7QF
agent › Researcher found two issues; tests pass (1/1). Details: …
```

Verify scope narrowing and teardown:

```console
$ microswarm trace 01JB… --events delegation.request,scope.narrowed,session.closed
$ jq '.mounts, .narrowed' ~/.microswarm/sessions/01JD…/control/scope.json
$ docker ps --filter label=ms.managed=true --format '{{.Names}}' | grep '01JB…'
ms-agent-01JB…
```

## 3 · Failure containment (US3)

```console
$ microswarm chat
you › ask a tester to run /bin/busybox sleep 60
$ docker rm -f ms-agent-<member-session-id>
$ microswarm trace 01JB… --events container.exited,delegation.failed
```

A forced container removal while the supervisor is running is classified as a
safety exit: `container.restart` count is `0`, and the lead receives
`member_failed{reason: safety}`. For transient recovery, stop the supervisor,
remove the lead container, and restart the supervisor; expect one
`container.restart` and a ready lead session. The strict US3 integration tests
exercise both paths with `MS_REQUIRE_RUNTIME=1`.

## 4 · Supervisor restart (US4)

```console
# with a lead, one member running, and an approval pending:
$ kill -9 "$MS_DAEMON_PID"
$ microswarmd & MS_DAEMON_PID=$!
$ microswarm trace 01JB… --events container.adopted,approval.expired,approval.reissued
```

Expect: every running container adopted, zero `container.orphan_stopped` for
matched sessions, the old approval expired, and `approval.reissued` with a new
code. The CLI does not display a reissued prompt after it disconnects; retrieve
the new code from the trace and approve it in a new chat session:

```console
$ microswarm trace 01JB… --json | jq -r '[.[] | select(.type == "approval.reissued")][-1].data.code'
$ microswarm chat
you › /approve <new-code>
```

## 5 · Add a tool without rebuilding (US5)

```console
$ export WORK=/absolute/path/to/the-host-directory-mounted-at-work
$ cp tests/fixtures/us5_stdio_mcp.sh "$WORK/weather_mcp.sh"
$ export MS_HOME=$(mktemp -d)
$ microswarm init --home "$MS_HOME" --lang en
$ microswarm tools add weather --stdio "/bin/sh /work/weather_mcp.sh"
$ # choose the same model and work mount in the wizard; grant weather in [lead.scope].tools, then:
$ microswarmd & MS_DAEMON_PID=$!
$ microswarm chat
you › what's the weather in Herzliya?
agent › 27 C and clear.
$ microswarm tools list
weather  stdio  /bin/sh /work/weather_mcp.sh
$ microswarm tools remove weather
```

The configured host work directory must be mounted at `/work`. This fixture uses
the shell already included in the agent image, so no image rebuild or `cargo`
command is needed. Existing sessions keep their original scope, so §5 uses a
fresh `MS_HOME` to create a lead with the new grant. Add the grant as
`{ server = "weather", tools = "*", policy = "allow" }`.

Telegram and webhook channels can be enabled in `microswarm.toml`:

```toml
[[channels]]
kind = "telegram"
token = "telegram-bot"          # credential name; set with microswarm creds

[[channels]]
kind = "webhook"
listen = "127.0.0.1:8788"
```

Webhook clients `POST /messages` with `{"thread_id":"build-42","text":"hello"}`;
the response contains the agent reply. The listener stays on loopback, so use a
TLS reverse proxy for external delivery.

## 6 · Trace (US6)

Use a session ID from `microswarm sessions` in the active `MS_HOME`.

```console
$ microswarm trace 01JB… --json > trace.json
$ python3 -m venv .venv && .venv/bin/pip install jsonschema
$ .venv/bin/python - <<'PY'
import json
from jsonschema import Draft202012Validator
schema = json.load(open('specs/001-microswarm/contracts/events.schema.json'))
events = json.load(open('trace.json'))
validator = Draft202012Validator(schema)
for event in events:
    validator.validate(event)
print(f'{len(events)} events valid')
PY
```

## Performance smoke (SC-001…SC-003)

```console
$ cargo run -p ms-bench -- spawn --n 100
spawn→agent.ready  p50 118 ms  p95 302 ms
image 27.4 MB  binary 11.8 MB  idle RSS 14 MB
```

Figures above are illustrative targets, not measurements.
