You are starting feature 002-console in the MicroSwarm repo: a web console for operating the
swarm (chat with a session's lead, approvals, team status, member scope, trace, host).

Setup:
- The spec bundle is now in the repo root: specs/002-console/ and
  .specify/memory/constitution-amendment-1.1.0.md. Commit it on a new branch 002-console as
  "docs: add 002-console spec bundle".
- Read, in order: AGENTS.md, .specify/memory/constitution.md, the amendment,
  specs/002-console/{spec,plan,research,data-model}.md, contracts/, design/README.md, tasks.md.
  tasks.md is the work order. AGENTS.md rules apply unchanged: test first with red tests
  compiling and ignored, the four CI commands in order before every push, CI green after
  every push, small commits titled "<task ids>: <what>", a one-line justification for any
  new dependency.

Ground rules specific to 002:
- The console is a client of the supervisor, never an authority (amendment Principle IX).
  It acts only through supervisor socket ops, reads supervisor.db, session dbs and config
  read-only, never touches the runtime socket, and never reads or returns credential values.
- Agent-originated content is untrusted. In web/, build the DOM only through js/dom.js. No
  innerHTML, outerHTML, insertAdjacentHTML, document.write, eval or new Function;
  scripts/web-lint.sh enforces this.
- Loopback only, one-time startup token, then an HttpOnly SameSite=Strict cookie. Host
  allowlist, Origin check, X-MS-Console header on POSTs, strict CSP with no inline
  script or style.
- ms-supervisor has 7 lines of budget headroom. Do the T212 extraction before adding
  anything to it, and include before/after loc-budget.sh numbers in that commit.
- No npm runtime dependencies and no frontend build step. Assets are embedded in the binary.
- Default answers to Q1–Q4 in tasks.md stand unless I say otherwise. If research shows a
  default is wrong, stop and tell me.

Start with Phase 0 (T201–T205). Every ⚠ item in research.md needs a decision backed by
evidence (file paths, measured numbers, command output). Stop at Checkpoint A and report:
the decision for each R-C item, the measured event_stream cost (T203) and whether T222 is
needed, and the predicted supervisor budget after the T212 move. Do not start Phase 1
until I confirm.
