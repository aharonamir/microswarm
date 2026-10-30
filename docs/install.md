# Install and first run

## Requirements

- Linux x86_64 or aarch64 host.
- Docker Engine with its Unix socket available to the user running `microswarmd`.
- Rust toolchain used by this repository.
- An OpenAI-compatible or Anthropic-compatible model endpoint and a credential
  for that endpoint.

Docker is the supported runtime. Podman support is deferred because the
supervisor's isolated network path to the credential proxy is not verified for
Podman.

## Build and initialize

From a checkout of MicroSwarm:

```sh
cargo install --path crates/ms-cli
cargo install --path crates/ms-supervisor
docker build --tag microswarm-agent:dev --file images/agent.Containerfile .
microswarm init --lang en
```

`MS_HOME` selects the home directory. If unset, it defaults to
`$HOME/.microswarm`. `microswarm init` writes configuration files directly in
that directory. The daemon also accepts a `config/` subdirectory when all four
configuration files are stored there; use one location consistently.

Edit `microswarm.toml` to configure the model, mount a working directory, and
grant the model credential name in the lead scope. The complete field list is
in the [configuration reference](configuration.md).

Store the credential value through standard input; do not put it in a command
argument or a configuration file:

```sh
microswarm creds set model-key
# paste the value, then press Ctrl-D
```

Start the supervisor and verify its configuration:

```sh
microswarmd
```

In another terminal:

```sh
microswarm doctor
microswarm chat
```

Use `microswarm sessions` to see active sessions and
`microswarm trace <session_id>` to inspect a session tree. See
[`specs/001-microswarm/quickstart.md`](../specs/001-microswarm/quickstart.md)
for the full multi-agent acceptance flow.

## Home directory layout

```text
$MS_HOME/
  microswarm.toml       # model, runtime, lead scope, channels, limits
  policy.toml           # spawn and tool policy
  ceiling.toml          # system-wide scope ceiling
  tools.toml            # additional MCP servers
  supervisor.db         # authoritative session, scope, and event records
  credentials/          # host-only credential values
  sessions/<ULID>/      # isolated per-session state and agent output
```

The supervisor removes session directories after their configured retention
period. Session rows remain in `supervisor.db` so the host record remains
authoritative.
