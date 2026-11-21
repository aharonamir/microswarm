import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createServer } from "node:net";
import { createInterface } from "node:readline";
import { expect } from "@playwright/test";

export const ROOT = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
export const MEMBER = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
export const TESTER = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
const NOW = "2026-11-19T14:09:00Z";

const agent = (id, role, colour_index) => ({ id, role, kind: role === "lead" ? "lead" : "member", colour_index, initial: role[0].toUpperCase() });
const status = (key, tone = "neutral") => ({ key: `status.${key}`, args: {}, tone });
const lead = agent(ROOT, "lead", 0);
const researcher = agent(MEMBER, "researcher", 1);
const tester = agent(TESTER, "tester", 2);
const session = { id: ROOT, root_id: ROOT, title: "Audit the auth module", role: "lead", kind: "lead", channel: "web", status_words: status("waiting_for_you", "attention"), in_flight: false, ended_at: null, member_count: 2 };
export const approvals = [{
  id: "01ARZ3NDEKTSV4RRFFQ69G5FB2", code: "K7QF", state: "pending", kind: "tool_call", root_session_id: ROOT,
  requesting_role: "tester", agent: tester, sentence: { key: "step.approval.asked", args: { role: "tester", what: "tool" } },
  tool_call: { server: "microswarm-tools", tool: "shell.run", arguments: { command: "cargo test -p auth" }, argument_hash: "sha256:abc", parsed: { kind: "simple", commands: ["cargo test -p auth"] } },
  runs_in: { folders: [{ path: "/work", mode: "ro" }], network: "none", credentials: [], tools_ask: ["shell.run"] }, expires_at: "2026-11-19T14:20:00Z",
}];
const groups = [{
  group_id: "group-1", anchor_message_id: "message-lead-1", started_at: "2026-11-19T14:08:00Z", more: 0,
  members: [
    { agent: researcher, delegation_id: "D001", task: "Inspect the authentication refresh path", task_truncated: false, status: status("working", "working"), state: "working", step_count: 2, started_at: "2026-11-19T14:08:10Z", elapsed_ms: 50000, current_step: { key: "step.read", args: { path: "refresh.rs" } }, reach_summary: { folders: [{ path: "/work/src/auth", mode: "ro" }], network: "none", credentials: [], tools_ask: [] }, reduced_count: 1 },
    { agent: tester, delegation_id: "D002", task: "Run the auth tests and report failures", task_truncated: false, status: status("waiting_for_you", "attention"), state: "waiting_you", step_count: 3, started_at: "2026-11-19T14:08:15Z", elapsed_ms: 45000, pending_approval: { id: approvals[0].id, code: "K7QF", summary: approvals[0].sentence }, reach_summary: { folders: [{ path: "/work", mode: "ro" }], network: "none", credentials: [], tools_ask: [] }, reduced_count: 1 },
  ],
}];
const steps = [
  { step_id: `${MEMBER}:1`, agent: researcher, at: NOW, ended_at: NOW, duration_ms: 200, kind: "tool", sentence: { key: "step.read", args: { path: "refresh.rs" } }, decision: { value: "allowed" }, event_seqs: [1], phase_id: "phase-1" },
  { step_id: `${TESTER}:1`, agent: tester, at: NOW, kind: "tool", sentence: { key: "step.shell.ask", args: { command: "cargo test -p auth" } }, decision: { value: "asked_you" }, approval: { id: approvals[0].id, code: "K7QF", state: "pending" }, event_seqs: [2], phase_id: "phase-1" },
];
const messages = [
  { id: "message-user-1", kind: "user", at: "2026-11-19T14:08:00Z", text: "Inspect the refresh flow and test it." },
  { id: "message-lead-1", kind: "lead", at: NOW, text: "The refresh path validates the token before reuse.", based_on_role: "researcher", team_group_id: null },
  { id: "message-user-2", kind: "user", at: NOW, text: "Please confirm the test command." },
  { id: "message-team-1", kind: "team", at: NOW, team_group_id: "group-1", team: { detail: "Two members started." } },
];
const reach = {
  summary: { key: "reach.summary", args: { folders: 1, network: "none", credentials: 0 } },
  can_reach: [{ icon: "folder", tag: "read only", text: { key: "reach.folder", args: { path: "/work/src/auth" } } }],
  reduced: [{ dimension: "mount", item: "/work/src/auth", reason: "mode_downgraded", text: { key: "reach.reason.mode_downgraded", args: {} } }],
  derivation: {
    requested: [{ text: "/work/src/auth read/write", struck: false, lowered: true }], lead: [{ text: "/work/src/auth read/write", struck: false, lowered: false }],
    ceiling: [{ text: "No additional host grants", struck: false, lowered: false }], granted: [{ text: "/work/src/auth read only", struck: false, lowered: false }],
  },
  container: { files_key: "reach.container.files", process_key: "reach.container.process", network_key: "reach.container.network", args: { mounts: 1, root_read_only: true, no_new_privileges: true, network: "none" } },
  raw: { granted: { mounts: [{ container_path: "/work/src/auth", mode: "ro" }], tools: [], credentials: [] }, requested: {}, lead: {}, ceiling: {} },
};

export async function startConsole(browser) {
  const home = await mkdtemp(join(tmpdir(), "ms-console-005-e2e-"));
  const fixtureBinary = process.env.MS_CONSOLE_FIXTURE_BIN || resolve("../../../target/debug/console_status_fixture");
  const fixture = spawn(fixtureBinary, [home], { stdio: ["pipe", "pipe", "inherit"] });
  const fixtureLines = createInterface({ input: fixture.stdout })[Symbol.asyncIterator]();
  await expectLine(fixtureLines, "READY");
  const port = await freePort();
  const binary = process.env.MS_CONSOLE_BIN || resolve("../../../target/debug/microswarm");
  const child = spawn(binary, ["console", "--listen", `127.0.0.1:${port}`], { env: { ...process.env, MS_HOME: home }, stdio: ["ignore", "ignore", "pipe"] });
  let token = "";
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", chunk => { stderr += chunk; token ||= stderr.match(/token=([^\s]+)/)?.[1] || ""; });
  for (let attempt = 0; attempt < 100 && !token; attempt++) await new Promise(resolve => setTimeout(resolve, 50));
  if (!token) throw new Error(`console did not print a startup token: ${stderr}`);
  await expectLine(fixtureLines, "CONNECTED");
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  installDemoApi(context);
  await context.addInitScript(() => {
    window.EventSource = class {
      constructor() { this.listeners = {}; window.__consoleEvents = this; setTimeout(() => this.onopen?.(), 0); }
      addEventListener(type, callback) { this.listeners[type] = callback; }
      close() {}
      emit(type, data) { this.listeners[type]?.({ data: typeof data === "string" ? data : JSON.stringify(data), lastEventId: "1" }); }
    };
  });
  const login = await context.newPage();
  login.on("pageerror", error => console.error(`console page error: ${error.message}`));
  login.on("console", message => { if (message.type() === "error") console.error(`console browser error: ${message.text()}`); });
  await login.goto(`http://127.0.0.1:${port}/login?token=${encodeURIComponent(token)}`);
  await expect(login).toHaveURL(`http://127.0.0.1:${port}/#/home`);
  await login.close();
  return {
    context,
    baseUrl: `http://127.0.0.1:${port}`,
    async close() {
      await context.close();
      child.kill("SIGTERM");
      fixture.kill("SIGTERM");
      await rm(home, { recursive: true, force: true });
    },
  };
}

export function installDemoApi(context) {
  const state = { approved: false, ended: false, calls: [] };
  context.route("**/api/v1/**", route => mockApi(route, state));
  return state;
}

async function mockApi(route, state) {
  const url = new URL(route.request().url());
  const method = route.request().method();
  state.calls.push({ method, path: `${url.pathname}${url.search}` });
  if (url.pathname === "/api/v1/events") return route.fulfill({ status: 200, contentType: "text/event-stream", body: "retry: 30000\n\n" });
  if (method === "POST") {
    if (url.pathname.endsWith("/approve")) state.approved = true;
    if (url.pathname.endsWith("/messages")) return route.fulfill({ status: 202, json: { request_id: "request-1", thread_id: "web-thread", session_id: ROOT } });
    return route.fulfill({ status: 200, json: { decision: "approved" } });
  }
  let body = {};
  if (url.pathname === "/api/v1/overview") body = { pending_approvals: state.approved ? 0 : 1 };
  else if (url.pathname === "/api/v1/home") body = { summary: { needs_you: 1, running: 1 }, needs_you: [], running: [], ended_today: [], system: {} };
  else if (url.pathname === "/api/v1/sessions") body = { sessions: url.searchParams.get("group") === "ended" ? [] : [session] };
  else if (url.pathname === `/api/v1/sessions/${ROOT}`) body = { session: state.ended ? { ...session, ended_at: "2026-11-19T14:30:00Z", end_reason: "done", status_words: status("ended") } : session, team: [], banner: { tone: "attention", detail: "The tester needs your approval." } };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/messages`) body = { items: messages, next_before: null };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/team`) body = { groups: groups, agents: [lead, researcher, tester] };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/steps`) {
    const selected = url.searchParams.get("agent");
    const after = Number(url.searchParams.get("after_seq") || 0);
    const result = steps.filter(step => (!selected || step.agent.id === selected) && Number(step.event_seqs[0]) > after);
    body = { steps: result, phases: [{ phase_id: "phase-1", title_key: "phase.parallel", title_args: { count: 2 }, started_at: NOW, step_ids: result.map(step => step.step_id) }], next_before_seq: null, last_seq: 2 };
  } else if (url.pathname.startsWith(`/api/v1/sessions/${ROOT}/steps/`)) { const step = steps.find(step => step.step_id === decodeURIComponent(url.pathname.split("/").at(-1))); body = { step, explanation: { facts: [] }, events: [{ type: "tool.call", data: { command: "cargo test -p auth" } }] }; }
  else if (url.pathname === `/api/v1/sessions/${ROOT}/timeline`) body = { range: { start: "2026-11-19T14:08:00Z", end: NOW }, stats: { duration_ms: 60000, agents: 3, steps: 5, waiting_you_ms: 12000, reduced: 2, denied_or_failed: 0, model_calls: 2 }, lanes: [lead, researcher, tester].map(agent => ({ agent, spans: [{ start: "2026-11-19T14:08:00Z", end: NOW, state: agent.id === TESTER ? "waiting_you" : "working" }], marks: [{ at: NOW, kind: agent.id === TESTER ? "approval" : "tool", step_id: agent.id === TESTER ? `${TESTER}:1` : `${MEMBER}:1` }] })) };
  else if (url.pathname === "/api/v1/approvals") body = { approvals: state.approved ? [] : approvals };
  else if (url.pathname === `/api/v1/approvals/${approvals[0].id}`) body = { approval: approvals[0] };
  else if (url.pathname === `/api/v1/members/${MEMBER}/reach` || url.pathname === `/api/v1/members/${TESTER}/reach`) body = { ...reach };
  else if (url.pathname === `/api/v1/members/${MEMBER}/report` || url.pathname === `/api/v1/members/${TESTER}/report`) body = { text: "Status: complete\nThe refresh path validates tokens before reuse.", at: NOW, status: "done" };
  else if (url.pathname === `/api/v1/members/${MEMBER}/scope` || url.pathname === `/api/v1/members/${TESTER}/scope`) body = { member_id: url.pathname.includes(TESTER) ? TESTER : MEMBER, lead_id: ROOT, role: url.pathname.includes(TESTER) ? "tester" : "researcher", source_ref: "auth-review@1.2.0" };
  else if (url.pathname === `/api/v1/members/${MEMBER}/steps` || url.pathname === `/api/v1/members/${TESTER}/steps`) body = { steps, phases: [], next_before_seq: null, last_seq: 2 };
  else if (url.pathname === `/api/v1/sessions/${ROOT}/reach`) body = { ...reach, derivation: null, reduced: [] };
  else body = {};
  return route.fulfill({ status: 200, json: body });
}

function expectLine(iterator, expected) {
  return iterator.next().then(result => {
    if (result.done || result.value !== expected) throw new Error(`expected fixture ${expected}, received ${result.value}`);
  });
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(error => error ? reject(error) : resolve(port));
    });
  });
}
