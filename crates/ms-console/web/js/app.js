import { append, el, clear, setAttr, setProp, toggleClass } from "./dom.js";
import { renderNav } from "./components/nav.js";
import { currentRoute, onRoute } from "./router.js";
import { loadLocale, t } from "./i18n.js";
import { connectEvents } from "./sse.js";
import { api } from "./api.js";
import { renderSessions, handleConsoleEvent } from "./pages/sessions.js";
import { renderApprovals } from "./pages/approvals.js";
import { renderTrace } from "./pages/trace.js";
import { renderHost } from "./pages/host.js";
import { captureMemberTab, renderMember } from "./pages/member.js";
import { renderOverview } from "./pages/overview.js";
import { captureSessionUi, handleSessionChatEvent, renderSession } from "./pages/session.js";
import { handleSessionLiveEvent } from "./live.js";

const app = document.getElementById("app");
const nav = document.querySelector(".primary-nav");
let reconnecting = false;
let supervisorStatus = "checking";
let renderPending = false;
let renderTask = null;

function render() {
  renderPending = true;
  if (!renderTask) {
    renderTask = (async () => {
      while (renderPending) {
        renderPending = false;
        await renderNow();
      }
    })().finally(() => {
      renderTask = null;
      if (renderPending) render();
    });
  }
  return renderTask;
}

async function renderNow() {
  await loadLocale(localStorage.getItem("ms-lang") || "en");
  const menuButton = document.getElementById("mobile-menu-toggle");
  const menuButtonLabel = document.getElementById("mobile-menu-toggle-label");
  clear(menuButtonLabel);
  append(menuButtonLabel, t("nav.menu"));
  setAttr(menuButton, "aria-label", t("nav.menu"));
  const route = currentRoute();
  const { page, parts = [] } = route;
  const isSessionDetail = (page === "sessions" && parts.length > 0) || page === "member";
  const sessionSnapshot = isSessionDetail ? captureSessionUi(app) : null;
  toggleClass(app, "session-index", page === "sessions" && parts.length === 0);
  toggleClass(document.querySelector(".console-shell"), "member-drawer-open", page === "member" && parts.length > 1);
  toggleClass(app, "session-workspace", isSessionDetail);
  const composer = page === "sessions" ? document.getElementById("message-input") : null;
  const composerDraft = composer?.value;
  const composerRoute = location.hash;
  let pending = 0;
  try { pending = Number((await api("/overview")).pending_approvals || 0); } catch { /* The page view reports its own connection error. */ }
  const focusedNavHref = nav.contains(document.activeElement)
    ? document.activeElement.getAttribute("href")
    : null;
  await renderNav(nav, { ...route, pending, t, focusHref: focusedNavHref });
  const supervisorChip = document.querySelector(".supervisor-chip");
  clear(supervisorChip);
  append(supervisorChip, el("span", { class: "status-dot" }), t(`header.supervisor_${supervisorStatus}`));
  clear(app);
  if (reconnecting) append(app, el("p", { class: "reconnecting-banner", role: "status", "aria-live": "polite" }, [t("common.reconnecting")]));
  if (page === "home") { await renderOverview(app, t); return; }
  if (page === "sessions") {
    const sessionId = parts[0] || null;
    if (sessionId) {
      await renderSession(app, sessionId, t, { snapshot: sessionSnapshot });
      return;
    }
    await renderSessions(app, null, t);
    if (composerDraft !== undefined && composerRoute === location.hash) {
      const nextComposer = document.getElementById("message-input");
      if (nextComposer) {
        nextComposer.value = composerDraft;
        nextComposer.dispatchEvent(new Event("input", { bubbles: true }));
      }
    }
    return;
  }
  if (page === "approvals") {
    await renderApprovals(app, currentRoute().parts[0] || null, t);
    return;
  }
  if (page === "trajectory" || page === "trajectories") {
    await renderTrace(app, route.parts[0] || null, t);
    return;
  }
  if (page === "host") {
    await renderHost(app, t);
    return;
  }
  if (page === "member" && route.parts[1]) {
    await renderSession(app, route.parts[0], t, { snapshot: sessionSnapshot });
    await renderMember(app, route.parts[0], route.parts[1], t);
    return;
  }
  if (page === "member" && route.parts[0]) {
    try {
      const legacy = await api(`/members/${route.parts[0]}/scope`);
      location.replace(`#/sessions/${legacy.lead_id}/members/${route.parts[0]}`);
    } catch (error) {
      append(app, el("section", { class: "page-main panel", role: "alert" }, [error.message]));
    }
    return;
  }
  append(app, el("section", { class: "page-main panel" }, [
    el("p", { class: "label" }, [t("console.eyebrow")]),
    el("h1", { class: "page-title" }, [t(`nav.${page}`)]),
    el("p", { class: "muted" }, [t("console.loading")]),
  ]));
}

const mobileMenu = document.getElementById("mobile-menu-toggle");
function syncMobileMenu() {
  const mobile = matchMedia("(max-width: 699px)").matches;
  setProp(nav, "hidden", mobile);
  setAttr(mobileMenu, "aria-expanded", String(!mobile));
}
const mobileQuery = matchMedia("(max-width: 699px)");
mobileQuery.addEventListener("change", syncMobileMenu);
syncMobileMenu();
mobileMenu.addEventListener("click", event => {
  const expanded = event.currentTarget.getAttribute("aria-expanded") === "true";
  setAttr(event.currentTarget, "aria-expanded", String(!expanded));
  setProp(nav, "hidden", expanded);
});
document.addEventListener("click", event => {
  if (!matchMedia("(max-width: 699px)").matches || nav.hidden) return;
  const target = event.target;
  if (nav.contains(target)) {
    if (target.closest("a")) {
      setProp(nav, "hidden", true);
      setAttr(mobileMenu, "aria-expanded", "false");
    }
    return;
  }
  if (!mobileMenu.contains(target)) {
    setProp(nav, "hidden", true);
    setAttr(mobileMenu, "aria-expanded", "false");
  }
});

document.getElementById("language-toggle").addEventListener("click", async event => {
  const next = localStorage.getItem("ms-lang") === "zh" ? "en" : "zh";
  localStorage.setItem("ms-lang", next);
  setAttr(event.currentTarget, "aria-pressed", String(next === "zh"));
  await render();
});
onRoute(render);
connectEvents((type, data) => {
  if (type === "console.reconnecting" || type === "supervisor.down") { reconnecting = true; if (type === "supervisor.down") supervisorStatus = "down"; render(); }
  else if (type === "console.connected") { if (reconnecting) { reconnecting = false; render(); } }
  else if (type === "supervisor.up") { reconnecting = false; supervisorStatus = "running"; render(); }
  else if (type === "resync") render();
  else if (type === "agent" || type === "supervisor") {
    const route = currentRoute();
    const sessionId = route.page === "sessions" ? route.parts[0] : route.page === "member" ? route.parts[0] : null;
    const memberId = route.page === "member" ? route.parts[1] : null;
    const refreshMember = memberId ? async snapshot => {
      const selectedTab = captureMemberTab(app);
      await renderSession(app, sessionId, t, { live: true, snapshot });
      await renderMember(app, sessionId, memberId, t, { selectedTab });
    } : null;
    if (sessionId && handleSessionLiveEvent(type, data, sessionId, app, t, refreshMember)) return;
    if (type === "supervisor") handleConsoleEvent(type, data, render, t);
  }
  else if (["chat.pending", "chat.reply", "chat.error"].includes(type)) {
    const route = currentRoute();
    const sessionId = route.page === "sessions" ? route.parts[0] : null;
    if (sessionId) handleSessionChatEvent(type, data, render, sessionId);
    else handleConsoleEvent(type, data, render, t);
  }
  else if (type === "status.changed") handleConsoleEvent(type, data, render, t);
});
