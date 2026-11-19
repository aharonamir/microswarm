import { el, clear } from "./dom.js";
import { renderNav } from "./components/nav.js";
import { currentRoute, onRoute } from "./router.js";
import { loadLocale, t } from "./i18n.js";
import { connectEvents } from "./sse.js";
import { api } from "./api.js";
import { renderSessions, handleConsoleEvent } from "./pages/sessions.js";
import { renderApprovals } from "./pages/approvals.js";
import { renderTrace } from "./pages/trace.js";
import { renderHost } from "./pages/host.js";
import { renderMember } from "./pages/member.js";
import { renderOverview } from "./pages/overview.js";

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
  clear(menuButton);
  menuButton.append(t("nav.menu"));
  menuButton.setAttribute("aria-label", t("nav.menu"));
  const route = currentRoute();
  const { page } = route;
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
  supervisorChip.append(el("span", { class: "status-dot" }), t(`header.supervisor_${supervisorStatus}`));
  clear(app);
  if (reconnecting) app.append(el("p", { class: "reconnecting-banner", role: "status", "aria-live": "polite" }, [t("common.reconnecting")]));
  if (page === "home") { await renderOverview(app, t); return; }
  if (page === "sessions") {
    await renderSessions(app, currentRoute().parts[0] || null, t);
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
    await renderMember(app, route.parts[1], t);
    return;
  }
  app.append(el("section", { class: "page-main panel" }, [
    el("p", { class: "label" }, [t("console.eyebrow")]),
    el("h1", { class: "page-title" }, [t(`nav.${page}`)]),
    el("p", { class: "muted" }, [t("console.loading")]),
  ]));
}

const mobileMenu = document.getElementById("mobile-menu-toggle");
function syncMobileMenu() {
  const mobile = matchMedia("(max-width: 699px)").matches;
  nav.hidden = mobile;
  mobileMenu.setAttribute("aria-expanded", String(!mobile));
}
const mobileQuery = matchMedia("(max-width: 699px)");
mobileQuery.addEventListener("change", syncMobileMenu);
syncMobileMenu();
mobileMenu.addEventListener("click", event => {
  const expanded = event.currentTarget.getAttribute("aria-expanded") === "true";
  event.currentTarget.setAttribute("aria-expanded", String(!expanded));
  nav.hidden = expanded;
});

document.getElementById("language-toggle").addEventListener("click", async event => {
  const next = localStorage.getItem("ms-lang") === "zh" ? "en" : "zh";
  localStorage.setItem("ms-lang", next);
  event.currentTarget.setAttribute("aria-pressed", String(next === "zh"));
  await render();
});
onRoute(render);
connectEvents((type, data) => {
  if (type === "console.reconnecting" || type === "supervisor.down") { reconnecting = true; if (type === "supervisor.down") supervisorStatus = "down"; render(); }
  else if (type === "console.connected") { if (reconnecting) { reconnecting = false; render(); } }
  else if (type === "supervisor.up") { reconnecting = false; supervisorStatus = "running"; render(); }
  else if (type === "resync") render();
  else if (["chat.pending", "chat.reply", "chat.error", "status.changed", "supervisor"].includes(type)) handleConsoleEvent(type, data, render, t);
});
