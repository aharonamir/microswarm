# Feature Specification: Console UI redesign

**Feature branch:** `005-ui-redesign`
**Status:** Ready to implement
**Builds on:** [`002-console`](../002-console/spec.md) (server, security model, API) and the Phase 10 restyle (T276–T280, merged in `1d1db2a`)
**Design reference:** [design/README.md](./design/README.md), with board sources in `design/boards/`
**Work order:** [tasks.md](./tasks.md)

## Context

The 002 console works and is secure, but people don't understand it at first glance. A review of `main` at `1d1db2a`, rendered with a realistic two-member session, found:

- **Chat:** member work is invisible in the conversation. A grey "researcher joined · tester joined" marker is all you see. The inline approval card is half hidden behind the composer.
- **Members:** they sit in a separate column as double-bordered boxes, cut off from what they are doing. The activity list next to them repeats raw event names (`tool.call`, `tool.call`).
- **Trace:** a flat list of event types with ISO timestamps beside a JSON dump. It has no time axis, no grouping and no explanation of decisions.
- **Member scope:** raw JSON arrays (`[ "/work rw" ]`) in grey boxes. Disclosures look like empty boxes.
- **Overall:** ISO timestamps, heavy borders and a Chinese-character brand mark. The product reads as dated.

The redesign keeps the 002 server, its security model and its API, and adds read-only views derived from data the console already reads. The screens are reorganised around three ideas taken from current multi-agent tools (research R-U1):

1. **One conversation, team inline.** The operator talks only to the lead. When the lead starts members, a Team card appears in the conversation and shows each member's task, live state and steps.
2. **"Needs you" is never out of view.** A pending approval is pinned above the composer on the session, and at the top of Home.
3. **Every agent on one clock.** The trajectory shows each agent as a lane on a shared time axis, the steps as sentences grouped into phases, and an inspector that explains each decision in words.

## User scenarios & testing *(mandatory)*

### User Story 1 — Follow and steer a session from one conversation (Priority: P1) 🎯 MVP

The operator opens a session and sees the conversation with the lead. When the lead starts members, a Team card appears at that point in the conversation. It lists each member with role, task, status, step count and elapsed time. Expanding a member shows its steps as plain sentences, each with a decision label. A pending approval for this session is pinned above the composer with the command, where it runs, what it can reach, and Approve and Deny buttons.

**Independent test:** with a fixture session holding a lead, two members and one pending approval, a first-time operator can say what the session is waiting for, which member is waiting, and send the lead a message, without scrolling, in 10 seconds or less.

**Acceptance scenarios:**
1. **Given** a lead that started two members, **when** the session page loads, **then** a Team card shows both members at the position in the conversation where they were started, each with role, task (first 160 characters), status words, step count and elapsed time.
2. **Given** a member with steps, **when** the operator expands it, **then** its steps appear as sentences ("Read `refresh.rs`", "Searched for `expires_at` · 6 matches"), each with a decision label (allowed, asked you, denied, failed) and a duration. No raw event type is shown.
3. **Given** a pending approval for this root session, **when** the operator scrolls the conversation anywhere, **then** the approval stays pinned above the composer and remains in the viewport at 1440, 768, 390 and 320 px widths.
4. **Given** the operator approves in the pinned card, **when** the supervisor confirms, **then** the card leaves, the member's status changes, and the Team card row updates within 2 seconds without a page reload.
5. **Given** a lead message built from a member's report, **then** its sender line reads "Lead · 14:09 · using the researcher's report".
6. **Given** live events arrive while the operator is typing, **then** the draft, focus, caret and conversation scroll position are preserved (as in 002 T275/T278).

### User Story 2 — See who is working and what each agent can reach (Priority: P1)

A Team panel on the session page shows a compact lane strip covering the last part of the session, plus one card per agent with status, the current step and a plain-language reach summary ("`/work/src/auth` read only · No network · no credentials"). Opening a card shows a member drawer beside the conversation: the task from the lead, then tabs for Steps, What it can reach, and Report.

**Independent test:** an operator asked "which member is waiting, and can it write files?" answers correctly in 15 seconds or less.

**Acceptance scenarios:**
1. **Given** a member whose access was reduced, **then** its card shows "N requests reduced" in amber, and the drawer's reach tab lists each reduction with its reason in words (`not_in_lead_scope` → "the lead doesn't hold it"; `ceiling` → "never allowed on this host"; `mode_downgraded` → "lowered to read only"; `policy_tightened` → "tightened to the lead's policy"; `limit_capped` → "capped by the host limit").
2. **Given** the drawer's reach tab, **then** it shows a one-sentence summary, "Can reach", "Asked for, but reduced", the four-step derivation (lead asked → lead holds → host allows → granted), and the container profile (files, process, network) in words. Raw scope JSON is only inside a "Raw" disclosure.
3. **Given** the drawer is open, **then** the conversation stays visible and usable at widths ≥ 1100 px. Below that, the drawer covers the screen and has a Close button that returns focus to the card that opened it.
4. **Given** "Ask the lead to dismiss it", **then** the composer is filled with "Please dismiss the <role>." and focused (as in 002), with nothing sent.
5. The old route `#/members/<id>` redirects to `#/sessions/<root>/members/<id>`.

### User Story 3 — Read the trajectory (Priority: P2)

The trajectory page shows:
- **Summary tiles:** running time, agents, steps, time waiting for you, access reduced, denied or failed.
- **Lanes:** one per agent (and one for You) on a shared time axis. Spans show working and waiting-for-you, with tick marks for tool calls and diamonds for messages, reports and approvals.
- **Steps:** grouped into phases.
- **Inspector:** for the selected step, "Why it stopped" or "Why it was allowed" in words, then the facts, then the raw events in a disclosure.

**Independent test:** given a session where one step was denied, an operator finds the step and says why it was denied in 20 seconds or less.

**Acceptance scenarios:**
1. Selecting a lane span or a step row selects the same step in both and opens it in the inspector.
2. The filter Everything / Decisions / Problems shows: all steps; only `ask`/`deny` decisions, reductions and approvals; only denied, failed, timed out and safety exits.
3. Phases are derived (data-model §Phase): a new phase starts at each operator message and whenever the set of working members changes. Each phase has a generated title: "The lead plans", "N members work in parallel", "The lead checks and replies", "The lead works", "Waiting for you".
4. Sessions with more than 500 steps load the first 200 and page on scroll. The lane view draws all spans from a summary endpoint.
5. Export JSON and the raw events stay available (as in 002).

### User Story 4 — Decide an approval (Priority: P1)

The approvals page shows a queue (Waiting, Earlier today, Expired) and one decision at a time:
- the request as a sentence ("The tester wants to run a command");
- the code and expiry;
- the command in a large monospace block;
- three tiles: What runs (parser facts), Where it runs (container, folders, network, credentials) and Why it asks (the policy rule and where it is set);
- the binding note;
- the task the requester was given by the lead;
- the exact call in a disclosure;
- a sticky Deny / Approve bar.

**Acceptance scenarios:**
1. A spawn approval shows "The lead wants to add a <role>", the task, and the reach the member would get (requested ∩ lead ∩ ceiling) as a preview of the reach tab.
2. "Already decided on another channel", expiry and re-issue behave as in 002 (T267).
3. At 390 px the decision is a full screen with a sticky 52 px Approve / Deny footer.

### User Story 5 — Know what needs you from Home (Priority: P2)

Home replaces Overview and becomes the default route. It shows:
- a one-line summary ("1 thing needs you. 2 sessions are running.");
- a Start-a-session box;
- "Needs you" cards with inline Approve and Deny;
- a Running table (session, status, team avatars, latest line, elapsed);
- an Ended-today table with outcome words.

**Acceptance scenarios:**
1. With nothing pending, "Needs you" reads "Nothing needs you right now."
2. "Latest" shows the newest lead or member sentence, never raw event text, and is cut to one line.
3. Starting a session from Home behaves like 002's New session (in-flight lock, error text) and navigates to the new session.

### User Story 6 — Host, phone and brand (Priority: P3)

- **Host:** a settings layout with sections Health, Channels, Team skills, Tools, Credentials and Limits, using 002's existing data. Spawn and tool policies show as words ("asks you before starting").
- **Brand:** the 微 mark is replaced by the swarm mark (a centre dot with four linked dots) and the favicon matches it. The wordmark stays "MicroSwarm".
- **Phone:** session pages show an agent status strip under the title, a pinned "Review" bar for a pending approval, and an approval bottom sheet.

### Edge cases

- **Members:** a member that never produced a step shows "Starting…" until its first event, then "No steps yet". A member started then denied shows in the Team card as "Not started: you denied it".
- **Many members:** with more than 4, the Team card shows 4 and "+N more", and lanes scroll vertically inside their own box.
- **Unknown or malicious tools:** a tool missing from the sentence table renders as "Used `<server>/<tool>`". Arguments are truncated to 120 characters and rendered as text.
- **Hostile text:** strings from agents, tools, skills and titles are rendered as text only (Principle IX). The 002 hostile-string e2e cases must pass on every new view.
- **Supervisor restart:** an expired approval disappears from the dock, and Home shows "Asked again as `XXXX`" when it is reissued.
- **Clock:** times show as local `HH:MM`. Durations show as `0.4 s`, `3 min 40 s` or `2 h 5 min`. ISO timestamps appear only inside Raw disclosures and exports.
- **Ended sessions:** they are read-only. The composer is replaced by "This session ended at 14:30 (done). Start a new one", and Team card rows show final states.

## Requirements *(mandatory)*

### Functional requirements

**Shell and navigation**
- **FR-U01** A left navigation (≥ 1100 px: 240 px with labels; 700–1099 px: 64 px icon rail; < 700 px: top bar with a menu) holds New session, Home, Approvals (count badge), Trajectories, Host, the session list with status words, the supervisor status and the language toggle. When a member drawer is open, the navigation collapses to the icon rail.
- **FR-U02** Routes:
  - `#/home` (default, replaces `#/overview`, which redirects);
  - `#/sessions/<id>`;
  - `#/sessions/<id>/members/<mid>`;
  - `#/sessions/<id>/trajectory[/<step_id>]`;
  - `#/approvals[/<id>]`;
  - `#/host[#section]`.

  The old `#/trace/<id>` and `#/members/<id>` routes redirect. The navigation-focus behaviour added in `4f789f0` ("preserve console navigation focus") is kept: after a route change, focus goes where that commit puts it, and its e2e case is ported to the new shell.
- **FR-U03** The brand mark is the swarm mark (design/README §Brand). No CJK glyph is used as a logo.

**Session**
- **FR-U04** Lead messages render without a bubble, behind a role avatar. Operator messages render as a right-aligned tinted bubble. Sender lines use local `HH:MM`.
- **FR-U05** Each delegation group appears as one Team card, anchored after the lead message whose turn started it. Data comes from `GET /api/v1/sessions/{id}/team`.
- **FR-U06** Team card rows can expand to show that member's steps (`GET …/steps?agent=<mid>`), up to 20 at a time, with "Show all N steps" linking to the trajectory filtered to that member.
- **FR-U07** Pending approvals whose root is this session are pinned in a dock above the composer, oldest first, with "+N more" when there are more than one. The dock is part of the conversation panel's layout, not an overlay, so it never covers messages.
- **FR-U08** The Team panel shows a lane strip (last 15 minutes or the whole session if shorter) and agent cards (FR-U10). It is hidden below 1100 px, where the agent strip under the title replaces it.

**Members**
- **FR-U09** The member drawer has a header (avatar, role, status, source skill), "Its task, from the lead", and tabs Steps, What it can reach, and Report (the member's last report as plain text).
- **FR-U10** Reach summaries and the reach tab use `GET /api/v1/members/{id}/reach`. Words come from i18n keys. Paths, tool names and credential names render in monospace as data.

**Trajectory**
- **FR-U11** The trajectory uses `GET …/timeline` for tiles and lanes, and `GET …/steps` (paged) for the list. Selection is kept in the URL (`/trajectory/<step_id>`).
- **FR-U12** The inspector shows the decision explanation (data-model §Explanation), the facts list, links (Review the approval, Open <role>), and "Raw events (N)" in a `<details>`.

**Approvals and Home**
- **FR-U13** The approval detail adds `brief` (the requester's task), `policy` (rule text and source) and `facts` (parser facts as i18n keys) to the 002 response (contracts/console-http-additions.md).
- **FR-U14** Home uses `GET /api/v1/home`. Overview's endpoint stays for compatibility and is no longer linked.

**Text and data rules**
- **FR-U15** Step sentences are computed on the server as `{key, args}` and rendered on the client through i18n (contracts/step-sentences.md). The client never builds sentences from raw event types.
- **FR-U16** No view outside a Raw disclosure or an export may show an event type identifier, an ISO-8601 timestamp, a ULID longer than 8 characters, or JSON syntax. An automated check enforces this (T310, made green in T347).
- **FR-U17** Agent identity colours come from a fixed palette of 6, assigned by the order in which members started within a root session (the lead is always ink). Colour never carries meaning alone: every avatar shows the role's initial and the role name appears next to it.

**Live updates**
- **FR-U18** On SSE `agent` or `supervisor` events for the open root session, the client refetches the affected views (team, steps tail, timeline, approvals). Each view refetches at most once per second, and appends steps with `after_seq`.

**Security and compatibility**
- **FR-U19** Principle IX and 002 FR-C08–C12 still apply: loopback, cookie auth, Host and Origin checks, strict CSP, and DOM writes only through `js/dom.js`. Lane geometry is set through CSSOM properties (`el.style.left`), never `style` attributes or inline `<style>`. `dom.js` gains a `style` option that assigns properties one by one.
- **FR-U20** No new supervisor operations, no writes to session or supervisor databases, no new dependencies in the browser. The new endpoints are read-only and derive everything from `supervisor.db`, the session `events.db` files and `ms_runtime::trace`.
- **FR-U21** Every new string exists in `i18n/en.json` and `i18n/zh.json`. Agent text is never translated.

### Key entities

See [data-model.md](./data-model.md): TeamGroup, MemberRow, Step, StepSentence, Decision, Phase, Lane, Span, Mark, Reach, Reduction, Explanation, HomeView.

## Success criteria *(mandatory)*

- **SC-U01** 3 of 3 first-time operators complete US1's independent test (identify what the session waits for and send a message) in ≤ 10 s each.
- **SC-U02** 3 of 3 answer US2's question ("which member is waiting, and can it write files?") correctly in ≤ 15 s.
- **SC-U03** 3 of 3 find why a denied step was denied, from the trajectory, in ≤ 20 s.
- **SC-U04** The FR-U16 text scan passes on every page with the demo fixture, in both locales.
- **SC-U05** The approval dock stays in the viewport while scrolling, at 1440, 768, 390 and 320 px.
- **SC-U06** Axe reports no violations, and there is no horizontal page scroll at the four widths on every page.
- **SC-U07** With 2,000 events, the session page is interactive in < 1 s and the trajectory first page in < 1.5 s, on the e2e machine.
- **SC-U08** All 002 security e2e cases (hostile strings, CSP, Host/Origin, cookie) still pass, with new cases for every new endpoint.

## Assumptions

- The board files in `design/boards/` are visual references written for a design canvas. Implement them in the console's own HTML, CSS and JS; don't copy their inline styles or runtime.
- Step and phase derivation runs in `ms-console` on each request. With a per-session cache keyed by the last event `seq`, this is fast enough for SC-U07, so no new storage is needed.
- The 002 decisions on Q1–Q4 stand: no mirroring console replies to the origin channel; the busy rule is unchanged; fonts are vendored; `evaluate_shell` is used for parser facts.

## Out of scope

- Messaging members directly (members only talk to the lead).
- Editing policy, limits or credentials beyond 002's Set value.
- Dark mode (tokens are named so it can be added later).
- A node-graph view of agents.
- Changes to the supervisor, agent or event schema.
