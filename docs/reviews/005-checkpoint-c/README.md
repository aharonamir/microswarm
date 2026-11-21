# Checkpoint C — approvals and trajectory

Checkpoint B was approved by the owner on 2026-11-21. This review covers T334–T339; Phase 6 has not started.

## Screenshots

Captured with Playwright MCP from the native console at 1440 px and 390 px. Desktop captures use a 1200 px viewport height so the selected row and inspector are visible together. Phone trajectory images capture the full document; the timeline scrolls horizontally within its card.

| Screen | Desktop | Phone |
| --- | --- | --- |
| Tool approval | [1440](approval-tool-1440.png) | [390](approval-tool-390.png) |
| Spawn approval, access preview open | [1440](approval-spawn-1440.png) | [390](approval-spawn-390.png) |
| Trajectory, waiting shell step selected | [1440](trajectory-waiting-1440.png) | [390](trajectory-waiting-390.png) |
| Trajectory, Problems filter and denied step | [1440](trajectory-problems-1440.png) | [390](trajectory-problems-390.png) |

The source is a fresh `console_status_fixture --scenario demo` home. A reviewer spawn approval was seeded into that disposable fixture database for the spawn variant. No production state was modified. The phone spawn capture is scrolled to the access preview; its decision footer remains visible. The fixture's tool approval expiry is intentionally fixed far in the future.

## Validation

- Complete browser suite: **79 passed, 1 skipped**. The skip is the existing T310 red scanner, which still covers pages scheduled for later phases.
- New approvals and trajectory coverage: tool and spawn variants; decided elsewhere; 52 px phone actions; lane/row/URL selection; server filters; phase headings; paging beyond 200; raw disclosures and JSON export.
- Populated trajectory axe and overflow checks pass at 1440/768/390/320, in addition to the existing console page checks.
- Rust regressions cover the tool approval brief, derived spawn Reach preview, complete-history step position/count, and correctly linked demo pending approval.
- Workspace formatting, Clippy, binary build, tests, and line budgets were checked before completing this checkpoint.
- Playwright MCP reported no browser errors during visual inspection.

## Implementation notes

The approval footer is a separate grid row; only the decision body scrolls. The inspector uses server sentences and decisions. All new strings have English and Chinese entries, and SVG/lane geometry uses the existing DOM helpers and CSSOM. No dependency was added.

The demo fixture now supplies its command in the actual approval payload format, emits the shell ask decision, and links its pending approval to the matching call. Spawn previews use the derived Reach contract instead of exposing the raw scope as a preview. Step details include complete-history `position` and `total` values for the inspector.

The chat request test now controls its event stream so earlier latency-test events cannot leak into it. The member tabs explicitly serialize ARIA booleans, including after a live refresh.
