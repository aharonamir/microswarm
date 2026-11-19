# MicroSwarm system prompts

The system prompt is assembled by `ms-agent` at boot, once per container, from these parts in this order. Static text comes first, so providers that cache prompt prefixes can reuse it across sessions.

| # | Part | Lead | Member | Source |
|---|---|---|---|---|
| 1 | `base.md` | ✓ | ✓ | embedded in the binary (`include_str!`) |
| 2 | `lead.md` or `member.md` | lead.md | member.md | embedded |
| 3 | Instructions | operator's `config/lead.md`, if present, under the heading "## Operator instructions" | the team skill's `role.md`, if present, under the heading "## Role instructions" | supervisor copies it into the session's `control/` at creation (read-only, immutable for the session) |
| 4 | Environment | ✓ | ✓ | rendered from `control/scope.json`, `MS_KIND`/`MS_ROLE` env, and (lead only) `control/team.json` |

## Trust order

`base.md` defines it: base, then role section, then instruction sections (operator instructions, or third-party role instructions), then messages from the party the agent works for. Everything else is data. Role instructions come from a team-skill author, which makes them a supply-chain input (ClawHavoc showed malicious skills at scale). They are rendered under the heading "## Role instructions (from team skill `<name>@<version>`)" so the model knows their source, and they can describe the job but never override base.

## Rendering safety

Several values that go into the system prompt come from people other than the operator:
- skill names, role names and descriptions (skill authors);
- MCP tool names (MCP server authors);
- member mount paths (requested by a model that may have read injected content).

Before rendering, every value is:
- reduced to one line: control characters and newlines removed;
- stripped of backticks, `{{`/`}}`, and leading `#`, `>` or `-`;
- capped: description 200 characters, names 64, paths 256;
- for descriptions only: wrapped in quotes.

A value that changes under sanitisation is logged in `agent.ready` as `prompt_sanitised: [field…]`. Whole instruction files (`instructions.md`) are inserted verbatim under their labelled heading, size-capped (8 KB), with any line starting `## ` demoted to `### `, so they can't open a fake section such as "## Operator instructions".

## Rules

- **Precedence:** `base.md` states that parts 3 and 4 may narrow or specialise behaviour, but cannot widen access or switch off the "Tools" and "Untrusted content" rules. Access is enforced by the container, the permission engine and the proxy whatever the prompt says. The prompt only makes the model's behaviour match the walls.
- **No secrets:** the environment block lists credential **names** only. A test asserts that no credential value, proxy URL token or host path appears in any assembled prompt.
- **Container paths, not host paths:** the environment shows `/work/...` as the agent sees it. Host paths never appear.
- **Size budget:** parts 1, 2 and 4 together are ≤ 16,000 characters; v3 measures about 14,300 for a lead and 10,000 for a member, before environment values are filled in. Part 3 is capped separately at 8 KB; past the cap it is truncated with a marker, and `prompt_truncated` goes into `agent.ready`. The whole prompt therefore stays under about 24,200 characters (≈ 6,000 tokens). For comparison, Anthropic's research lead prompt is about 3,500 words.
- **Fixed for the session:** nothing per-turn goes into the system prompt. The start time is rendered once.
- **Language:** the prompts are in English and instruct the model to reply in the language it is addressed in, which works for the en/zh operators MicroSwarm targets. Add translated variants only if evaluation shows a measurable gain for a zh-first model.
- **Versioning:** each file starts with a `vN` comment, which is stripped before sending. `agent.ready` gains `prompt_version` and `prompt_sha256`, so the trace shows which prompt a session ran with.
- **Rendering:** `environment.tmpl.md` shows the output shape. Render it in Rust with plain string building, not a template engine dependency.

## Why it looks like this

| Choice | Source |
|---|---|
| Lead works by default; delegates only for isolation, parallelism, context or a fitting role, scaled 0 / 2–3 / more | Anthropic's multi-agent research system: explicit scaling rules stopped "50 subagents for simple queries". agent-core's subagent rules (use for ≥ 2 deep reads or ≥ 3 sources, not for single tool calls). Departs from agent-core's team leader, which may never execute work, because a MicroSwarm member costs a container |
| Task brief = objective, context, boundaries, output, done-when, with a good/bad example | Anthropic: vague delegation caused duplicated work and gaps. DeepSeek Harness: fix the scope before fan-out, list each worker's task. agent-core leader: "define what and why, not how" |
| Least-privilege scope requests; read `narrowed` | MicroSwarm-specific: the scope intersection is the product's core guarantee, and the model should cooperate with it rather than discover it by failure |
| Member reports as evidence to verify; tool output is data | Security review residual risk 1 (agent output is untrusted); OWASP ASI01/ASI06 (goal hijack, poisoned context). DeepSeek Harness: "a JSON shape doesn't validate evidence; spot-check findings" |
| Member report contract: Status / Result / Evidence / Limitations / Artifacts | DeepSeek Harness worker contract (`files_read`, `commands_run`, `limitations`, `partial_result` instead of invented results) |
| Short results inline, long ones in a file with a path | agent-core leader and teammate policy: "the channel follows the shape of the content" |
| Messages to a member come from the lead, not a person | agent-core inbound tags (who said what). MicroSwarm delivers the lead's task as a `user` message, so the member must be told who that "user" is |
| Don't repeat a failed call with identical arguments | agent-core identity section |
| Plain text, no tables or headings | The console renders agent text as plain text (002 FR-C10/C11); Telegram and the CLI do the same |
| Base + role + operator instructions + generated environment | NanoClaw: a shared `CLAUDE.md`, plus a runtime append with the agent's name and destinations, plus a per-group `instructions.prepend.md`. agent-core: prompt sections with priorities |
| Plan first: straightforward / breadth / depth; dependency-first ordering; start independent members back to back, then follow up; stop at diminishing returns or near limits | Claude Cookbook orchestrator-workers pattern, `research_lead_agent.md` (query-type determination, plan execution, early termination). MicroSwarm's `spawn_member` waits for the first task reply and returns it as `initial_reply`, so initial tasks start sequentially even when they are independent. |
| Brief adds one core objective, key questions, starting points, which tools, source-quality guidance, effort level, and high information density | `research_lead_agent.md` delegation instructions. Adapted with data minimisation: pass only the context the task needs, not the whole conversation |
| Lead writes the final answer and never delegates synthesis; empty or off-task reports are failures; conflicting reports resolved by evidence or surfaced | `research_lead_agent.md` (synthesis responsibility, discrepancies); cookbook notebook failure modes (empty or malformed worker output) |
| Member plans a tool-call budget, works in an observe-decide-act loop, flags speculation and weak sources, reports unresolved conflicts | `research_subagent.md` (research budget, OODA loop, source quality) |
| Deliberate differences from the cookbook | It always uses at least one subagent and the lead does no primary research; MicroSwarm works in by default for trusted work (each member is a container) but forces untrusted input to members, which converges for web research. The cookbook prompts contain no prompt-injection, exfiltration or approval rules, so those parts of v2 are kept, and its all-caps emphasis ("MUST", "NEVER") is not copied: current Claude models follow calm instructions well and can overreact to shouted ones. XML section tags are not used, because the prompts must work across model providers and the sanitiser protects Markdown headings |
| Credentials by name only; never ask for secrets | NanoClaw container `CLAUDE.md` ("keep real credentials in the gateway"); MicroSwarm proxy injection |
