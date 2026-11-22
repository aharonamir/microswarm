# Checkpoint D — console redesign

Checkpoint C was approved by the owner on 2026-11-21. This report covers T340–T349. T350, the three-participant usability study, remains owner-run and open.

## Before and after

Captured through Playwright MCP from the native console and a disposable `console_status_fixture --scenario demo` home. Session/drawer captures use 1440, 768, 390 and 320 pixel widths. The fixture's approval expiry is deliberately fixed in the future. Baseline captures are the T302 artifacts, not reconstructed mockups.

| Screen | T302 before | Current after |
| --- | --- | --- |
| Desktop session | [1440](../005-baseline/session-1440.png) | [1440](session-1440.png) |
| Tablet session | No T302 capture | [768](session-768.png) |
| Phone session | [390](../005-baseline/session-390.png) | [390](session-390.png), [320](session-320.png) |
| Member drawer | — | [1440](drawer-1440.png), [768](drawer-768.png), [390](drawer-390.png), [320](drawer-320.png) |
| Phone approval sheet | — | [390](sheet-390.png), [320](sheet-320.png) |
| Home | — | [1440](home-1440.png), [390](home-390.png), [Chinese 390](home-zh-390.png) |
| Host | — | [1440](host-1440.png), [390](host-390.png) |

Approval and trajectory screenshots are also available in [Checkpoint C](../005-checkpoint-c/README.md).

## Automated acceptance

| Criterion | Evidence |
| --- | --- |
| SC-U04 — human text only | Real demo and hostile fixtures, every view including approval detail and member drawer, English and Chinese. Scanner excludes only explicitly marked Raw disclosures; checks event identifiers, ISO timestamps, identifiers longer than eight consecutive ULID characters and JSON delimiters. |
| SC-U05 — dock remains visible | Four-width scrolling regression; phone Review bar lives in the layout above the composer. |
| SC-U06 — accessibility and overflow | Axe and page overflow checks on every populated page at 1440/768/390/320; sheet axe checks on both phone widths; keyboard tabs, segments, rows, dialogs and focus return; reduced motion. |
| SC-U07 — 2,000 events | Actual load fixture with six members, real GET requests and unmocked responses. Session measured through a successful composer edit with Send enabled; trajectory through visible rows/inspector and a focusable filter. Latest local full run: session **840 ms** (<1,000), trajectory **841 ms** (<1,500). |
| SC-U08 — 002 security preserved | Cookie and Host checks for all new GET views; bad ids rejected; POST unavailable; hostile Origin rejected; actual hostile approval/roles/tasks/paths rendered as text; exact unchanged CSP and zero violation/alert reports on every page. Existing 002 browser and Rust checks retained. |

The test harness waits for the authenticated Home to finish rendering before closing its login page, so abandoned login-page requests cannot overlap the measured session load. Each timed navigation starts a fresh document. The session/trajectory GET requests use the production derivation code; only the fixture's event stream is controlled.

## Validation status

- Workspace format, Clippy, binary build and all Rust tests passed before the final review documentation edits.
- Complete Playwright suite: **99 passed, 0 skipped**. It includes 2,000-event performance, real demo/hostile text scanning in both locales, four-width axe/overflow, keyboard and read-only security coverage.
- Playwright MCP visual review: Home, session, member drawer and Host captured at desktop/phone widths; session and drawer also captured at tablet and 320 px. Clicking outside the open mobile navigation dismisses it. No browser errors.
- CI status will be added after the feature branch push completes.

## Remaining owner checks

SC-U01–U03 require three first-time participants and cannot be inferred from automated tests. Record T350 results under “005” in [console-human-acceptance.md](../console-human-acceptance.md). [Language review notes](language-review.md) distinguish automated translation/layout checks from the native-speaker review still pending.

No new dependency, CSP change, supervisor/agent/runtime change, event schema change or database write path was introduced. The authoritative scope continues to come from the supervisor database. New console routes remain GET-only.
