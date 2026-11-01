import { api } from "../api.js";
import { clear, el } from "../dom.js";

let inFlight = new Set();
const pendingLocks = new Map();
const completedRequests = new Set();
const olderPages = new Map();
let newDraft = false;
let requestError = "";
let composerPrefill = "";

const statusText = (view, t) => t(view?.status_words?.key || "", view?.status_words?.args) || view?.status_words?.en || "Unknown";
const timeText = value => { const date = new Date(value); return Number.isNaN(date.valueOf()) ? "" : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }); };

export async function renderSessions(root, selectedId, t) {
  clear(root);
  let overview;
  try { overview = await api("/sessions?group=active"); }
  catch (error) { root.append(el("section", { class: "panel page-main" }, [el("h1", { class: "page-title" }, [t("nav.sessions")]), el("p", { role: "alert" }, [error.message])])); return; }
  const sessions = overview.sessions || [];
  const selected = selectedId ? sessions.find(item => item.id === selectedId) : null;
  if (selectedId && !selected && !newDraft) newDraft = false;
  const sidebar = el("aside", { class: "page-rail panel" }, [
    el("button", { class: "primary-button new-session", onclick: () => { newDraft = true; requestError = ""; location.hash = "#/sessions"; renderSessions(root, null, t); } }, [`＋ ${t("sessions.new")}`]),
    el("h2", { class: "section-title rail-heading" }, [t("sessions.your_sessions")]),
    el("div", { class: "session-list" }, sessions.length ? sessions.map(item => el("a", { class: "session-link", href: `#/sessions/${item.id}`, "aria-current": item.id === selectedId ? "page" : false }, [
      el("strong", {}, [item.title || t("sessions.new")]),
      el("span", { class: `status-line tone-${item.status_words?.tone || "neutral"}` }, [el("span", { class: "status-dot" }), statusText(item, t)]),
      el("span", { class: "session-meta" }, [`${item.member_count || 0} ${t("sessions.members")} · ${item.channel || "web"}`]),
    ])) : [el("p", { class: "muted empty-copy" }, [t("sessions.none")])]),
  ]);

  let detail = null;
  let messages = { items: [] };
  let approvals = { approvals: [] };
  let activity = { events: [] };
  if (selected) {
    try { [detail, messages, approvals, activity] = await Promise.all([api(`/sessions/${selected.id}`), api(`/sessions/${selected.id}/messages?limit=100`), api("/approvals?state=pending"), api(`/sessions/${selected.id}/activity`)]); }
    catch (error) { requestError = error.message; }
    const older = olderPages.get(selected.id);
    if (older) {
      messages.items = [...older.items, ...(messages.items || [])];
      messages.next_before = older.nextBefore;
    }
  }
  const main = el("section", { class: "page-main session-main" });
  if (selected && detail) {
    const view = detail.session;
    const banner = el("div", { class: `status-banner tone-${detail.banner?.tone || view.status_words?.tone || "neutral"}`, role: "status", "aria-live": "polite" }, [
      el("span", { class: "status-dot" }), el("strong", {}, [statusText(view, t)]), el("span", {}, [detail.banner?.detail || ""]),
      Number(view.open_approvals) > 0 ? el("a", { href: "#/approvals" }, [t("sessions.review")]) : el("span", {}, []),
    ]);
    const messageItems = (messages.items || []).map(item => el("article", { class: `message message-${item.kind || "team"}` }, [
      el("div", { class: "message-meta" }, [`${item.kind === "user" ? t("sessions.you") : item.kind === "lead" ? t("sessions.lead") : t("sessions.team")} · ${timeText(item.at)}${item.from_role ? ` · ${t("sessions.from")} ${item.from_role}` : ""}`]),
      item.kind === "team" ? el("div", { class: "team-marker" }, [item.team?.detail || t("sessions.team_update")]) : el("p", { class: "message-text" }, [item.text || ""]),
    ]));
    const inlineApprovals = (approvals.approvals || []).filter(item => item.root_session_id === view.root_id).map(item => el("article", { class: "approval-card" }, [
      el("p", { class: "label" }, [t("sessions.approval_title")]),
      el("h3", {}, [`${item.requesting_role || "Member"} · ${t("sessions.wants_approval")}`]),
      el("p", { class: "mono" }, [item.code]),
      item.tool_call ? el("pre", { class: "command-block mono" }, [JSON.stringify(item.tool_call.arguments || {}, null, 2)]) : el("p", {}, [item.spawn?.task || ""]),
      el("div", { class: "approval-actions" }, [
        el("button", { class: "secondary-button", onclick: () => decide(item.code, "approve", t) }, [t("approvals.approve")]),
        el("button", { class: "secondary-button deny-button", onclick: () => decide(item.code, "deny", t) }, [t("approvals.deny")]),
        el("a", { href: "#/approvals" }, [t("approvals.more_details")]),
      ]),
    ]));
    const composer = composerView({ sessionId: view.id, isNew: false, disabled: view.in_flight || inFlight.has(view.id), t });
    main.append(
      el("header", { class: "session-heading" }, [
        el("div", {}, [el("h1", { class: "page-title" }, [view.title || t("sessions.new")]), el("p", { class: "muted" }, [t("sessions.chat_helper")])]),
        el("div", { class: "heading-actions" }, [el("a", { class: "secondary-button button-link", href: `#/trace/${view.root_id}` }, [t("sessions.history")]), el("button", { class: "secondary-button", onclick: () => endSession(view.id, t) }, [t("sessions.end")])]),
      ]),
      banner,
      el("div", { class: "conversation", role: "log", "aria-label": t("sessions.messages"), "aria-live": "polite" }, [
        messages.next_before ? el("button", { class: "secondary-button load-older", onclick: () => loadOlder(selected.id, messages.next_before, t) }, [t("sessions.load_older")]) : el("span", {}, []),
        ...messageItems, ...inlineApprovals,
      ]),
      requestError ? el("p", { class: "inline-error", role: "alert" }, [requestError]) : el("span", {}, []),
      composer,
    );
    const team = el("aside", { class: "page-aside" }, [
      el("h2", { class: "section-title" }, [t("sessions.who_working")]),
      el("p", { class: "muted" }, [t("sessions.team_helper")]),
      el("article", { class: "wall-card" }, [el("strong", {}, [t("sessions.lead")]), el("span", { class: `status-line tone-${view.status_words?.tone || "neutral"}` }, [statusText(view, t)]), el("p", { class: "muted" }, [t("sessions.lead_note")])]),
      ...(detail.team || []).map(member => el("a", { class: "wall-card member-wall", href: `#/members/${member.id}` }, [
        el("strong", {}, [member.role || t("sessions.member")]), el("span", { class: `status-line tone-${member.status_words?.tone || "neutral"}` }, [statusText(member, t)]),
        el("p", { class: "muted" }, [scopeSummary(member.scope)]),
      ])),
      el("h3", { class: "section-title activity-heading" }, [t("sessions.latest_activity")]),
      el("p", { class: "muted" }, [t("sessions.activity_hint")]),
      ...(activity.events || []).slice(0, 5).map(item => el("article", { class: "activity-item" }, [
        el("strong", {}, [item.observation ? t("sessions.observation") : item.type]),
        el("span", { class: "muted" }, [`${item.agent_role || t("sessions.team")} · ${timeText(item.at)}`]),
        el("p", {}, [activityText(item.data)]),
      ])),
      el("a", { href: `#/trace/${view.root_id}` }, [t("sessions.all_events")]),
    ]);
    root.append(sidebar, main, team);
  } else {
    main.append(
      el("header", { class: "session-heading" }, [el("div", {}, [el("h1", { class: "page-title" }, [t("sessions.new_title")]), el("p", { class: "muted" }, [t("sessions.chat_helper")])])]),
      el("div", { class: "empty-state" }, [t(sessions.length ? "sessions.pick_or_new" : "sessions.first_message")]),
      requestError ? el("p", { class: "inline-error", role: "alert" }, [requestError]) : el("span", {}, []),
      composerView({ sessionId: null, isNew: true, disabled: inFlight.has("new"), t }),
    );
    root.append(sidebar, main);
  }
}

function composerView({ sessionId, isNew, disabled, t }) {
  const form = el("form", { class: "composer", onsubmit: async event => {
    event.preventDefault();
    const textarea = form.querySelector("textarea");
    const text = textarea.value.trim();
    if (!text) return;
    const lock = sessionId || "new";
    inFlight.add(lock); requestError = "";
    const button = form.querySelector("button[type=submit]"); button.disabled = true;
    let accepted = false;
    try {
      const result = isNew ? await api("/sessions", { method: "POST", body: JSON.stringify({ text }) }) : await api(`/sessions/${sessionId}/messages`, { method: "POST", body: JSON.stringify({ text }) });
      if (completedRequests.delete(result.request_id)) inFlight.delete(lock);
      else pendingLocks.set(result.request_id, lock);
      accepted = true;
      textarea.value = "";
      if (result.session_id) location.hash = `#/sessions/${result.session_id}`;
      await renderSessions(document.getElementById("app"), result.session_id || selectedFromLocation() || sessionId, t);
    } catch (error) { inFlight.delete(lock); requestError = error.code === "session_busy" ? t("sessions.busy") : error.message; }
    finally { if (!accepted) inFlight.delete(lock); await renderSessions(document.getElementById("app"), selectedFromLocation() || sessionId, t); }
  } }, [
    el("label", { class: "label", for: "message-input" }, [t("sessions.message_label")]),
    el("textarea", { id: "message-input", rows: 2, placeholder: t("sessions.placeholder"), maxlength: "8000", disabled, value: composerPrefill }),
    el("div", { class: "composer-row" }, [el("span", { class: "muted" }, [sessionId ? t("sessions.replies_here") : t("sessions.no_mirror")]), el("button", { class: "primary-button", type: "submit", disabled }, [t("sessions.send")])]),
    disabled ? el("p", { class: "muted" }, [t("sessions.still_answering")]) : el("span", {}, []),
  ]);
  composerPrefill = "";
  return form;
}

export function prefillComposer(text) { composerPrefill = text; }

function scopeSummary(scope) {
  const mounts = scope?.mounts || [];
  if (!mounts.length) return "No additional mounted paths.";
  return mounts.map(mount => `${mount.mode === "rw" ? "Edits" : "Reads"} ${mount.container_path || mount.path || "/"}`).join(", ");
}

function activityText(data) {
  const value = data?.text || data?.detail || data?.command || data?.reason || "";
  return typeof value === "string" ? value.slice(0, 240) : JSON.stringify(value).slice(0, 240);
}

function selectedFromLocation() { return location.hash.match(/^#\/sessions\/([^/]+)/)?.[1] || null; }

async function loadOlder(sessionId, cursor, t) {
  try {
    const query = new URLSearchParams({ limit: "100", before: cursor });
    const page = await api(`/sessions/${sessionId}/messages?${query}`);
    const previous = olderPages.get(sessionId)?.items || [];
    olderPages.set(sessionId, { items: [...previous, ...(page.items || [])], nextBefore: page.next_before });
  } catch (error) { requestError = error.message; }
  await renderSessions(document.getElementById("app"), sessionId, t);
}

async function decide(code, decision, t) {
  try { await api(`/approvals/${code}/${decision}`, { method: "POST", body: "{}" }); }
  catch (error) { requestError = error.message; }
  const route=location.hash.match(/^#\/sessions(?:\/([^/]+))?/);
  await renderSessions(document.getElementById("app"),route?.[1]||null,t);
}

async function endSession(id, t) {
  if (!confirm(t("sessions.confirm_end"))) return;
  try { await api(`/sessions/${id}/end`, { method: "POST", body: "{}" }); }
  catch (error) { requestError = error.message; }
  await renderSessions(document.getElementById("app"),id,t);
}

export async function handleConsoleEvent(type, raw, render) {
  let event;
  try { event=JSON.parse(raw); } catch { return; }
  if (type === "supervisor" && event.event?.type !== "session.closed" && !String(event.event?.type || "").startsWith("approval.")) return;
  if (["chat.reply","chat.error"].includes(type)) {
    const lock=pendingLocks.get(event.request_id);
    if (lock) { inFlight.delete(lock); pendingLocks.delete(event.request_id); }
    else completedRequests.add(event.request_id);
  }
  if (["chat.pending","chat.reply"].includes(type) && event.session_id && location.hash === "#/sessions") location.hash=`#/sessions/${event.session_id}`;
  await render();
}
