# Design reference: MicroSwarm Console v2

**Source:** the "MicroSwarm Console v2" design canvas (the owner's claude.ai artifact, private). Its board sources are copied in `boards/*.dc.html` for reference only. They use a canvas runtime and inline styles, so rebuild them in the console's HTML/CSS/JS and don't copy them.

**Precedence:** this file is the implementable record. Where it and a board disagree, this file wins. Where it and `spec.md` disagree, `spec.md` wins. On completion, this file replaces `specs/002-console/design/README.md`; 002's file then points here.

**Principles:**
- **Plain words first, data second.** "Reads `/work`, can't write", not `/work ro`.
- **Name the actor.** "The tester wants to run a command", not "approval required".
- **Red only means "needs you".**
- **Status is always a word plus a dot or icon, never colour alone.**

## Tokens (`web/styles.css` `:root`)

| Token | Value | Use |
|---|---|---|
| `--bg` | `#F7F5F2` | page background, nav |
| `--bg-chat` | `#FCFBF9` | conversation and decision panels |
| `--surface` | `#FFFFFF` | cards, panels, inputs |
| `--sunken` | `#F1EEE9` | lanes, inline code background, Raw blocks |
| `--hover` | `#EFEBE5` | nav/list hover, segmented control track |
| `--you` | `#EDE8E1` | operator bubbles |
| `--line` | `#E7E2DA` | card borders, dividers |
| `--line-soft` | `#F1EEE9` | row dividers inside cards |
| `--line-strong` | `#D9D2C7` | buttons, inputs |
| `--ink` | `#1C1917` | text, lead avatar, code blocks |
| `--ink-2` | `#57534E` | secondary text |
| `--ink-3` | `#6F6862` | labels, times (≥ 4.6:1 on `--bg`) |
| `--seal` | `#B3312A` | **needs you only**: dock border, waiting dots, badges |
| `--seal-ink` | `#8E241E` | attention text |
| `--seal-soft` | `#FBEAE7` | attention pill background |
| `--seal-wash` | `#FFF8F6` | waiting member row background |
| `--teal` | `#1D6A66` | primary actions, links |
| `--teal-ink` | `#134A47` | ok text |
| `--teal-soft` | `#E3F0EE` | working pill, binding note |
| `--brand-dot` | `#5FB3AA` | brand mark dots only |
| `--amber-ink` | `#7A4E0B` | reduced, check |
| `--amber-soft` | `#FBF0DC` | reduced pill |
| `--neutral-soft` | `#EFECE7` | done/allowed pill |

**Agent colours** (avatar background / text / lane bar), `--agent-0` to `--agent-6`:

| # | Use | bg | text | bar |
|---|---|---|---|---|
| 0 | lead | `#1C1917` | `#FFFFFF` | `#1C1917` |
| 1 | member | `#E6E8FB` | `#3440A8` | `#6872DB` |
| 2 | member | `#F1E6F7` | `#6B3787` | `#9A62B8` |
| 3 | member | `#E4F1E2` | `#2F6B2D` | `#5E9A5B` |
| 4 | member | `#E1EEF5` | `#1F5F7F` | `#4A8DB0` |
| 5 | member | `#F5E9E0` | `#80462A` | `#B87655` |
| 6 | member | `#ECEAE6` | `#4A4540` | `#8A837B` |

Avatar text on its background must be ≥ 4.5:1 (T303 checks it). No member colour is red, teal or amber, so they never read as status.

- **Waiting-for-you stripe:** `repeating-linear-gradient(135deg, #F3C6BF 0 5px, #FBEAE7 5px 10px)`.
- **Type:** IBM Plex Sans 400/500/600 (vendored); IBM Plex Mono 400/500 for paths, commands and codes. Body 14/20; chat text 14/20; page titles 18–20/24–26 600; Home headline 24/30 600; decision title 22/28 600; labels 11 px, 600, uppercase, 0.06em tracking.
- **Radii:** 8 avatars/small buttons; 10 buttons; 12 tiles/code blocks; 14 cards; 16 composer and user bubbles (16 16 4 16); 999 pills.
- **Shadows:** cards none; composer `0 2px 8px rgba(28,25,23,.05)`; dock `0 6px 20px rgba(142,36,30,.08)`; drawer `-12px 0 32px rgba(28,25,23,.06)`.
- **Targets:** ≥ 40 px desktop, ≥ 44 px for primary actions and icon buttons, 48 px decision buttons, 52 px on phone.

## Brand

- **Swarm mark:** a 30 px rounded square (r 8) in `--ink`, containing a 22 px SVG: four lines from the centre (11,11) to (4.5,5.5), (17.5,7), (6,17.5) and (17,16.5), stroke `--brand-dot` 1.2; a centre circle r 3.2 in white; circles r 2, 2, 2 and 1.6 in `--brand-dot` at the four ends.
- The wordmark "MicroSwarm" is 15 px 600.
- The favicon is the same mark at 32 px, as an SVG asset.
- The mark is `aria-hidden`; the wordmark carries the name.

## Shell (FR-U01)

**Left nav, 240 px:**
1. brand;
2. **New session** (teal, full width, 44 px);
3. Home · Approvals (seal count pill) · Trajectories · Host;
4. "Sessions": rows with the title (13.5 px) and a status line (dot + words in tone colour; the current row is a white card with a border);
5. "Ended today": muted rows;
6. at the bottom, "Supervisor running" (teal dot) and the `EN · 中文` toggle (`aria-pressed`).

**Icon rail, 64 px:** the same items as 44 px icon buttons with `aria-label`. Approvals shows a seal dot.

## Session (board: `Main.dc.html`)

**Layout:** nav | conversation (`flex: 999 1 560px`) | Team panel (`flex: 1 1 300px`, ≥ 1100 px only).

**Header (white, bottom border):**
- title h1 (18/24, 600);
- helper "Web chat · started 14:02 · you talk to the lead; it brings in members";
- status pill;
- agent avatar stack (overlapping by 6 px, 2 px white ring);
- **Trajectory** and **End session** buttons.

**Conversation (max-width 820, centred, gap 24):**
- **You:** right-aligned, max 78%; sender line "You · 14:02" above; `--you` bubble.
- **Lead:** 28 px lead avatar, sender line "**Lead** · 14:02" (+ " · using the researcher's report"), then text with no bubble.

**Team card:** indented 40 px to align with lead text.
- **Header:** branch icon, "Team · N members", "running 7 min", and an "Open trajectory" link, on `#FBFAF8`.
- **Member row:** avatar; role (link to the drawer); status pill; "9 steps · 3 min 40 s"; the task as a sentence; an expand button (`aria-expanded`).
- **Waiting row:** `--seal-wash` background plus a seal-ink line "Wants to run `cargo test -p auth`. See the request below."
- **Expanded steps:** indented 40 px, with a 2 px left rule in the agent's bg colour. Grid columns: icon 20 / sentence / duration (mono) / decision pill (20 px tall). A report step is in the agent's text colour, 500 weight.

**Findings inside lead text:** when a lead reply contains a list, render it as written (plain text). The finding cards on the board are illustrative only. Don't parse lead text.

**Dock (sticky bottom, gradient fade):**
- **Approval card:** 1.5 px seal border, r 14.
  - Row 1: avatar, "The tester wants to run a command", code + "expires in 11 min" in mono.
  - Then the dark command block with `$ ` prefix, and reach facts in one wrapping row (container · folders · network · parser).
  - Buttons: **Approve this command** (teal, 44) · **Deny** (seal-ink text, `#E2B7B1` border) · "Why it asks" link.
- **Composer:** the visible label "Message the lead"; a borderless textarea with 2 rows; the helper "Only the lead reads this. It passes instructions on to members."; Send (teal) with an arrow icon. In-flight: Send disabled, helper "The lead is still answering your last message."

**Team panel (white):**
- "Team" + "each in its own container";
- the lane strip (avatar 22 + 10 px lane per agent; times above; "Striped: waiting for you." below);
- agent cards (avatar, role, status pill, reach lines with folder/globe/key icons, amber "1 request reduced");
- a closing hint.

## Member drawer (board: `Member.dc.html`)

- The nav collapses to the icon rail. The conversation dims its history to 55% opacity and highlights the member's row in the Team card with a 3 px ring in its bg colour.
- **Drawer (`flex: 1 1 560px`):**
  - header with a 36 px avatar, role h2, "Member of this session · from team skill `x@1.2.0`", status pill and Close (44, `aria-label`);
  - the task box on `--bg`;
  - tabs (`role=tablist`, underline on the active tab): **Steps · N**, **What it can reach**, **Report**.
- **Reach tab:**
  - the summary sentence (15/22, key words 600);
  - "Can reach" rows (green check / amber shield for "asks you");
  - "Asked for, but reduced" rows with reason pills (amber for lowered or capped, seal-soft for never allowed);
  - "How its access was worked out": 4 columns (`--bg` tiles; the last one `--teal-soft`), chips mono 12 px; struck chips use seal-ink with line-through, lowered chips amber;
  - the explanation line "A member gets only what appears in all three…";
  - "The container around it": 3 cards (Files, Process, Network);
  - footer: "Ask the lead to dismiss it", "See its steps in the trajectory", and the short id + scope hash in mono 11.5.
- **Under 1100 px:** the drawer is full screen, with a Close button that returns focus.

## Trajectory (board: `Trajectory.dc.html`)

**Layout:** icon rail | main | inspector (`flex: 1 1 360px`, white).

**Header:**
- a breadcrumb link to the session;
- h1 "Trajectory";
- helper "Every step every agent took, on one clock. Pick a step to see why it was allowed.";
- a segmented control (Everything / Decisions / Problems, `aria-pressed`);
- Export JSON.

**Tiles:** auto-fit, min 130 px: Running for, Agents, Steps, Waiting for you (seal border and text), Access reduced (amber), Denied or failed.

**Lanes card** (horizontal scroll inside the card below 640 px):
- grid 110 px label / lane;
- time ticks (mono 11.5, at 0/25/50/75%, plus "now");
- lanes 28 px tall on `--bg`, r 7;
- spans 20 px, r 5, white 11.5 px label when ≥ 60 px wide;
- tool ticks are 2 px, white at 70%;
- marks are 14 px diamonds (message `--ink-2`, approval `--seal`, report in the agent's text colour);
- the selected span has a 2 px ink outline;
- a legend below.

**Steps card:**
- phase headings (14 px 600 + time range in mono);
- rows in a grid of time 52 / avatar 22 / sentence / decision / duration 56;
- the selected row has `--seal-wash` (if waiting) or `--sunken`, with a 1.5 px ink outline.

**Inspector:**
- avatar + h2 step sentence + "14:06:12 · step 18 of 31";
- the decision pill;
- the explanation box (`--bg`, label "Why it stopped" / "Why it was allowed" / …);
- a facts `dl` (110 px labels);
- actions (Review the approval, Open <role>);
- `<details>` "Raw events (N)" holding a dark mono block.

## Approvals (board: `Approvals.dc.html`)

**Layout:** icon rail | queue (`flex: 1 1 300px`) | decision.

**Queue:**
- h1, then tabs Waiting · N / Decided / Expired · N;
- the selected waiting item is a white card with a 1.5 px seal border: avatar, "tester · run a command", the command in mono, "<session> · 11 min left";
- "Earlier today" items show outcome words;
- expired items are dashed and muted: "Expired when the supervisor restarted. Asked again as `P3VN`, approved."

**Decision:**
- header with a 40 px avatar, h2 22/28 sentence, "In <session link> · asked 14:06 · if you don't answer by 14:17 it counts as denied", and the code chip (seal-soft);
- the command block in 15 px;
- 3 tiles: What runs, Where it runs, Why it asks (with "See the policy");
- the binding note on `--teal-soft` with a lock icon;
- "What the tester was asked to do" as a blockquote;
- `<details>` "Exact call" (server, tool, args, binding);
- a sticky footer with "Your answer goes to the supervisor, not to the agent." and Deny / Approve (48 px).

**Spawn variant:** the title is "The lead wants to add a <role>"; the tiles are What it will do (task), What it would reach (`spawn_preview` summary) and Why it asks; a "Preview its access" disclosure shows the 4-column derivation.

## Home (board: `Home.dc.html`)

- Main content is max 1040 wide.
- **Headline:** "1 thing needs you. 2 sessions are running." (24/30), with "5 containers up · 3 approvals today · last restart 2 days ago" under it.
- **Start a session card:** label, a borderless textarea "What should the lead work on?", chips (folder `~/code/api` as `/work`, Team), and Start.
- **"Needs you":** cards with a 1.5 px seal border: avatar, "The tester wants to run `…`", "In <session> · in its own container, read-only, no network · expires in 11 min", Deny / Approve.
- **Running and Ended today:** tables in cards, horizontally scrollable below 860 px. Columns: Session (title + channel), Status, Team (avatar stack), Latest (one line, ellipsis), For/ended (mono). A row is a single link.

## Host (board: `Host.dc.html`)

- **Layout:** icon rail | section nav (Health, Channels, Team skills, Tools, Credentials, Limits) | content (max 920).
- **Health:** check tiles (status pill + name + detail) and "Run checks".
- **Channels:** rows of name / detail / On-Off pill.
- **Team skills:** a card per skill, with policy pills in words.
- **Credentials:** rows of name (mono) / "Used by …" / Set value (002 dialog). The heading note reads "Values can be set here, never shown. Agents see names only."
- **Limits:** two cards, "Per session" and "Never grantable", with "Edit on the host: `~/.microswarm/config.toml`".

## Phone (board: `Phone.dc.html`)

- **Session:** a header with Back (44), the title, Trajectory (44), and a horizontal strip of agent status pills (avatar 24 + words).
- The conversation is as on desktop; the Team card is compact (role + state per line).
- **Docked:** a "tester wants to run `…`" bar with **Review** (52 px min height), then the composer input with a 44 px Send.
- **Approval:** a bottom sheet over a dimmed scrim, with a handle, the title, the command block, 4 fact lines, "Covers exactly this command, once.", and a 2-column grid of Deny / Approve at 52 px.

## Motion

- Expanding steps animates height (150 ms ease-out). A new step fades in (120 ms). The dock slides in (160 ms).
- Lane spans grow in width with live data and never jump back.
- All motion is disabled under `prefers-reduced-motion`.

## Copy rules

As in 002, plus:
- Never show `tool.call`, `permit.decision` or any event type outside Raw.
- Use "steps", not "events", in the UI.
- Use "reduced" for narrowed access, with the reason in words.
- "Waiting for you" is the only label for operator-blocked state.
