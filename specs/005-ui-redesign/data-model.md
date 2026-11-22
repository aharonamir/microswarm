# Data model: Console UI redesign

These are view models, computed in `ms-console` from data it already reads. Nothing here is stored. Sources:
- `supervisor.db`: `sessions`, `delegations`, `approvals`, `supervisor_events`;
- each session's `out/events.db` (read-only);
- `ms_runtime::trace::build`, the merged gapless trace.

Identifiers: `id` fields are the full ULIDs, used for routing only. The UI shows roles, never ids, except a shortened `01ARZ3…FAW` inside "Raw".

## Agent

```
Agent { id, kind: "lead"|"member", role, colour_index: 0..6, initial }
```
- `colour_index`: 0 for the lead. Members get 1–6 in order of their `delegation.spawned` time within the root session, wrapping after 6.
- `initial`: the first letter of the role, uppercased. If two agents share an initial, the second takes the first two letters ("Te", "Tr").

## TeamGroup and MemberRow (Team card)

```
TeamGroup { group_id, anchor_message_id?, started_at, ended_at?, members: [MemberRow], more: int }
MemberRow {
  agent: Agent, delegation_id, task, task_truncated: bool,
  status: StatusWords,          // reuse 002 StatusWords
  state: "starting"|"working"|"waiting_you"|"waiting_lead"|"reported"|"dismissed"|"failed"|"denied"|"timed_out",
  step_count, started_at, ended_at?, elapsed_ms,
  current_step?: StepSentence,  // newest step while working
  pending_approval?: { id, code, summary: StepSentence },
  reach_summary: ReachSummary,
  reduced_count: int,
  report_excerpt?: string       // first 280 chars of the last delegation.reply text
}
```

**Grouping.** Delegations whose `delegation.spawned` events fall in the same lead turn (between one `turn.start` and the next `turn.end` of the lead) form one group. The anchor is the last operator message before that `turn.start`. If no anchor is found, the group is placed by time.

**State mapping:**

| `delegations.status` / member session | `state` |
|---|---|
| requested, spawning | `starting` |
| awaiting_approval | `waiting_you` (spawn approval) |
| active, with a pending tool approval | `waiting_you` |
| active, no turn open, lead hasn't replied | `waiting_lead` (shown as "idle") |
| active, turn open | `working` |
| waiting_reply, last event is `delegation.reply` | `reported` |
| dismissed | `dismissed` |
| denied | `denied` |
| failed, with `failure_reason = timeout` | `timed_out` |
| failed, other reasons | `failed` |

## Step and Decision

A step is one thing an agent did, paired from events.

```
Step {
  step_id,                  // "<session_id>:<seq of first event>"
  agent: Agent, at, ended_at?, duration_ms?,
  kind: "tool"|"message_in"|"report"|"spawn"|"dismiss"|"approval"|"model"|"exit"|"reduced",
  sentence: StepSentence,
  decision?: Decision,
  approval?: { id, code, state },
  phase_id,
  event_seqs: [int]         // all events folded into the step (shown in Raw)
}
StepSentence { key: string, args: { [name]: string } }   // contracts/step-sentences.md
Decision { value: "allowed"|"asked_you"|"approved"|"denied_by_you"|"denied_by_policy"|"failed"|"timed_out"|"reduced"|"expired",
           rule?: string, source?: "policy"|"team_skill"|"ceiling"|"operator" }
```

**Pairing:**
- `tool.call` + `permit.decision` + `tool.result` with the same `call_id` form one `tool` step. `duration_ms` comes from `tool.result`.
- When the decision is `ask`, the matching `approval.requested`/`approval.resolved` (by session and argument hash) attaches as `approval`, and the decision becomes `approved`, `denied_by_you` or `expired`.
- `delegation.request` (lead) + `delegation.spawned`/`delegation.denied` (supervisor) + `scope.narrowed` form one `spawn` step on the lead. If anything was narrowed, the decision is `reduced`.
- `delegation.reply` forms a `report` step on the member.
- `model.call` events are not steps. They are counted in tiles only.
- Events that match no rule become a step with sentence key `step.other` and are visible only under the "Everything" filter.

## Phase

```
Phase { phase_id, title_key, title_args, started_at, ended_at?, step_ids: [] }
```

Steps are walked in order, with a running set of working agents. A new phase starts:
- at each operator `message.inbound` (title `phase.lead_works`, or `phase.lead_plans` if a spawn follows in the same turn);
- when the set of working members goes from empty to non-empty (`phase.parallel` with count N, or `phase.one_member` with role);
- when it goes from non-empty to empty while the lead continues (`phase.lead_checks`);
- while an approval is pending and no agent works (`phase.waiting_you`).

## Timeline (lanes)

```
Timeline { range: {start, end}, stats: Stats, lanes: [Lane] }
Lane  { agent: Agent | {kind:"you"}, spans: [Span], marks: [Mark] }
Span  { start, end, state: "working"|"waiting_you"|"idle", step_id? }
Mark  { at, kind: "message"|"report"|"approval"|"spawn"|"denied"|"tool", step_id }
Stats { duration_ms, agents, steps, waiting_you_ms, reduced, denied_or_failed, model_calls }
```

- **Working spans** run from `turn.start` to `turn.end` per agent, with gaps under 2 s merged.
- **Waiting-you spans** run from `approval.requested` to `approval.resolved`/`expired`, and supersede working.
- **Tool marks** are drawn as ticks, and capped at 200 per lane (evenly sampled), with the count in Stats.
- The **You lane** has `message` marks for operator messages and `approval` marks where a decision was asked of you or made.

## Reach

```
Reach {
  summary: { key, args },                // e.g. reach.summary.one_folder_no_net_no_creds
  can_reach: [ReachItem], reduced: [Reduction],
  derivation: { requested: [Chip], lead: [Chip], ceiling: [Chip], granted: [Chip] },
  container: { files_key, process_key, network_key, args },
  raw: { granted, requested, lead, ceiling }   // for the Raw disclosure only
}
ReachSummary { folders: [{path, mode}], network: "none"|"proxy_only"|"hosts", credentials: [name], tools_ask: [name] }
ReachItem { icon: "folder"|"tool"|"credential"|"network", text: {key,args}, tag: "read only"|"read and write"|"allowed"|"asks you" }
Reduction { dimension, item, reason, text: {key,args} }
Chip { text, struck: bool, lowered: bool }
```

Reason words are i18n keys `reach.reason.<reason>`, matching `ms_proto::NarrowedReason`: `not_in_lead_scope`, `ceiling`, `mode_downgraded`, `policy_tightened`, `limit_capped`.

## Explanation (inspector)

`Explanation { heading_key: "why.stopped"|"why.allowed"|"why.denied"|"why.failed"|"why.reduced", body: {key,args}, facts: [{label_key, value, mono: bool}] }`

| Decision | body key | args |
|---|---|---|
| allowed | `why.allowed.rule` | rule, role |
| asked_you (pending) | `why.ask.pending` | tool, role |
| approved / denied_by_you | `why.ask.decided` | by ("you"), channel, at |
| denied_by_policy | `why.deny.rule` | rule, source |
| reduced | `why.reduced` | count, first reason |
| failed | `why.failed` | error (one line, ≤ 200 chars, as text) |
| timed_out | `why.timeout` | timeout |

## HomeView

```
HomeView {
  summary: { needs_you: int, running: int },
  needs_you: [{ approval_id, code, session_id, session_title, agent: Agent, sentence: StepSentence, runs_in: ReachSummary, expires_at }],
  running: [{ session_id, title, channel, status: StatusWords, agents: [Agent], latest: {who, text_or_sentence}, started_at }],
  ended_today: [{ session_id, title, channel, outcome_key, ended_at, agents: [Agent], last_line }],
  system: { containers, approvals_today, supervisor_started_at }
}
```
`latest.text_or_sentence` is either the first line of the newest lead message (plain text, ≤ 140 characters) or the newest member StepSentence.
