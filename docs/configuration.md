# Configuration reference

`microswarm init` creates `microswarm.toml`, `policy.toml`, `ceiling.toml`, and
`tools.toml` in `$MS_HOME` (default `~/.microswarm`). The supervisor also reads
those four files from `$MS_HOME/config` when that directory contains
`microswarm.toml`.

## `microswarm.toml`

```toml
[home]
retention_days = 30
lang = "en" # en or zh

[runtime]
socket = "unix:///var/run/docker.sock"
image = "microswarm-agent:dev"

[model]
provider = "openai-compatible" # or anthropic
base_url = "https://model.example/v1"
model = "model-name"
credential = "model-key" # name stored with `microswarm creds set`

[lead.scope]
mounts = [{ host = "/home/me/project", path = "/work", mode = "rw" }]
credentials = ["model-key"]
tools = [{ server = "microswarm-tools", tools = "*", policy = "ask" }]
delegation = { enabled = false, max_members = 0 }

[limits]
max_sessions = 200
max_members_per_lead = 8
member_idle_s = 900
lead_idle_s = 86400
grace_ms = 5000
approval_timeout_s = 600
restart_max = 3
memory_mb = 256
pids = 64
tmp_mb = 128
max_turns = 30
max_tokens = 65536

[[channels]]
kind = "cli"
```

`provider` is `openai-compatible` or `anthropic`. The model endpoint must use
HTTPS except for a local loopback development endpoint. `credential` is a
credential **name**, not a key value. That name must also appear in
`lead.scope.credentials` so the supervisor proxy can authorize model access.

Each mount maps an absolute host directory to `/work`, `/skills`, or `/data` in
the container. Modes are `ro` and `rw`. Delegation is disabled by default.
Setting it to enabled permits the lead to request members; the effective member
scope is still intersected with the lead scope and system ceiling.

The channel kinds are `cli`, `telegram`, and `webhook`. Telegram's `token` is
the name of a host credential. Webhook `listen` must be loopback, for example
`127.0.0.1:8788`; put a TLS-terminating reverse proxy in front if remote access
is needed. `api_base` can override Telegram's Bot API endpoint.

## `policy.toml`

```toml
[spawn]
default = "ask"
roles.researcher = "allow"

[tools.defaults]
"microswarm-tools/fs.read" = "ask"
"microswarm-tools/fs.write" = "ask"
"microswarm-tools/shell.run" = "ask"
```

Spawn policy values are `allow`, `ask`, and `deny`. A role-specific spawn
policy can tighten the default. Tool decisions can be overridden per tool in
`lead.scope.tools[].overrides`; decisions are `allow`, `ask`, or `deny`.

## `ceiling.toml`

```toml
paths = ["$MS_HOME", "/var/run", "/run", "/etc/shadow", "~/.ssh"]
credentials = []
tools = []
```

The ceiling is a deny boundary. A requested scope never widens the lead scope
or this ceiling. Do not add a path, credential, or tool to the ceiling to grant
it to a session; grants belong in the lead scope and are still subject to the
ceiling rules.

## `tools.toml`

```toml
[[servers]]
id = "weather"
transport = "stdio"
command = "python3"
args = ["/opt/tools/weather_mcp.py"]

[[servers]]
id = "catalog"
transport = "streamable_http"
url = "https://tools.example/mcp"
```

Register servers with `microswarm tools add`, inspect them with `tools list`,
and remove them with `tools remove`. Grant each server and its allowed tool
names in `lead.scope.tools`. Standard input/output servers run inside the agent
container; use executables and dependencies present in the configured image.
HTTP MCP servers must use HTTPS and are reached through the session proxy.
