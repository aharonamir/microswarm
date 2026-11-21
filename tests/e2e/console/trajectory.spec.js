import AxeBuilder from "@axe-core/playwright";
import { test, expect } from "@playwright/test";
import { startConsole, ROOT, TESTER, MEMBER } from "./support.js";
let harness;
test.beforeAll(async ({ browser }) => { harness = await startConsole(browser); });
test.afterAll(async () => harness?.close());
test("lanes rows URL and inspector select the same step", async () => {
 const page=await harness.context.newPage();
 await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/trajectory`);
 await page.locator('.trajectory-step').filter({hasText:'cargo test -p auth'}).click();
 await expect(page).toHaveURL(new RegExp(`${TESTER}:1`));
 await expect(page.locator('.trajectory-inspector')).toContainText('cargo test -p auth');
 await expect(page.locator('.trajectory-step[aria-current="step"]')).toHaveCount(1);
 await expect(page.locator('.trajectory-lane-control[aria-pressed="true"]')).not.toHaveCount(0);
 await page.getByRole('button', { name: /Select researcher step/ }).first().click();
 await expect(page).toHaveURL(new RegExp(`${MEMBER}:1`));
 await page.close();
});
test("filters request server sentences and show phase headings", async () => {
 const page=await harness.context.newPage();
 let query='';page.on('request',r=>{if(r.url().includes('/steps?'))query=r.url();});
 await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/trajectory`);
 await expect(page.locator('.trajectory-phase')).not.toHaveCount(0);
 await page.getByRole('button',{name:'Problems',exact:true}).click();
 await expect.poll(()=>query).toContain('filter=problems');
 await page.close();
});
test("inspector discloses raw events and exports JSON", async () => {
 const page=await harness.context.newPage();
 await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/trajectory/${TESTER}:1`);
 const raw=page.locator('.trajectory-inspector details[data-raw]');
 await expect(raw).not.toHaveAttribute('open');
 await raw.locator('summary').click();await expect(raw.locator('pre')).toBeVisible();
 const download=page.waitForEvent('download');await page.getByRole('button',{name:'Export JSON',exact:true}).click();
 expect((await download).suggestedFilename()).toContain('.json');
 await page.close();
});
test("paging loads older than the latest 200 without duplicates", async () => {
 const page=await harness.context.newPage();
 let pages=0;
 await page.route(`**/api/v1/sessions/${ROOT}/steps?**`,r=>{
 const older=new URL(r.request().url()).searchParams.has('before_seq');pages++;
 const steps=Array.from({length:older?50:200},(_,i)=>({step_id:`${TESTER}:${older?i+1:i+51}`,agent:{id:TESTER,role:'tester',kind:'member',initial:'T',colour_index:2},sentence:{key:'step.shell',args:{command:`command ${older?i+1:i+51}`}},at:'2026-11-19T14:09:00Z',phase_id:'phase-1'}));
 return r.fulfill({json:{steps,phases:[],next_before_seq:older?null:51,last_seq:250}});
 });
 await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/trajectory`);
 await expect(page.locator('.trajectory-step')).toHaveCount(200);
 await page.getByRole('button',{name:'Load older steps',exact:true}).click();
 await expect(page.locator('.trajectory-step')).toHaveCount(250);expect(pages).toBeGreaterThan(1);
 await page.close();
});

for (const width of [1440, 768, 390, 320]) {
 test(`populated trajectory remains accessible at ${width}px`, async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({width,height:900});
  await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}/trajectory/${TESTER}:1`);
  await expect(page.locator('.trajectory-inspector h2')).toContainText('cargo test -p auth');
  const result=await new AxeBuilder({page}).analyze();
  expect(result.violations,JSON.stringify(result.violations.map(v=>({id:v.id,nodes:v.nodes.map(n=>n.target)})))).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  await page.close();
 });
}
