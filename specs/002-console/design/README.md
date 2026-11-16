# Design reference: MicroSwarm Console

Source: the "MicroSwarm Console" design canvas (owner's claude.ai artifact; private). This file is the implementable record of it. Where it and the canvas disagree, this file wins. Where it and `spec.md` disagree, `spec.md` wins.

The principle behind every screen: **the walls are the interface.** Every agent is drawn as a container with a visible wall (a 3 px double border). What it can reach is written on it in plain words, and the supervisor is the one gate between them.

## Tokens (`web/styles.css` `:root`)

| Token | Value | Use |
|---|---|---|
| `--ground` | `#F6F2EB` | page background |
| `--panel` | `#FFFDF9` | cards, header, composer |
| `--ink` | `#1E1916` | text, primary buttons, container walls |
| `--ink-2` | `#5E524A` | secondary text (7:1 on ground) |
| `--ink-3` | `#6E6056` | labels, timestamps (≥ 4.5:1 on ground and panel) |
| `--rule` | `#DDD2C4` | dividers |
| `--rule-2` | `#CDBFAF` | input borders, tree lines |
| `--tint` | `#EFE7DB` | team markers, active nav |
| `--seal` | `#B3312A` | brand mark; **attention only** (waiting for you, approvals) |
| `--seal-ink` | `#8E241E` | attention text on light |
| `--seal-soft` | `#F6E1DC` | attention banner background |
| `--teal` | `#1D6A66` | primary action (Send, New session), "working/ready" |
| `--teal-ink` | `#134A47` | ok text |
| `--teal-soft` | `#DCEDEA` | ok chip background |
| `--amber-ink` | `#6B4508` | narrowed, policy tightened, warn |
| `--amber-soft` | `#F5E8CF` | narrowed chip background |
| `--wall` | `3px double var(--ink)` | container border; `var(--seal)` when that agent is waiting for you |

- **Type:** IBM Plex Sans 400/500/600 for UI, 14/20 base. IBM Plex Mono 400/500 for ids, paths, commands and events, 12–13 px. CJK: the system stack (research R-C4).
- **Sizes:** page title 20/26 600, section title 15 600, labels 11 px uppercase with 0.08em tracking and 600 weight.
- **Radii:** 6 (containers), 8 (cards, buttons), 12 (chat bubbles).
- **Minimum targets:** 44 px, or 52 px on the phone layout.
- **Meaning:** red is reserved for "needs you". Status always pairs a word with a dot, never colour alone.

## Global frame

- **Header (all pages):** the 微群 mark (32 px seal square), "MicroSwarm", then the nav: Sessions · Approvals (with a red count badge) · Trace · Host. On the right: the "supervisor running" chip and the EN | 中文 toggle (`aria-pressed`).
- **Layout:** a flex row that wraps. The left rail is `flex: 1 1 230px`, main is `flex: 999 1 560px; min-width: 0`, and the right panel is `flex: 1 1 340px`. Under 900 px the panels stack: rail, main, right panel.

## Sessions page (revised 2026-10-30, the primary screen)

**Left rail**
1. **New session** button at the top: teal, full width, plus icon.
2. "Your sessions": cards with title (600), a status line (dot + words, coloured by tone) and a detail line ("lead + 2 members · telegram"). The current card has a 1.5 px ink border and `aria-current`.
3. "Ended today": compact rows with the end reason in words; they link to Trace.

**Main column (conversation with the lead)**
1. **Header:** session title (h1), then the helper line "Chat with the lead agent. It brings in team members when it needs them." On the right: a "Full history" link (to Trace) and a small **End session** button that asks for confirmation.
2. **Status banner** (`role="status"`, `aria-live="polite"`) directly under the header. The words come from StatusWords, plus a one-sentence detail and an action link ("Review it ↓" jumps to the inline approval). Tone colours: attention = seal-soft with a seal-ink title; active and ok use teal-soft.
3. **Messages:**
   - Each has a sender line, "You · 14:04" or "Lead · 14:05", or "Lead · 14:07 · from the researcher" for relayed member findings.
   - "You" bubbles are ink, right-aligned. "Lead" bubbles are panel with a rule border, left-aligned. Bubbles are at most 72–78 % wide.
   - **Team markers** are tint rows labelled "TEAM", for example "researcher joined · tester joined after you approved it", with a link "What can they reach?" to the member page.
   - **Inline approval card:** 2 px seal border. Title "Your approval: tester wants to run", the code and expiry in mono, the command in a dark mono block, then one sentence on where it runs. Buttons: Approve (ink), Deny (seal outline), and a "More details" link.
4. **Composer** (separated by a 2 px ink top border):
   - a visible label "Message the lead";
   - a 2-row textarea with the placeholder "Ask a question, give new instructions, or tell it to stop a member…";
   - a teal Send button;
   - a helper line under it: on web sessions "Replies appear here."; on other channels "This session started on telegram @amir. Replies to messages you send here appear only here."
   - Disabled state: "The lead is still answering your last message."

**Right panel ("Who's working")**
1. Subtitle: "Each agent runs in its own container. Open one to see exactly what it can reach."
2. The lead card (wall, "Lead", status, "You talk to this one. …").
3. Members, indented with a 2 px rule tree line: wall cards with role, status and `reach_summary`. A member waiting for you gets a seal wall. Ended members show as a single muted row with their end reason.
4. "Latest activity": the last 4–6 events as short sentences with a mono time, plus an "All events" link.

## Approvals page

- **Left: the queue.** Tabs: Waiting · N / Decided / Expired · N.
  - Pending items have a seal border, a title ("tester · run a command"), the code in seal mono, the command or role in mono, and "<session> · expires in N min".
  - Expired items are dashed and muted: "Expired when the supervisor restarted at 14:02. Sent again as K7QF."
- **Right: the detail.**
  - h1 "tester wants to run a command", then the code and "expires 14:17 · then it counts as denied".
  - A `dl` grid: Asked by, Member of, Why it asks, Shown on.
  - **Exact call:** a dark mono block with `server`, `tool`, `args` and `binding sha256:…`, and the sentence "The approval is bound to this server, tool and argument hash. If the tester asks for anything different, it needs a new approval."
  - **How the shell parser read it:** chips such as "simple command", "program: cargo", "no pipes, redirects or substitutions" (R-C6).
  - **Where it runs:** a wall card with mount, credential-name and network chips, and a link to the member page.
  - Buttons: "Approve this call" and "Deny", both 48 px.
  - Under them, the next item in the queue as a 3-column scope preview for spawn requests.

## Member page (scope)

- **Header:** role, ids, status chip, "from code-review-team@1.2.0 #researcher" and the scope hash.
- **Grid:** column headers Requested · ∩ Lead holds · ∩ Never grantable · = Granted. The last column has a 2 px teal border.
- **Rows:** Mounts, Credentials, Tools, Limits, Delegation.
  - Narrowed items are struck through in seal-ink, with the reason in words ("not in lead scope").
  - Tightened policy is shown in amber ("tightened to the lead's policy").
- **"The container around it":** a wall card with three columns, Filesystem, Process and Network, filled from ContainerProfile.
- **Actions:** "Ask the lead to dismiss it" pre-fills the composer with "Please dismiss the <role>." and navigates to the session. Also a link to "See its events in the trace".

## Trace page

- **Header:** title, "root … · N members · N events", and filter buttons (All, Tools, Decisions, Delegation, Exits) with `aria-pressed`, plus Export JSON.
- **Body:** a lead wall section, then the members indented with a tree line.
- **Rows:** a 4-column grid: time, description (mono), decision, duration. Decision colours: allow is teal-ink; ask and narrowed are amber-ink; denied and failed are seal-ink. A narrowed row has an amber-soft background.
- **Right panel:** the selected event's JSON in a dark block, the paired result sentence, and a Copy button.

## Host page

Cards in an auto-fit grid (min 380 px):
- **Doctor:** pass/warn/fail rows.
- **Channels:** cli, telegram, web (this console), webhook.
- **Tools:** MCP servers, with "Add a server" disabled and a "use `microswarm tools add`" hint.
- **Team skills:** roles with spawn-policy chips.
- **Credentials:** names, "used by", and a **Set a value** dialog with a password-type input. The field is cleared after submit, and the note says "Values can be set here but never shown."
- **Limits and ceiling:** read-only, with "edit on the host".

## Phone layout (390 px)

The same pages, stacked. The Approvals detail becomes a full screen with:
- the title;
- the dark command block;
- the "Runs in the tester's container" wall card;
- the binding sentence;
- a sticky footer with 52 px Approve and Deny buttons and a "Next request" link.

## Copy rules

- Plain words first and mono details second ("Reads /work", not "/work ro").
- Name the actor: "tester wants to run…", not "approval required".
- Never "AI", "magic" or emoji. Errors say what happened and what to do next.
- Every string lives in `i18n/en.json` and `i18n/zh.json` under stable keys. Agent text is never translated.
