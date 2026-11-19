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
