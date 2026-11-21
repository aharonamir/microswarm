import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { startConsole, ROOT, TESTER } from './support.js';
let harness;
test.beforeAll(async ({browser}) => { harness = await startConsole(browser, {scenario:'demo',mock:false}); });
test.afterAll(async () => harness?.close());
for (const width of [1440,768,390,320]) test.fixme(`red: T345 — populated views and sheet accessibility at ${width}px`, async () => {
  const page = await harness.context.newPage();
  await page.setViewportSize({width,height:900});
  for (const hash of ['home','sessions',`sessions/${ROOT}`,'approvals/K7QF',`sessions/${ROOT}/trajectory`,`sessions/${ROOT}/members/${TESTER}`,'host']) {
    await page.goto(`${harness.baseUrl}/#/${hash}`);
    await expect(page.locator('#app')).not.toBeEmpty();
    if(hash.includes('/members/')) await expect(page.getByRole('dialog')).toBeVisible();
    else await expect(page.locator('#app h1')).toBeVisible();
    expect(await page.evaluate(()=>document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const result = await new AxeBuilder({page}).analyze();
    expect(result.violations.map(v=>({id:v.id,nodes:v.nodes.map(n=>n.target)})),hash).toEqual([]);
  }
  if(width<700) {
    await page.goto(`${harness.baseUrl}/#/sessions/${ROOT}`);
    await page.getByRole('button',{name:'Review',exact:true}).click();
    expect((await new AxeBuilder({page}).analyze()).violations.map(v=>v.id)).toEqual([]);
  }
  await page.close();
});
test.fixme('red: T345 — approval segments support arrow keys and credential cancel returns focus',async()=>{
  const page=await harness.context.newPage();
  await page.goto(`${harness.baseUrl}/#/approvals`);
  await page.getByRole('button',{name:'Waiting',exact:true}).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button',{name:'Decided',exact:true})).toBeFocused();
  await expect(page.getByRole('button',{name:'Decided',exact:true})).toHaveAttribute('aria-pressed','true');
  await page.goto(`${harness.baseUrl}/#/host`);
  const add=page.getByRole('button',{name:'Add credential'});await add.click();
  await page.keyboard.press('Escape');await expect(add).toBeFocused();
  await page.close();
});
