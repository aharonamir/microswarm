import {test,expect} from '@playwright/test';
import {startConsole,ROOT} from './support.js';
test.fixme('red: T349 — 2000 event session and trajectory meet interaction budgets',async({browser},testInfo)=>{
  const harness=await startConsole(browser,{scenario:'load',mock:false});
  try{
    const page=await harness.context.newPage();const timings={};
    for(const [name,hash,budget] of [['session',`sessions/${ROOT}`,1000],['trajectory',`sessions/${ROOT}/trajectory`,1500]]){
      const started=Date.now();await page.goto(`${harness.baseUrl}/?perf=${name}#/${hash}`);
      if(name==='session'){
        await expect(page.locator('.team-card')).toBeVisible();
        await page.getByLabel('Message the lead').fill('Check interaction');
        await expect(page.getByRole('button',{name:'Send',exact:true})).toBeEnabled();
      }else{await expect(page.locator('.trajectory-step').first()).toBeVisible();await expect(page.locator('.trajectory-inspector')).toBeVisible();await page.getByRole('button',{name:'Problems',exact:true}).focus();}
      timings[name]=Date.now()-started;expect(timings[name],name).toBeLessThan(budget);
    }
    console.log(`SC-U07 ${JSON.stringify(timings)}`);
    await testInfo.attach('SC-U07.json',{body:JSON.stringify(timings,null,2),contentType:'application/json'});
    await page.close();
  }finally{await harness.close();}
});
