import { test, expect } from '@playwright/test';
import { startConsole, ROOT, TESTER } from './support.js';
let harness;
test.beforeAll(async ({browser})=>{harness=await startConsole(browser,{scenario:'hostile',mock:false});});
test.afterAll(async()=>harness?.close());
const routes=['/home',`/sessions/${ROOT}/team`,`/sessions/${ROOT}/steps?limit=20`,`/sessions/${ROOT}/steps/${TESTER}:1`,`/sessions/${ROOT}/timeline`,`/members/${TESTER}/reach`,`/members/${TESTER}/steps`,`/members/${TESTER}/report`];
test.fixme('red: T348 — new GET views enforce cookie Host and id checks',async({request})=>{
  for(const path of routes){
    expect((await request.get(`${harness.baseUrl}/api/v1${path}`)).status(),path).toBe(401);
    expect((await harness.context.request.get(`${harness.baseUrl}/api/v1${path}`,{headers:{Host:'evil.example'}})).status(),path).toBe(421);
  }
  for(const path of ['/sessions/bad/team','/sessions/bad/steps','/sessions/bad/timeline','/members/bad/reach','/members/bad/report','/members/bad/steps'])expect((await harness.context.request.get(`${harness.baseUrl}/api/v1${path}`)).status(),path).toBe(400);
});
test.fixme('red: T348 — hostile data remains inert with unchanged CSP on every view',async()=>{
  const page=await harness.context.newPage();
  await page.addInitScript(()=>{window.__violations=[];window.__dialogs=[];document.addEventListener('securitypolicyviolation',event=>window.__violations.push(event.violatedDirective));window.alert=text=>window.__dialogs.push(text);});
  for(const hash of ['home','sessions',`sessions/${ROOT}`,'approvals',`sessions/${ROOT}/trajectory`,`sessions/${ROOT}/members/${TESTER}`,'host']){
    const response=await page.goto(`${harness.baseUrl}/#/${hash}`);
    expect(response.headers()['content-security-policy']).toBe("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'");
    await expect(page.locator('#app')).not.toBeEmpty();
    if(hash.includes('/members/'))await expect(page.getByRole('dialog')).toBeVisible();else await expect(page.locator('#app h1')).toBeVisible();
    expect(await page.locator('#app img, #app script, #app [onerror]').count()).toBe(0);
    expect(await page.evaluate(()=>window.__violations)).toEqual([]);expect(await page.evaluate(()=>window.__dialogs)).toEqual([]);
  }
  await page.close();
});
