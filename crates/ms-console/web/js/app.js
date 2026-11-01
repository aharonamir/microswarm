import { el, clear } from "./dom.js";
import { currentRoute, onRoute } from "./router.js";
import { loadLocale, t } from "./i18n.js";
import { connectEvents } from "./sse.js";
import { api } from "./api.js";
import { renderSessions, handleConsoleEvent } from "./pages/sessions.js";
import { renderApprovals } from "./pages/approvals.js";
import { renderTrace } from "./pages/trace.js";
import { renderHost } from "./pages/host.js";
import { renderMember } from "./pages/member.js";

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
  const { page } = currentRoute();
  let pending = 0;
  try { pending = Number((await api("/overview")).pending_approvals || 0); } catch { /* The page view reports its own connection error. */ }
  clear(nav);
  for (const route of ["sessions", "approvals", "trace", "host"]) nav.append(el("a", { href: `#/${route}`, "data-route": route, "aria-current": route === page ? "page" : false }, [
    t(`nav.${route}`),
    ...(route === "approvals" ? [el("span", { class: "badge", hidden: pending < 1 }, [String(pending)])] : []),
  ]));
  const supervisorChip = document.querySelector(".supervisor-chip");
  clear(supervisorChip);
  supervisorChip.append(el("span", { class: "status-dot" }), t(`header.supervisor_${supervisorStatus}`));
  clear(app);
  if (reconnecting) app.append(el("p", { class: "reconnecting-banner", role: "status", "aria-live": "polite" }, [t("common.reconnecting")]));
  if (page === "sessions") {
    await renderSessions(app, currentRoute().parts[0] || null, t);
    return;
  }
  if (page === "approvals") {
    await renderApprovals(app, currentRoute().parts[0] || null, t);
    return;
  }
  if (page === "trace") {
    await renderTrace(app, currentRoute().parts[0] || null, t);
    return;
  }
  if (page === "host") {
    await renderHost(app, t);
    return;
  }
  if (page === "members" && currentRoute().parts[0]) {
    await renderMember(app, currentRoute().parts[0], t);
    return;
  }
  app.append(el("section", { class: "page-main panel" }, [
    el("p", { class: "label" }, [t("console.eyebrow")]),
    el("h1", { class: "page-title" }, [t(`nav.${page}`)]),
    el("p", { class: "muted" }, [t("console.loading")]),
  ]));
}

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
  else if (["chat.pending", "chat.reply", "chat.error", "status.changed", "supervisor"].includes(type)) handleConsoleEvent(type, data, render);
});
