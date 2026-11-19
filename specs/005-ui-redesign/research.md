# Research: Console UI redesign

## R-U1 — How multi-agent tools present a team (2026)

**Decision:** one conversation with the lead, members shown inline as a Team card plus a persistent team panel; approvals pinned; a lane timeline for the trajectory.

**Evidence:**
- **Cursor Agents Window:** breadcrumb navigation into subagents hides the parent's context. Users asked for a persistent execution tree and split panes, and Cursor's answer ("Projects") is a nested tree of workers with live status beside the coordinator. [forum thread](https://forum.cursor.com/t/hierarchical-ui-concurrency-view-for-multi-agent-workflows-in-agents-window/171765)
- **Claude Code agent view:** one table of sessions with state, latest output and last interaction. "Waiting" is the state that matters most, and you can reply from a peek panel. [guide](https://www.buildfastwithai.com/blogs/claude-code-agent-view-guide)
- **Magentic-UI:** shows its plan and progress on the left, asks before consequential actions with Accept and Deny, and shows every intermediate step. [Microsoft Research](https://www.microsoft.com/en-us/research/blog/magentic-ui-an-experimental-human-centered-web-agent)
- **AgentGUI (2026):** a live activity feed plus a wall-clock overview per task. In its study, people found information 38% faster with 93% accuracy against 80% (N = 8, small). [paper](https://www.alphaxiv.org/abs/2607.26300)
- **Honeycomb Agent Timeline:** horizontal lanes per agent, failures highlighted inline, a "Show failures only" toggle, and a conversation summary header. [blog](https://www.honeycomb.io/blog/agent-timeline-flight-recorder-for-your-ai-agents)

**Rejected:**
- A node-graph canvas: harder to read at first glance, and it adds a layout engine.
- Separate chat tabs per member: members don't talk to the operator (core rule).

## R-U2 — Why 002's screens weren't understood

Rendering `main@b7fcb49` with a realistic fixture showed:
- member work absent from the conversation;
- the approval card hidden by the composer;
- raw event types and ISO times;
- JSON arrays on the scope page;
- disclosures that look like empty boxes.

Each problem maps to a requirement: FR-U05/U06, U07, U15/U16, U10 and U12.

## R-U3 — Where derivation runs

**Decision:** in `ms-console` (Rust), on request, memoised by last seq.

**Rationale:**
- The browser must not interpret raw events (FR-U15). That keeps one tested implementation and keeps the client thin.
- `ms_runtime::trace::build` already merges sessions gaplessly.
- Nothing is written, so supervisor authority is unchanged (Principle IX, FR-U20).

**Alternative rejected:** precomputing steps in the supervisor. That would add supervisor lines (budget) and a write path.

## R-U4 — CSP and dynamic geometry

The CSP forbids inline styles. CSSOM property writes (`el.style.left = "12.5%"`) aren't governed by `style-src` 'unsafe-inline'; style attributes in markup and `setAttribute('style', …)` are. So `dom.js` gets a `style: {left, width}` option that assigns properties one by one, and a unit test asserts it never calls `setAttribute('style')`.

## R-U5 — Phases

Phases come from operator messages and from changes in the set of working members (data-model §Phase). This yields the readable "plans / work in parallel / checks and replies" shape without any model or text analysis. When it fails (a lead that alternates rapidly), it degrades to "The lead works" phases, which are still correct.

## R-U6 — Agent colours

There are 6 member hues, chosen away from the status colours (red, teal, amber) and differing in lightness. The role initial and name always accompany the colour (WCAG 1.4.1). Colours are assigned by start order and stay stable for the session.

## R-U7 — Brand

The owner asked for a non-Chinese mark. The swarm mark is a centre (supervisor) linked to four agents. It is ink and teal, which keeps red free for "needs you". "MS" monogram was offered as an alternative and not chosen.

## R-U8 — Usability measurement

The 002 SC-C08 method is kept: three first-time participants, unaided, timed. 005 adds two tasks (SC-U02, SC-U03). Results go in `docs/reviews/console-human-acceptance.md`, using anonymous labels P1–P3.
