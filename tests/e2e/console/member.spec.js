import { test, expect } from "@playwright/test";
import { startConsole, ROOT, MEMBER, TESTER } from "./support.js";

let harness;

test.beforeAll(async ({ browser }) => { harness = await startConsole(browser); });
test.afterAll(async () => harness?.close());

test("the reach tab explains reduced access and its derivation", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/members/${TESTER}`);
  await expect(page.getByText("Accessible folders: 1. Network access and host credentials are disabled.")).toBeVisible();
  await expect(page.getByText("lowered to read only")).toBeVisible();
  for (const label of ["Lead asked", "Lead holds", "Host allows", "Granted"]) await expect(page.getByText(label, { exact: true })).toBeVisible();
  await expect(page.getByText(/The container mounts 1 folders/)).toBeVisible();
  await page.close();
});

test("the drawer tabs show steps, reach, and a plain-text report", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/members/${MEMBER}`);
  await expect(page.getByText("Its task, from the lead")).toBeVisible();
  await page.getByRole("tab", { name: /Steps/ }).click();
  await expect(page.getByRole("tabpanel").getByText("Read refresh.rs")).toBeVisible();
  await page.getByRole("tab", { name: "Report" }).click();
  const report = page.locator(".member-report");
  await expect(report).toContainText("The refresh path validates tokens before reuse.");
  await expect(report.locator("a, button, input")).toHaveCount(0);
  await page.close();
});

test("desktop drawer leaves the conversation visible and restores focus", async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  const trigger = page.locator(`.team-member-row [data-member-open="${MEMBER}"]`);
  await trigger.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.locator(".conversation-scroll")).toBeVisible();
  await expect(page.locator(".member-content-dim")).toBeVisible();
  await expect(page.locator(".session-team-sidebar")).toBeHidden();
  const drawer = await page.locator("dialog").boundingBox();
  expect(drawer.x).toBeGreaterThan(0);
  expect(drawer.x + drawer.width).toBe(1440);
  const approve = await page.getByRole("button", { name: "Approve this command" }).boundingBox();
  expect(approve.x + approve.width).toBeLessThanOrEqual(drawer.x);
  await page.getByRole("button", { name: "Close member details" }).click();
  await expect(page).toHaveURL(new RegExp(`#\\/sessions\\/${ROOT}$`));
  await expect(page.locator(`.team-member-row [data-member-open="${MEMBER}"]`)).toBeFocused();
  await page.close();
});

test("dismiss prefill focuses the composer without sending", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/members/${MEMBER}`);
  await page.getByRole("button", { name: "Ask the lead to dismiss it" }).click();
  await expect(page).toHaveURL(new RegExp(`#\\/sessions\\/${ROOT}$`));
  const composer = page.getByLabel("Message the lead");
  await expect(composer).toHaveValue("Please dismiss the researcher.");
  await expect(composer).toBeFocused();
  await expect(page.locator("[data-method='POST']")).toHaveCount(0);
  await page.close();
});

test("the old member route resolves to its owning session", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/members/${MEMBER}`);
  await expect(page).toHaveURL(new RegExp(`#\\/sessions\\/${ROOT}\\/members\\/${MEMBER}$`));
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.close();
});

test("the mobile member drawer fits the viewport at 390 and 320 pixels", async () => {
  const page = await harness.context.newPage();
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 720 });
    await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/members/${TESTER}`);
    await expect(page.getByRole("dialog")).toBeVisible();
    const layout = await page.evaluate(() => ({ viewport: innerWidth, scroll: document.documentElement.scrollWidth, drawerRight: document.querySelector("dialog").getBoundingClientRect().right }));
    expect(layout.scroll).toBeLessThanOrEqual(layout.viewport);
    expect(layout.drawerRight).toBeLessThanOrEqual(layout.viewport);
  }
  await page.close();
});

test("Escape closes the member drawer and returns focus to its row", async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
  const trigger = page.locator(`.team-member-row [data-member-open="${TESTER}"]`);
  await trigger.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`#\\/sessions\\/${ROOT}$`));
  await expect(page.locator(`.team-member-row [data-member-open="${TESTER}"]`)).toBeFocused();
  await page.close();
});

test("a live session update keeps the member drawer and its selected tab open", async () => {
  const page = await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/members/${MEMBER}`);
  await page.getByRole("tab", { name: "Report" }).click();
  await page.evaluate(({ rootId, memberId }) => window.__consoleEvents.emit("agent", { root_session_id: rootId, session_id: memberId }), { rootId: ROOT, memberId: MEMBER });
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("tab", { name: "Report" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".member-report")).toContainText("The refresh path validates tokens before reuse.");
  await page.close();
});
