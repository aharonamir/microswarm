import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const ROOT = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const MEMBER = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const hostile = "<img src=x onerror=alert(1)> javascript:alert(1) data:text/html,<script>";
const session = { id: ROOT, root_id: ROOT, title: hostile, kind: "lead", role: "lead", channel: "web", status_words: { key: "status.ready", tone: "ready", args: {} }, member_count: 1, open_approvals: 1, in_flight: false, scope: { mounts: [] } };

let child;
let home;
let token;
let context;

test.describe.configure({ mode: "serial" });

test.beforeAll(async ({ browser }) => {
  home = await mkdtemp(join(tmpdir(), "ms-console-e2e-"));
  const binary = process.env.MS_CONSOLE_BIN || resolve("../../../target/debug/microswarm");
  child = spawn(binary, ["console", "--listen", "127.0.0.1:8792"], { env: { ...process.env, MS_HOME: home }, stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", chunk => { stderr += chunk; token ||= stderr.match(/token=([^\s]+)/)?.[1]; });
  for (let attempt = 0; attempt < 100 && !token; attempt++) await new Promise(resolve => setTimeout(resolve, 50));
  if (!token) throw new Error(`console did not print a startup token: ${stderr}`);
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const login = await context.newPage();
  await login.route("**/api/v1/**", route => mockApi(route));
  await login.goto(`/login?token=${encodeURIComponent(token)}`);
  await expect(login).toHaveURL("http://127.0.0.1:8792/");
  await login.close();
});

test.afterAll(async () => {
  await context?.close();
  child?.kill("SIGTERM");
  if (home) await rm(home, { recursive: true, force: true });
});

for (const viewport of [{ width: 1440, height: 900 }, { width: 390, height: 844 }]) {
  for (const pageCase of [
    { name: "sessions", hash: `#/sessions/${ROOT}`, heading: hostile },
    { name: "approvals", hash: "#/approvals", heading: "microswarm-tools / shell.run" },
    { name: "trace", hash: `#/trace/${ROOT}`, heading: "Trace" },
    { name: "host", hash: "#/host", heading: "Host" },
    { name: "member", hash: `#/members/${MEMBER}`, heading: "reviewer" },
  ]) {
    test(`${pageCase.name} is usable and accessible at ${viewport.width}px`, async () => {
      const page = await context.newPage();
      await page.setViewportSize(viewport);
      await page.route("**/api/v1/**", route => mockApi(route));
      await page.goto(`/#${pageCase.hash.slice(1)}`);
      await expect(page.locator("#app")).toContainText(pageCase.heading);
      const axe = await new AxeBuilder({ page }).analyze();
      expect(axe.violations, JSON.stringify(axe.violations.map(item => ({ id: item.id, nodes: item.nodes.map(node => node.target) })))).toEqual([]);
      await page.close();
    });
  }
}

test("hostile agent text stays inert in a real browser", async () => {
  const page = await context.newPage();
  await page.route("**/api/v1/**", route => mockApi(route));
  await page.goto(`/#/sessions/${ROOT}`);
  await expect(page.locator(".message-user .message-text")).toContainText(hostile);
  await expect(page.locator("img, script:not([src])")).toHaveCount(0);
  await expect(page.locator("[role=alert]")).toHaveCount(0);
  await page.close();
});

async function mockApi(route) {
  const url = new URL(route.request().url());
  if (url.pathname === "/api/v1/events") return route.fulfill({ status: 200, contentType: "text/event-stream", body: "retry: 30000\n\n" });
  if (route.request().method() !== "GET") return route.fulfill({ status: 204, body: "" });
  let body = {};
  if (url.pathname === "/api/v1/overview") body = { pending_approvals: 1 };
  else if (url.pathname === "/api/v1/sessions") body = { sessions: [session] };
  else if (url.pathname === `/api/v1/sessions/${ROOT}`) body = { session, team: [{ ...session, id: MEMBER, role: "reviewer", scope: { mounts: [] } }], banner: { tone: "ready", detail: "Supervisor status" } };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/messages`) body = { items: [{ kind: "user", at: new Date().toISOString(), text: hostile }, { kind: "lead", at: new Date().toISOString(), text: "Lead reply" }], next_before: null };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/activity`) body = { events: [{ observation: true, agent_role: "reviewer", type: "tool.result", at: new Date().toISOString(), data: { text: hostile } }] };
  else if (url.pathname === "/api/v1/approvals") body = { approvals: [{ id: "approval-1", code: "A1B2", state: "pending", kind: "tool_call", root_session_id: ROOT, requesting_role: "reviewer", tool_call: { server: "microswarm-tools", tool: "shell.run", arguments: { command: "cargo test" }, argument_hash: "sha256:abc", parsed: { kind: "simple", commands: ["cargo test"] } }, runs_in: {} }] };
  else if (url.pathname === "/api/v1/approvals/A1B2") body = { approval: (await mockApiValue("/api/v1/approvals")).approvals[0] };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/trace`) body = { sessions: [], events: [{ seq: 1, session_id: ROOT, source: "agent", type: "tool.result", agent_role: "reviewer", ts: new Date().toISOString(), data: { text: hostile } }] };
  else if (url.pathname === "/api/v1/host") body = { host: { doctor: [{ check: "configuration", result: "pass", detail: "Ready" }], channels: [{ kind: "telegram", detail: "Configured" }], skills: [], credentials: [{ name: "model-key", used_by: ["lead"] }], limits: {}, ceiling: {} } };
  else if (url.pathname === `/api/v1/members/${MEMBER}/scope`) body = { member_id: MEMBER, lead_id: ROOT, role: "reviewer", source_ref: "delegation-1", rows: [{ dimension: "mounts", requested: [], lead: [], ceiling: [], granted: [], narrowed: [] }], profile: { filesystem: { root_read_only: true } } };
  else return route.fulfill({ status: 404, json: { error: "not_found" } });
  return route.fulfill({ status: 200, json: body });
}

async function mockApiValue(path) {
  return { approvals: [{ id: "approval-1", code: "A1B2", state: "pending", kind: "tool_call", root_session_id: ROOT, requesting_role: "reviewer", tool_call: { server: "microswarm-tools", tool: "shell.run", arguments: { command: "cargo test" }, argument_hash: "sha256:abc", parsed: { kind: "simple", commands: ["cargo test"] } }, runs_in: {} }] };
}
