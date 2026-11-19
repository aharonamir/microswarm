# Contract: Step sentences

The server maps events to `StepSentence {key, args}`. The client renders `t(key, args)` and wraps each argument marked *mono* in a `<code>` element, created through `dom.js`, never through string HTML.

Argument rules: single line; control characters removed; at most 120 characters, with "…" when cut. Paths are shown relative to the mount root when shorter (`/work/src/auth/refresh.rs` becomes `refresh.rs` in the sentence, and the full path goes in the inspector facts). Counts are integers.

## Tools (`tool` steps)

| server / tool | key | args (*mono*) | en |
|---|---|---|---|
| any / `fs.read`, `read_file` | `step.read` | *path* | Read {path} |
| any / `fs.read` ×N folded | `step.read_many` | *first*, n | Read {first} and {n} more |
| any / `fs.list` | `step.list` | *path*, count | Listed {path} · {count} entries |
| any / `fs.write`, `write_file` | `step.write` | *path* | Wrote {path} |
| any / `fs.edit` | `step.edit` | *path* | Edited {path} |
| any / `grep`, `search_files` | `step.search` | *pattern*, count | Searched for {pattern} · {count} matches |
| any / `shell.run` | `step.shell` | *command* | Ran {command} |
| any / `shell.run` (decision ask, pending) | `step.shell.ask` | *command* | Wants to run {command} |
| any / `web.fetch` | `step.fetch` | *host* | Opened a page on {host} |
| any / `web.search` | `step.web_search` | *query* | Searched the web for {query} |
| other | `step.tool` | *server*, *tool* | Used {server}/{tool} |

**Folding:** consecutive allowed `fs.read` steps by the same agent within 5 s fold into one `step.read_many`, which keeps all its `event_seqs`.

## Delegation and messages

| event(s) | agent | key | en |
|---|---|---|---|
| `delegation.request`+`spawned` | lead | `step.spawn` | Started {role} with {reach} |
| `delegation.request`+`denied` | lead | `step.spawn.denied` | Couldn't start {role}: {reason} |
| `delegation.send` | lead | `step.send` | Sent {role} new instructions |
| `delegation.reply` | member | `step.report` | Reported to the lead: {status}, {summary} |
| `delegation.dismiss(ed)` | lead | `step.dismiss` | Dismissed {role} |
| `delegation.timeout` | member | `step.timeout` | Stopped: no reply within {timeout} |
| `message.inbound` (operator) | you | `step.you_said` | You asked: “{text}” (first 140 chars, as text) |
| lead `turn.end` with reply | lead | `step.replied` | Replied to you |
| `approval.requested` | you | `step.approval.asked` | {role} asked you to approve: {what} |
| `approval.resolved` | you | `step.approval.decided` | You {approved/denied} it on {channel} |
| `approval.expired` | you | `step.approval.expired` | The request expired |
| `agent.exit` non-ok | agent | `step.exit.<reason>` | Stopped: {reason words} |
| unknown | agent | `step.other` | Did something the console can't describe (shown under Everything only; Raw has the event) |

`{reach}` is the ReachSummary rendered short: "read access to `/work/src/auth`", "no file access", and so on.

## Status words for decisions

`decision.allowed` "allowed", `decision.asked_you` "waiting for you", `decision.approved` "approved by you", `decision.denied_by_you` "denied by you", `decision.denied_by_policy` "denied by policy", `decision.failed` "failed", `decision.timed_out` "timed out", `decision.reduced` "reduced", `decision.expired` "expired".

Tone: allowed/approved are neutral; asked_you is attention (seal); reduced is amber; denied/failed/timed_out/expired are seal-ink text on a neutral pill. Every pill always shows its words.
