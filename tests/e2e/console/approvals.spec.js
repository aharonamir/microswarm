import { test, expect } from "@playwright/test";
import { approvals, startConsole } from "./support.js";
let harness;
test.beforeAll(async ({ browser }) => { harness = await startConsole(browser); });
test.afterAll(async () => harness?.close());
const tool = { ...approvals[0], brief: { text: "Run the auth tests" }, policy: { rule: "shell.run = ask", source: "team_skill" }, facts: [{ key: "parse.simple" }] };
async function open(detail, width = 1440) {
 const page = await harness.context.newPage();
 await page.setViewportSize({width,height:900});
 await page.route("**/api/v1/approvals**", route => route.fulfill({json:new URL(route.request().url()).pathname === "/api/v1/approvals" ? {approvals:[detail]} : {approval:detail}}));
 await page.goto(`${harness.baseUrl}/#/approvals/${detail.code}`);
 return page;
}
test.fixme("red: T334 — tool decision explains permissions and discloses exact data", async () => {
 const page = await open(tool);
 await expect(page.locator(".approval-command")).toHaveText("cargo test -p auth");
 await expect(page.locator(".decision-tiles article")).toHaveCount(3);
 await expect(page.locator("blockquote")).toContainText(tool.brief.text);
 await expect(page.locator("details[data-raw]")).not.toHaveAttribute("open");
 await page.close();
});
test.fixme("red: T334 — spawn decision previews access derivation", async () => {
 const page = await open({...tool,kind:"spawn",tool_call:null,spawn:{role:"reviewer",task:"Review auth"},spawn_preview:{derivation:{requested:[],lead:[],ceiling:[],granted:[]}}});
 await expect(page.locator(".approval-title")).toContainText("reviewer");
 await page.getByText("Preview its access",{exact:true}).click();
 await expect(page.locator(".reach-derivation-column")).toHaveCount(4);
 await page.close();
});
test.fixme("red: T334 — decided elsewhere removes actions", async () => {
 const page = await open({...tool,state:"approved"});
 await expect(page.getByText("Already decided on another channel",{exact:true})).toBeVisible();
 await expect(page.locator(".approval-footer button")).toHaveCount(0);
 await page.close();
});
test.fixme("red: T334 — phone footer stays visible with 52 pixel actions", async () => {
 const page = await open(tool,390);
 await expect(page.locator(".approval-command")).toBeVisible();
 await page.locator(".approval-detail-scroll").evaluate(n=>{n.scrollTop=n.scrollHeight;});
 await expect(page.locator(".approval-footer")).toBeInViewport();
 expect((await page.locator(".approval-footer button").first().boundingBox()).height).toBeGreaterThanOrEqual(52);
 expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
 await page.close();
});
