import { test, expect } from '@playwright/test';
import { startConsole, expectView, ROOT, TESTER } from './support.js';
let harness;
test.beforeAll(async ({browser})=>{harness=await startConsole(browser,{scenario:'hostile',mock:false});});
test.afterAll(async()=>harness?.close());
const routes=['/home',`/sessions/${ROOT}/team`,`/sessions/${ROOT}/steps?limit=20`,`/sessions/${ROOT}/steps/${TESTER}:1`,`/sessions/${ROOT}/timeline`,`/members/${TESTER}/reach`,`/sessions/${ROOT}/reach`,`/members/${TESTER}/report`];
test('new GET views enforce cookie Host and id checks',async({request})=>{
  for(const path of routes){
    expect((await request.get(`${harness.baseUrl}/api/v1${path}`)).status(),path).toBe(401);
    const post = await harness.context.request.post(`${harness.baseUrl}/api/v1${path}`,{headers:{Origin:harness.baseUrl,'X-MS-Console':'1'},data:{}});
    expect(post.status(),`${path} must be read only`).toBe(404);
    expect((await harness.context.request.post(`${harness.baseUrl}/api/v1${path}`,{headers:{Origin:'https://evil.example','X-MS-Console':'1'},data:{}})).status()).toBe(403);
    expect((await harness.context.request.get(`${harness.baseUrl}/api/v1${path}`,{headers:{Host:'evil.example'}})).status(),path).toBe(421);
  }
  for(const path of ['/sessions/bad/team','/sessions/bad/steps','/sessions/bad/timeline','/sessions/bad/reach','/sessions/bad/steps/nope','/members/bad/reach','/members/bad/report'])expect((await harness.context.request.get(`${harness.baseUrl}/api/v1${path}`)).status(),path).toBe(400);
});
test('hostile data remains inert with unchanged CSP on every view',async()=>{
  const page=await harness.context.newPage();
  await page.addInitScript(()=>{window.__violations=[];window.__dialogs=[];document.addEventListener('securitypolicyviolation',event=>window.__violations.push(event.violatedDirective));window.alert=text=>window.__dialogs.push(text);});
  for(const hash of ['home','sessions',`sessions/${ROOT}`,'approvals/K7QF',`sessions/${ROOT}/trajectory`,`sessions/${ROOT}/members/${TESTER}`,'host']){
    const response=await page.goto(`${harness.baseUrl}/?view=${encodeURIComponent(hash)}#/${hash}`);
    expect(response.headers()['content-security-policy']).toBe("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'");
    await expect(page.locator('#app')).not.toBeEmpty();
    await expectView(page,hash);
    if(hash==='approvals/K7QF') await expect(page.locator('.approval-command')).toContainText('<img src=x onerror=alert(1)>');
    expect(await page.locator('#app img, #app script, #app [onerror]').count()).toBe(0);
    expect(await page.evaluate(()=>window.__violations)).toEqual([]);expect(await page.evaluate(()=>window.__dialogs)).toEqual([]);
  }
  await page.close();
});
