import { api } from "../api.js";
import { el, clear } from "../dom.js";

let filter = "all";
let selectedKey = null;
export async function renderTrace(root, sessionId, t) {
  clear(root);
  if (!sessionId) { root.append(el("section", { class: "panel page-main" }, [el("h1", { class: "page-title" }, [t("nav.trace")]), el("p", { class: "muted" }, [t("trace.choose_session")]), el("a", { href: "#/sessions" }, [t("nav.sessions")])])); return; }
  let data;
  try { data = await api(`/sessions/${sessionId}/trace`); }
  catch (error) { root.append(el("section", { class: "panel page-main" }, [el("p", { role: "alert" }, [error.message])])); return; }
  const events = data.events || [];
  const filtered = events.filter(event => filter === "all" || (filter === "tools" && /tool|command/.test(event.type)) || (filter === "decisions" && /approval|policy|decision/.test(event.type)) || (filter === "delegation" && /delegat|member/.test(event.type)) || (filter === "exits" && /exit|closed|crash/.test(event.type)));
  const keyOf = event => `${event.source || ""}:${event.session_id || ""}:${event.seq || ""}`;
  const chosen = filtered.find(event => keyOf(event) === selectedKey) || filtered[filtered.length - 1];
  root.append(el("header", { class: "trace-header" }, [el("div", {}, [el("h1", { class: "page-title" }, [t("nav.trace")]), el("p", { class: "muted" }, [sessionId])]),
    el("button", { class: "secondary-button", onclick: async () => { await navigator.clipboard.writeText(JSON.stringify(chosen || {}, null, 2)); } }, [t("trace.copy")]),
    el("button", { class: "secondary-button", onclick: () => exportJson(data) }, [t("trace.export")]),
  ]), el("nav", { class: "trace-filters", "aria-label": t("trace.filters") }, ["all", "tools", "decisions", "delegation", "exits"].map(value => el("button", { class: value === filter ? "tab active" : "tab", "aria-pressed": String(value === filter), onclick: () => { filter = value; renderTrace(root, sessionId, t); } }, [t(`trace.${value}`)]))),
  el("div", { class: "trace-grid" }, [
    el("section", { class: "panel trace-list", "aria-label": t("trace.events") }, filtered.map(event => el("button", { class: "trace-row", "aria-current": keyOf(event) === keyOf(chosen || {}) ? "true" : false, onclick: () => { selectedKey = keyOf(event); renderTrace(root, sessionId, t); } }, [el("strong", {}, [event.type || "event"]), el("span", { class: "muted" }, [`${event.agent_role || event.source || ""} · ${event.ts || ""}`])]))),
    el("pre", { class: "command-block mono trace-json", "aria-label": t("trace.selected_json") }, [chosen ? JSON.stringify(chosen, null, 2) : t("trace.no_events")]),
  ]));
}
function exportJson(data) { const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" })); const link = el("a", { href: url, download: "microswarm-trace.json" }, []); document.body.append(link); link.click(); link.remove(); URL.revokeObjectURL(url); }
