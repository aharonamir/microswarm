import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve("../../../crates/ms-console/web/i18n");

test("English and Chinese dictionaries have matching keys and placeholders", async () => {
  const en = JSON.parse(await readFile(`${root}/en.json`, "utf8"));
  const zh = JSON.parse(await readFile(`${root}/zh.json`, "utf8"));
  expect(Object.keys(zh).sort()).toEqual(Object.keys(en).sort());
  const slots = value => [...value.matchAll(/\{([a-z_]+)\}/g)].map(match => match[1]).sort();
  for (const key of Object.keys(en)) expect(slots(zh[key]), key).toEqual(slots(en[key]));
});

import {startConsole,ROOT,TESTER} from './support.js';
for(const locale of ['en','zh']) test.fixme(`red: T346 — long ${locale} labels fit at four widths`,async({browser})=>{
  const harness=await startConsole(browser,{scenario:'demo',mock:false});
  try{
    const page=await harness.context.newPage();
    await page.addInitScript(lang=>localStorage.setItem('ms-lang',lang),locale);
    await page.route(`**/i18n/${locale}.json`,async route=>{
      const response=await route.fetch();const dictionary=await response.json();
      for(const key of ['common.close','sessions.end','dock.approve_command','sheet.review','home.start','host.add_credential'])dictionary[key]=`${dictionary[key]} ${dictionary[key]}`;
      await route.fulfill({json:dictionary});
    });
    for(const width of [1440,768,390,320]){
      await page.setViewportSize({width,height:900});
      for(const hash of ['home',`sessions/${ROOT}`,`sessions/${ROOT}/members/${TESTER}`,'host']){
        await page.goto(`${harness.baseUrl}/?locale=${locale}&view=${encodeURIComponent(hash)}#/${hash}`);
        if(hash.includes('/members/'))await expect(page.getByRole('dialog')).toBeVisible();else await expect(page.locator('#app h1')).toBeVisible();
        expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`${locale} ${width} ${hash}`).toBe(true);
        const clipped=await page.locator('button').evaluateAll(buttons=>buttons.filter(button=>button.getClientRects().length&&button.scrollWidth>button.clientWidth+1).map(button=>button.textContent));
        expect(clipped,`${locale} ${width} ${hash}`).toEqual([]);
      }
    }
    await page.close();
  }finally{await harness.close();}
});
