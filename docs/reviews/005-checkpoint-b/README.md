# 005 Checkpoint B

Captured from the rebuilt demo fixture with Playwright MCP at 2026-11-19. The baseline column points to T302; that baseline was captured at 1440px and 390px.

| Width | T302 baseline | Session | Session with tester drawer |
| ---: | --- | --- | --- |
| 1440 | [baseline](../005-baseline/session-1440.png) | [session](session-1440.png) | [tester drawer](session-drawer-1440.png) |
| 768 | — | [session](session-768.png) | [tester drawer](session-drawer-768.png) |
| 390 | [baseline](../005-baseline/session-390.png) | [phone session](session-390.png) | [tester drawer](session-drawer-390.png) |
| 320 | — | [phone session](session-320.png) | [tester drawer](session-drawer-320.png) |

Rendered geometry checks reported no horizontal page overflow at any width. The approval dock remained in the viewport. The desktop drawer occupied the right 562px at 1440px; below 1100px it filled the viewport.
