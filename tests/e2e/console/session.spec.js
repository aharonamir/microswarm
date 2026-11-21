import { test, expect } from "@playwright/test";
import { approvals, startConsole, ROOT } from "./support.js";

let harness;

test.beforeAll(async ({ browser }) => {
  harness = await startConsole(browser);
});

test.afterAll(async () => harness?.close());

test("the session conversation anchors its team card after the lead turn", async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await expect(page.getByRole("heading", { name: "Audit the auth module" })).toBeVisible();
  await expect(page.getByText(/using the researcher's report/)).toBeVisible();
  const lead = page.getByText("The refresh path validates the token before reuse.");
  const team = page.getByRole("region", { name: "Team card" });
  const anchoredAfterLead = await lead.evaluate(node => {
    const card = document.querySelector('[aria-label="Team card"]');
    return Boolean(card && (node.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING));
  });
  expect(anchoredAfterLead).toBe(true);
  await expect(team.getByText("researcher")).toBeVisible();
  await expect(team.getByRole("link", { name: "tester" })).toBeVisible();
  await page.close();
});

test("member steps are sentences with decisions and durations", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await page.getByRole("button", { name: /Show steps for researcher/ }).click();
  const row = page.locator(".step-row").filter({ hasText: "Read refresh.rs" });
  await expect(row).toContainText("Allowed");
  await expect(row).toContainText("0.2 s");
  await expect(page.locator("body")).not.toContainText("tool.result");
  await page.close();
});

test("the approval dock stays visible at all four widths", async () => {
  const page = await harness.context.newPage();
  for (const width of [1440, 768, 390, 320]) {
    await page.setViewportSize({ width, height: width < 500 ? 844 : 900 });
    await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
    const dock = page.locator("[data-approval-dock]");
    await expect(dock).toBeVisible();
    await expect(dock.locator(".dock-header strong")).toHaveText("tester asked you to approve: tool");
    const box = await dock.boundingBox();
    expect(box.y + box.height).toBeLessThanOrEqual(width < 500 ? 844 : 900);
    await page.locator(".conversation-scroll").evaluate(node => { node.scrollTop = node.scrollHeight; });
    await expect(dock).toBeInViewport();
  }
  await page.close();
});

test("approving updates through the existing decision route", async () => {
  const page = await harness.context.newPage();
  let decisionRequest;
  await page.route("**/api/v1/approvals/K7QF/approve", async route => {
    decisionRequest = route.request();
    return route.fulfill({ status: 200, json: { decision: "approved" } });
  });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await page.getByRole("button", { name: "Approve this command" }).click();
  await expect.poll(() => decisionRequest?.url()).toContain("/api/v1/approvals/K7QF/approve");
  expect(decisionRequest.postDataJSON()).toEqual({});
  await page.close();
});

test("denying uses the existing decision route", async () => {
  const page = await harness.context.newPage();
  let decisionRequest;
  await page.route("**/api/v1/approvals/K7QF/deny", async route => {
    decisionRequest = route.request();
    return route.fulfill({ status: 200, json: { decision: "denied" } });
  });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await page.getByRole("button", { name: "Deny" }).click();
  await expect.poll(() => decisionRequest?.url()).toContain("/api/v1/approvals/K7QF/deny");
  expect(decisionRequest.postDataJSON()).toEqual({});
  await page.close();
});

test("the dock reports approvals decided on another channel", async () => {
  const page = await harness.context.newPage();
  await page.route("**/api/v1/approvals/K7QF/approve", route => route.fulfill({
    status: 409, json: { error: "already_resolved", message: "Already resolved" },
  }));
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await page.getByRole("button", { name: "Approve this command" }).click();
  await expect(page.locator(".dock-error")).toHaveText("Already decided on another channel");
  await page.close();
});

test("the dock summarizes additional pending approvals", async () => {
  const page = await harness.context.newPage();
  await page.route("**/api/v1/approvals**", async route => {
    const url = new URL(route.request().url());
    if (url.pathname === "/api/v1/approvals" && url.searchParams.get("state") === "pending") {
      const extra = { ...approvals[0], id: "01ARZ3NDEKTSV4RRFFQ69G5FB3", code: "K7QG", created_at: "2026-11-19T14:10:00Z" };
      return route.fulfill({ status: 200, json: { approvals: [extra, approvals[0]] } });
    }
    return route.continue();
  });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await expect(page.locator(".dock-code")).toHaveText("K7QF");
  await expect(page.locator(".dock-header")).toContainText("+1 more");
  await page.close();
});

test("live updates preserve the draft, caret, focus, and conversation scroll", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  const composer = page.getByLabel("Message the lead");
  await composer.fill("Keep investigating the refresh path");
  await composer.evaluate(node => node.setSelectionRange(6, 6));
  await composer.focus();
  await page.locator(".conversation-scroll").evaluate(node => { node.scrollTop = 84; });
  await page.evaluate(rootId => window.__consoleEvents.emit("agent", { session_id: rootId, root_session_id: rootId }), ROOT);
  await expect(composer).toHaveValue("Keep investigating the refresh path");
  await expect(composer).toBeFocused();
  expect(await composer.evaluate(node => node.selectionStart)).toBe(6);
  await expect.poll(() => page.locator(".conversation-scroll").evaluate(node => node.scrollTop)).toBe(84);
  await page.close();
});

test("ended sessions replace the composer with a read-only notice", async () => {
  const page = await harness.context.newPage();
  await page.route("**/api/v1/sessions/01ARZ3NDEKTSV4RRFFQ69G5FAV", route => route.fulfill({
    status: 200,
    json: { session: { id: ROOT, root_id: ROOT, title: "Audit the auth module", status_words: { key: "status.ended", tone: "neutral", args: {} }, ended_at: "2026-11-19T14:30:00Z", end_reason: "done" }, team: [], banner: {} },
  }));
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  await expect(page.getByText(/This session ended at/)).toBeVisible();
  await expect(page.getByLabel("Message the lead")).toHaveCount(0);
  await page.close();
});

test("the session team panel shows CSSOM positioned lanes and collapses to the agent strip", async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  const panel = page.getByRole("complementary", { name: "Team" });
  await expect(panel).toBeVisible();
  await expect(panel.locator(".lane-row")).toHaveCount(3);
  const geometry = await panel.locator(".lane-span").first().evaluate(node => ({ left: node.style.left, width: node.style.width }));
  expect(geometry.left).toMatch(/^\d+(\.\d+)?%$/);
  expect(geometry.width).toMatch(/^\d+(\.\d+)?%$/);
  await page.setViewportSize({ width: 768, height: 900 });
  await expect(panel).toBeHidden();
  await expect(page.locator(".session-agent-strip")).toBeVisible();
  await page.close();
});

test("lane role labels start after the agent avatars", async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  const panel = page.getByRole("complementary", { name: "Team" });
  await expect(panel.locator(".lane-row")).toHaveCount(3);
  const gaps = await panel.locator(".lane-row").evaluateAll(rows => rows.map(row => {
    const avatar = row.querySelector(".lane-avatar").getBoundingClientRect();
    const role = row.querySelector(".lane-role").getBoundingClientRect();
    return { role: row.querySelector(".lane-role").textContent, gap: role.left - avatar.right };
  }));
  expect(gaps.every(item => item.gap >= 4), JSON.stringify(gaps)).toBe(true);
  await page.close();
});
