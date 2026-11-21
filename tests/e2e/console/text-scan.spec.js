import { test, expect } from "@playwright/test";
import {startConsole, expectView, ROOT, TESTER} from "./support.js";

const eventTypes = [
  "agent.exit", "approval.expired", "approval.requested", "approval.resolved", "delegation.denied", "delegation.dismiss", "delegation.dismissed", "delegation.reply", "delegation.request", "delegation.send", "delegation.spawned", "delegation.timeout", "message.inbound", "permit.decision", "scope.narrowed", "tool.call", "tool.result", "turn.end", "turn.start",
];
const patterns = [
  new RegExp(`\\b(?:${eventTypes.map(value => value.replaceAll(".", "\\.")).join("|")})\\b`),
  /\b\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)\b/,
  /\b[0-9A-HJKMNP-TV-Z]{9,26}\b/,
  /[\[\]{}]/,
];

async function publicText(page) {
  return page.locator("body").evaluate((body) => {
    const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
    const visible = [];
    while (walker.nextNode()) {
      const node = walker.currentNode;
      if (node.parentElement?.closest("[data-raw]")) continue;
      visible.push(node.textContent || "");
    }
    return visible.join(" ");
  });
}

for (const scenario of ['demo','hostile']) test(`${scenario} views keep technical metadata inside Raw in en and zh`, async ({browser}) => {
  const harness = await startConsole(browser, {scenario,mock:false});
  try {
    for (const locale of ['en','zh']) {
      const page = await harness.context.newPage();
      await page.addInitScript(lang=>localStorage.setItem('ms-lang',lang),locale);
      for (const route of ['home','sessions',`sessions/${ROOT}`,'approvals','approvals/K7QF',`sessions/${ROOT}/trajectory`,`sessions/${ROOT}/members/${TESTER}`,'host']) {
        await page.goto(`${harness.baseUrl}/?view=${encodeURIComponent(route)}#/${route}`);
        await expectView(page,route);
        const text = await publicText(page);
        for (const pattern of patterns) expect(text, `${scenario} ${locale} ${route} matches ${pattern}`).not.toMatch(pattern);
      }
      await page.close();
    }
  } finally { await harness.close(); }
});
