import { api } from "../api.js";
import { append, clear, el } from "../dom.js";
import { duration, localTime } from "../format.js";
import { avatarStack, avatar } from "../components/avatar.js";
import { statusPill } from "../components/pill.js";
import { sentenceNodes, stepRow } from "../components/step.js";
import { teamCard } from "../components/teamcard.js";
import { approvalDock } from "../components/dock.js";
import { teamPanel } from "../components/teampanel.js";
import { agentStrip } from "../components/agentstrip.js";
import { navIcon } from "../components/nav.js";

const states = new Map();
let focusAfterRender = null;
const memberFocus = new Map();

export function captureSessionUi(root) {
  const input = root.querySelector("#message-input");
  const conversation = root.querySelector(".conversation-scroll");
  return input ? {
    value: input.value,
    focused: document.activeElement === input,
    start: input.selectionStart,
    end: input.selectionEnd,
    direction: input.selectionDirection,
    scrollTop: conversation?.scrollTop || 0,
  } : null;
}

export function prefillSessionMessage(sessionId, text) {
  const state = stateFor(sessionId);
  state.draft = text;
  focusAfterRender = sessionId;
}

export function focusMemberOnSession(sessionId, memberId) { memberFocus.set(sessionId, memberId); }

export function sessionState(sessionId) { return stateFor(sessionId); }

function stateFor(id) {
  if (!states.has(id)) states.set(id, { draft: "", inFlight: false, pending: new Map(), completed: new Set(), lastSeq: 0, steps: [], message: "", version: 0, liveTimer: null, lastLiveAt: 0 });
  return states.get(id);
}

export async function renderSession(root, sessionId, t, { snapshot = null, live = false } = {}) {
  const state = stateFor(sessionId);
  const version = ++state.version;
  if (snapshot && !state.draft) state.draft = snapshot.value;
  clear(root);
  let detail, messages, team, approvals, steps, timeline;
  const query = live && state.lastSeq ? `?after_seq=${state.lastSeq}&limit=200` : "?limit=20";
  try {
    [detail, messages, team, approvals, steps, timeline] = await Promise.all([
      api(`/sessions/${sessionId}`),
      api(`/sessions/${sessionId}/messages?limit=100`),
      api(`/sessions/${sessionId}/team`),
      api("/approvals?state=pending"),
      api(`/sessions/${sessionId}/steps${query}`),
      api(`/sessions/${sessionId}/timeline?since=${encodeURIComponent(new Date(Date.now() - 900_000).toISOString())}`),
    ]);
  } catch (error) {
    if (version === state.version) append(root, el("section", { class: "session-error panel", role: "alert" }, [el("h1", { class: "page-title" }, [t("session.load_failed")]), el("p", {}, [error.message])]));
    return null;
  }
  if (version !== state.version) return null;
  if (live && state.steps.length) {
    const known = new Set(state.steps.map(step => step.step_id));
    state.steps = [...state.steps, ...(steps.steps || []).filter(step => !known.has(step.step_id))];
  } else state.steps = steps.steps || [];
  state.lastSeq = Number(steps.last_seq || state.lastSeq || 0);
  const session = detail.session || {};
  const agents = team.agents || [];
  state.agentIds = new Set(agents.map(agent => agent.id));
  const ended = Boolean(session.ended_at || session.ended || session.status_words?.key?.includes("ended"));
  const page = el("section", { class: "session-page", "aria-label": session.title || t("sessions.messages") });
  const main = el("section", { class: "session-main-panel" });
  const heading = el("header", { class: "session-header" }, [
    el("a", { class: "session-back secondary-button button-link", href: "#/home", "aria-label": t("common.back") }, [navIcon("back")]),
    el("div", { class: "session-heading-copy" }, [
      el("h1", { class: "session-title" }, [session.title || t("sessions.new")]),
      el("p", { class: "session-helper" }, [sessionHelper(session, t)]),
      agentStrip(agents, team.groups || [], t),
    ]),
    el("div", { class: "session-header-actions" }, [
      statusPill(session.status_words || { key: "status.unknown" }, t),
      avatarStack(agents, t),
      el("a", { class: "secondary-button button-link", href: `#/sessions/${sessionId}/trajectory` }, [t("session.trajectory")]),
      !ended ? el("button", { class: "secondary-button", type: "button", onclick: () => endSession(sessionId, t, root) }, [t("session.end")]) : el("span", {}, []),
    ]),
  ]);
  const feed = el("div", { class: "conversation-scroll", role: "log", "aria-label": t("sessions.messages"), "aria-live": "polite", tabindex: "0" }, [
    el("div", { class: "conversation-feed" }, renderMessages(messages.items || [], team.groups || [], sessionId, t)),
  ]);
  const dock = approvalDock(approvals.approvals || [], session.root_id || sessionId, t, () => renderSession(root, sessionId, t, { snapshot: captureSessionUi(root) }));
  const composer = ended ? endedNotice(session, t) : composerView(sessionId, state, t, root);
  append(main, heading,
    el("div", { class: `session-status-line${detail.banner?.tone === "attention" ? " session-status-attention" : ""}`, role: "status" }, [statusPill(session.status_words || {}, t), el("span", {}, [detail.banner?.detail || ""]), Number(session.open_approvals || 0) > 0 ? el("a", { href: "#/approvals" }, [t("sessions.review")]) : el("span", {}, [])]),
    feed, state.message ? el("p", { class: "session-error-copy", role: "alert" }, [state.message]) : el("span", {}, []), dock, composer,
  );
  const side = teamPanel(agents, team.groups || [], timeline, sessionId, t);
  append(page, main, side);
  append(root, page);
  restoreUi(root, state, snapshot, focusAfterRender === sessionId);
  if (focusAfterRender === sessionId) focusAfterRender = null;
  const focusId = memberFocus.get(sessionId);
  if (focusId) {
    [...root.querySelectorAll("[data-member-open]")].find(node => node.getAttribute("data-member-open") === focusId)?.focus();
    memberFocus.delete(sessionId);
  }
  return { session, messages: messages.items || [], team, approvals: approvals.approvals || [], steps: state.steps, state };
}

function sessionHelper(session, t) {
  const channel = session.channel || "web";
  const started = session.created_at ? localTime(session.created_at) : "";
  return t("session.helper", { channel, started });
}

function renderMessages(messages, groups, sessionId, t) {
  const byAnchor = new Map(groups.filter(group => group.anchor_message_id).map(group => [group.anchor_message_id, group]));
  const renderedGroups = new Set();
  const output = [];
  for (const message of messages) {
    if (message.kind === "team" && message.team_group_id) {
      const group = groups.find(item => item.group_id === message.team_group_id);
      if (group && !renderedGroups.has(group.group_id)) { output.push(teamCard(group, sessionId, t)); renderedGroups.add(group.group_id); }
      continue;
    }
    if (message.kind === "user") output.push(el("article", { class: "conversation-message message-you", "data-message-id": message.id || "" }, [
      el("p", { class: "message-meta" }, [t("sessions.you"), " · ", localTime(message.at)]),
      el("p", { class: "message-text" }, [message.text || ""]),
    ]));
    else {
      const role = message.from_role || message.agent?.role || "lead";
      const leadMessage = el("article", { class: "conversation-message message-lead", "data-message-id": message.id || "" }, [
        avatar(message.agent || { kind: "lead", role, colour_index: 0, initial: role.slice(0, 1).toUpperCase() }),
        el("div", { class: "lead-message-body" }, [
          el("p", { class: "message-meta" }, [el("strong", {}, [role === "lead" ? t("sessions.lead") : role]), " · ", localTime(message.at), message.based_on_role ? ` · ${t("session.sender_based_on", { role: message.based_on_role })}` : ""]),
          el("p", { class: "message-text" }, [message.text || ""]),
        ]),
      ]);
      output.push(leadMessage);
    }
    const anchored = byAnchor.get(message.id) || groups.find(group => group.group_id === message.team_group_id);
    if (anchored && !renderedGroups.has(anchored.group_id)) { output.push(teamCard(anchored, sessionId, t)); renderedGroups.add(anchored.group_id); }
  }
  for (const group of groups) if (!renderedGroups.has(group.group_id)) output.push(teamCard(group, sessionId, t));
  return output;
}

function findMember(groups, id) {
  for (const group of groups || []) {
    const found = (group.members || []).find(member => member.agent?.id === id);
    if (found) return found;
  }
  return null;
}

function composerView(sessionId, state, t, root) {
  const form = el("form", { class: "session-composer", onsubmit: event => sendMessage(event, sessionId, state, t, root) }, [
    el("label", { class: "label", for: "message-input" }, [t("sessions.message_label")]),
    el("textarea", { id: "message-input", rows: 2, placeholder: t("sessions.placeholder"), maxlength: "8000", disabled: state.inFlight, value: state.draft }),
    el("div", { class: "composer-row" }, [
      el("span", { class: "muted composer-helper" }, [state.inFlight ? t("sessions.still_answering") : t("session.composer_helper")]),
      el("button", { class: "primary-button", type: "submit", disabled: state.inFlight }, [t("sessions.send")]),
    ]),
  ]);
  form.querySelector("textarea").addEventListener("input", event => { state.draft = event.currentTarget.value; });
  return form;
}

async function sendMessage(event, sessionId, state, t, root) {
  event.preventDefault();
  const textarea = event.currentTarget.querySelector("textarea");
  const text = textarea.value.trim();
  if (!text || state.inFlight) return;
  state.inFlight = true;
  state.message = "";
  try {
    const result = await api(`/sessions/${sessionId}/messages`, { method: "POST", body: JSON.stringify({ text }) });
    if (state.completed.delete(result.request_id)) state.inFlight = false;
    else if (result.request_id) state.pending.set(result.request_id, sessionId);
    state.draft = "";
  } catch (error) {
    state.inFlight = false;
    state.message = error.code === "session_busy" ? t("sessions.busy") : error.message;
  }
  await renderSession(root, sessionId, t, { snapshot: { value: state.draft, focused: true, start: 0, end: 0, scrollTop: root.querySelector(".conversation-scroll")?.scrollTop || 0 } });
  root.querySelector("#message-input")?.focus();
}

function endedNotice(session, t) {
  const when = session.ended_at ? localTime(session.ended_at) : "";
  const reason = session.end_reason ? t(`common.${session.end_reason}`, {}) : t("common.ended");
  return el("section", { class: "session-ended-notice" }, [
    el("p", {}, [t("session.ended_at", { time: when, reason })]),
    el("a", { class: "primary-button button-link", href: "#/sessions" }, [t("session.start_new")]),
  ]);
}

function restoreUi(root, state, snapshot, forceFocus) {
  const input = root.querySelector("#message-input");
  const conversation = root.querySelector(".conversation-scroll");
  if (input) {
    input.value = state.draft;
    if (snapshot?.focused || forceFocus) {
      input.focus();
      input.setSelectionRange(snapshot?.start ?? input.value.length, snapshot?.end ?? input.value.length, snapshot?.direction || "none");
    }
  }
  if (conversation && snapshot) conversation.scrollTop = snapshot.scrollTop || 0;
}

async function endSession(sessionId, t, root) {
  if (!confirm(t("sessions.confirm_end"))) return;
  const state = stateFor(sessionId);
  try { await api(`/sessions/${sessionId}/end`, { method: "POST", body: "{}" }); }
  catch (error) { state.message = error.message; }
  await renderSession(root, sessionId, t);
}

export async function handleSessionChatEvent(type, raw, render, sessionId) {
  if (!["chat.pending", "chat.reply", "chat.error"].includes(type)) return false;
  let event;
  try { event = JSON.parse(raw); } catch { return true; }
  const state = stateFor(event.session_id || sessionId || "");
  if (["chat.reply", "chat.error"].includes(type)) {
    if (state.pending.has(event.request_id)) { state.pending.delete(event.request_id); state.inFlight = false; }
    else state.completed.add(event.request_id);
    if (type === "chat.error") state.message = event.error || "";
  }
  if (event.session_id && !sessionId) location.hash = `#/sessions/${event.session_id}`;
  await render();
  return true;
}
