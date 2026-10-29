# Contract: CLI and configuration

## Binaries

| Binary | Runs on | Purpose |
|---|---|---|
| `microswarmd` | host | Supervisor daemon |
| `microswarm` | host | Operator CLI (talks to `supervisor.db` read-only and to `microswarmd` over a local Unix socket) |
| `microswarm-agent` | container | Agent core |
| `microswarm-tools` | container | First-party MCP server: `fs.*`, `shell.run`, `web.fetch` |

## `microswarm` commands

```text
microswarm init [--home DIR] [--lang en|zh] [--provider P --api-key-ref NAME] [--verify]
microswarm doctor                         # runtime socket, image present, model reachable via proxy, ports, disk
microswarm chat [--session ID]            # CLI channel
microswarm sessions [--all] [--json]
microswarm trace <session_id> [--json] [--events TYPES]
microswarm approve <code> | deny <code>
microswarm stop <session_id>              # graceful; members first, then lead
microswarm skills install <dir> | list | remove <name>
microswarm tools add <id> --stdio "<cmd>" | --http <url> ; tools list ; tools remove <id>
microswarm creds set <name> ; creds list  # value read from stdin; stored by supervisor only
```

Exit codes: `0` success, `1` usage error, `2` not found, `3` supervisor unreachable, `4` validation error.

### `trace` output (text)

```text
01JB… lead  ready   scope: 2 mounts · 3 tools · 1 cred
├─ turn 1  model.call 1.2s  in 812 / out 140
│  ├─ tool.call  microswarm-tools/fs.read  /work/README.md       allow  12ms ✓
│  └─ delegation.request researcher  → spawned 01JC…  narrowed: 1
├─ 01JC… researcher  closed(dismissed)  scope: 1 mount(ro) · 2 tools · 1 cred
│  ├─ turn 1  model.call 2.4s
│  │  └─ tool.call  microswarm-tools/web.fetch  https://…        allow  340ms ✓
│  └─ reply → lead
└─ turn 2  reply → channel
```

`--json` emits an array of events conforming to `events.schema.json`, ordered by `(ts_mono, source, seq)`.

## `microswarm.toml`

```toml
[home]
retention_days = 30
lang = "en"                       # en | zh — user-facing supervisor strings

[runtime]
socket = "unix:///var/run/docker.sock"
image  = "ghcr.io/microswarm/agent:0.1"

[limits]
max_sessions       = 200
max_members_per_lead = 8
member_idle_s      = 900
lead_idle_s        = 86400
grace_ms           = 5000
approval_timeout_s = 600
restart_max        = 3
memory_mb          = 256
pids               = 64
tmp_mb             = 128
max_turns           = 30
max_tokens          = 65536

[model]
provider   = "openai-compatible"  # or "anthropic"
base_url   = "https://api.example.com/v1"
model      = "example-model"
credential = "model-key"          # name; value stored via `microswarm creds set`

[lead.scope]                      # the default lead scope (derived_from = config)
mounts      = [{ host = "~/work", path = "/work", mode = "rw" }]
credentials = ["model-key", "web-search"]
tools       = [{ server = "microswarm-tools", tools = "*", policy = "ask" }]
delegation  = { enabled = true, max_members = 4 }

[[channels]]
kind = "cli"

[[channels]]
kind  = "telegram"
token = "telegram-bot"            # credential name
```

## `policy.toml`

```toml
[spawn]
default = "ask"                   # for roles not in any team skill ('adhoc')
[spawn.roles]
researcher = "allow"

[tools.defaults]                  # applied when scope omits per-tool policy
"microswarm-tools/fs.read"  = "allow"
"microswarm-tools/fs.write" = "ask"
"microswarm-tools/shell.run" = "ask"
```

## `ceiling.toml`

```toml
# Never grantable to any container, whatever a skill or lead requests.
# The session's own directory is mounted at /session by the supervisor outside scope
# computation, so listing $MS_HOME here does not block it.
paths       = ["$MS_HOME", "/var/run", "/run", "/etc/shadow", "~/.ssh", "~/.aws", "~/.config/gcloud"]
credentials = []                  # names that may only be used by the supervisor itself
tools       = []
```
