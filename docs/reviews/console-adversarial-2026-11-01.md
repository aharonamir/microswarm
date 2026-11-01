# Console adversarial review

Date: 2026-11-01  
Scope: `crates/ms-console`, its embedded web assets, the shared shell parser, and the supervisor RPC calls used by the console. This is a source and test review; it does not replace the browser E2E and operator usability checks listed in `specs/002-console/tasks.md`.

## Results against Principles I–IX

| Principle | Evidence | Result |
|---|---|---|
| I. Small enough to audit | `scripts/loc-budget.sh`: console 2,134 Rust lines / 4,000; web 594 lines / 3,500. The console has no model or MCP client dependency. | Pass |
| II. Isolation first | `crates/ms-console/src/rpc.rs` connects only to `$MS_HOME/supervisor.sock`; database opens in `src/read.rs` use `SQLITE_OPEN_READ_ONLY`. No runtime socket client exists in the crate. | Pass |
| III. Supervisor is the only authority | Chat, approval decisions, end-session, credentials, and skill installation go through supervisor RPC. Status, scope, and approvals are read from supervisor-owned state. | Pass |
| IV. Default deny and intersect | The console cannot grant tools or scope. Approval decisions pass only `{op, code}`; requested and granted scopes are presented from stored records. | Pass |
| V. Protocol boundaries | HTTP JSON is guarded before route dispatch; supervisor communication uses newline-delimited JSON over the Unix socket; live events use framed SSE. | Pass |
| VI. Events at the source | `rpc::follow` forwards supervisor and agent ledger events and derives `status.changed` from the supervisor commit state. Agent events are labelled observations in the Sessions feed. | Pass |
| VII. Fail closed | Invalid IDs and cursors are rejected; unknown statuses map to an unknown status word; approval failures map to conflict, gone, or unavailable responses; RPC errors do not expose raw supervisor errors. | Pass |
| VIII. Core stays dumb | The console depends on `ms-runtime` read/profile helpers and the dependency-free `ms-shell` parser; it has no model or MCP client. | Pass |
| IX. Operator surfaces hold no authority | The console stores only its short-lived auth session and chat job locks. The supervisor validates all state-changing operations; credential values are not returned or logged by console code. | Pass |

## Web and HTTP checks

- `scripts/web-lint.sh` passes. Web content is built through `web/js/dom.js`; forbidden HTML sinks, inline scripts, and inline style attributes are absent.
- `crates/ms-console/tests/auth.rs` covers loopback binding, single-use ten-minute startup tokens, cookie flags, Host and Origin checks, the console POST header, content type, and body size.
- `crates/ms-console/tests/headers.rs` checks the exact CSP and security headers on HTML, API, and static asset responses.
- `crates/ms-console/tests/xss.rs` verifies script, `javascript:` and data-URL strings remain inert JSON/SSE data. `tests/no_secrets.rs` verifies a credential value is absent from API, SSE, host view, and console output paths.
- Static assets and fonts are embedded in the binary; there is no frontend build step and no npm runtime dependency.

## Findings and remaining validation

No authority-boundary or credential-disclosure defect was found in the reviewed console paths. TOML document loading initially fell back to an empty configuration because `toml::Value::FromStr` parses a value rather than a document; `read_config_value` now parses a TOML table, and `api_host` checks doctor parity against the shared runtime doctor.

The `status.changed` delivery test measured 298 µs from a fake supervisor event write to the derived console event. It does not include the database commit or combined 50-session follower CPU cost. The incremental fixture measured 4.7% of one core for the follower alone and 3.2% for the combined merge plus one console follower over 60 seconds. Browser SSE client fan-out remains outside this CPU fixture.

The Playwright/axe browser suite passed for all six page states at 1440 px and 390 px, including the real-browser hostile-text check. Its latest local startup-to-render sample was 143 ms, and the 20-sample synthetic `status.changed` browser DOM p95 was 28 ms. Native Chinese review, three-person usability, and the clean-host full quickstart remain separate acceptance checks; they are not represented as source-review passes.
