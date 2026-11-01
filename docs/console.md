# Operating MicroSwarm Console

Start the supervisor, then run:

```sh
microswarm console
```

The console binds to `127.0.0.1:8790` by default and prints a one-time sign-in link. Open it once within ten minutes. The link is exchanged for an HttpOnly, SameSite=Strict cookie and removed from the address bar. To reach a remote host, forward the local port with SSH:

```sh
ssh -L 8790:127.0.0.1:8790 operator@swarm-host
```

Then open the printed loopback link on the local machine. The console refuses non-loopback listen addresses.

## Sessions and channel behavior

Sessions lists active lead sessions and lets an operator start a new session on the `web` channel, send messages to a lead, inspect its team, and end the session. Session history is reconstructed from the lead's inbound and outbound ledgers. Member replies appear as separate lead messages labelled with the member role. Agent event text is displayed as an observation.

Messages sent in the console stay in the console. They do not get mirrored to the session's original Telegram or webhook channel. Telegram continues waiting for a reply when a Telegram session is paused for approval, so an approval made in the console still reaches that Telegram conversation. The webhook adapter waits for a follow-up only when its own client resolves the approval; a console decision leaves that follow-up visible in the console.

## Approvals, scope, trace, and host

Approvals shows the exact bound server, tool, arguments, argument hash, parsed shell command, and requesting member scope. Approve and deny actions send the short approval code to the supervisor, which validates the bound call. A resolved or expired approval cannot be applied again.

Member pages compare the requested scope, lead scope, granted scope, narrowed items, and the container profile derived by the shared runtime profile builder. Trace exports the shared runtime trace as JSON. Host shows the CLI doctor checks, configured channels, skill names, credential names, limits, and ceiling. It also accepts credential values and local skill directory paths.

## Security model

The console is an operator client of the supervisor. It sends state-changing operations only over the supervisor Unix socket. It reads `supervisor.db`, session databases, and configuration read-only; it does not connect to the container runtime socket. The supervisor remains the source of authority for status, approvals, scope, chat, shutdown, credential storage, and skill installation.

Credential values are never included in host views, API responses, event frames, or console logs. The console only lists credential names. Agent-originated text and event payloads are untrusted and are rendered as text. The web app has a strict Content Security Policy, builds elements through its DOM helper, and rejects cross-origin POSTs and requests without its console header.
