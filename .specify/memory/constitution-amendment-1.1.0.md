# Constitution amendment 1.1.0 (MINOR): Principle IX

**Apply in task T206.** Insert the section below after Principle VIII in `.specify/memory/constitution.md`, add row IX to the Constitution Check table, and update the header line to `**Version:** 1.1.0 · **Ratified:** 2026-10-07 · **Last amended:** <date applied>`.

**Rationale:** `002-console` adds the first operator surface with a network listener. It needs explicit limits so that it can never become a second authority, and so that agent-originated content can never execute in the operator's browser.

---

### IX. Operator surfaces hold no authority of their own

Operator surfaces (the CLI, the console, and any future UI) are clients of the supervisor, never peers of it.

- They act only through supervisor socket operations, and read MicroSwarm state read-only. They never connect to the container-runtime socket, never write `supervisor.db` or session files, and never read credential values.
- Agent-originated content (replies, events, tool arguments) is untrusted data. It is rendered as text, never executed, never turned into links or markup, and never used to authorize anything.
- A network-facing surface listens on loopback only, requires an authenticated session for every request, checks `Host` and `Origin`, and serves a strict Content-Security-Policy.
- Each surface has its own line budget: `ms-console` ≤ 4,000 Rust lines, and its web assets ≤ 3,500 lines of JS + CSS, with no npm runtime dependencies and no build step. Exceeding it requires an amendment.

**Constitution Check row:**

| IX | Operator surfaces | Does the surface act only through supervisor ops, read state read-only, render agent content as text, and stay on loopback with auth, Host/Origin checks and CSP? |
