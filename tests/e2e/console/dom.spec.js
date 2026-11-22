import { test, expect } from "@playwright/test";
import { readFile, readdir } from "node:fs/promises";
import { resolve, join } from "node:path";

const jsRoot = resolve("../../../crates/ms-console/web/js");

 test("web DOM helpers contain no unsafe HTML or style-attribute writes", async () => {
  async function files(dir) {
    const entries = await readdir(dir, { withFileTypes: true });
    return (await Promise.all(entries.map(entry => entry.isDirectory() ? files(join(dir, entry.name)) : entry.name.endsWith(".js") ? [join(dir, entry.name)] : []))).flat();
  }
  const sources = await Promise.all((await files(jsRoot)).map(path => readFile(path, "utf8")));
  const joined = sources.join("\n");
  expect(joined).not.toMatch(/\.innerHTML\s*=|\.insertAdjacentHTML\s*\(|setAttribute\s*\(\s*["']style["']/);
});
