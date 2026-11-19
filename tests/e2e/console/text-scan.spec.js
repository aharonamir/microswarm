import { test, expect } from "@playwright/test";

const eventTypes = [
  "agent.exit", "approval.expired", "approval.requested", "approval.resolved", "delegation.denied", "delegation.dismiss", "delegation.dismissed", "delegation.reply", "delegation.request", "delegation.send", "delegation.spawned", "delegation.timeout", "message.inbound", "permit.decision", "scope.narrowed", "tool.call", "tool.result", "turn.end", "turn.start",
];
const patterns = [
  new RegExp(`\\b(?:${eventTypes.map(value => value.replaceAll(".", "\\.")).join("|")})\\b`),
  /\b\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)\b/,
  /\b[0-9A-HJKMNP-TV-Z]{26}\b/,
  /[\[\]{}]/,
];

async function publicText(page) {
  return page.locator("body").evaluate((body) => {
    const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
    const visible = [];
    while (walker.nextNode()) {
      const node = walker.currentNode;
      if (node.parentElement?.closest("details,[data-raw]")) continue;
      visible.push(node.textContent || "");
    }
    return visible.join(" ");
  });
}

test.fixme("red: T310 — current views still expose trace event metadata", async ({ page }) => {
  for (const route of ["#/overview", "#/sessions", "#/approvals", "#/trace/01ARZ3NDEKTSV4RRFFQ69G5FAV", "#/host", "#/members/01ARZ3NDEKTSV4RRFFQ69G5FAW"]) {
    await page.goto(`/${route}`);
    const text = await publicText(page);
    for (const pattern of patterns) expect(text, `${route} matches ${pattern}`).not.toMatch(pattern);
  }
});
