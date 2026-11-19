import { el, clear } from "../dom.js";
import { api } from "../api.js";

export async function renderNav(root, { page, parts, pending, t, focusHref }) {
  clear(root);
  const links = [
    ["sessions", "nav.new_session", "#/sessions", "＋"],
    ["home", "nav.home", "#/home", "⌂"],
    ["approvals", "nav.approvals", "#/approvals", "✓"],
    ["trajectories", "nav.trajectories", "#/trajectories", "↗"],
    ["host", "nav.host", "#/host", "⚙"],
  ];
  for (const [route, key, href, icon] of links) {
    const active = route === page || (route === "trajectories" && page === "trajectory");
    root.append(el("a", { href, "data-route": route, "aria-label": t(key), "aria-current": active ? "page" : false, class: route === "sessions" ? "new-session-link" : "" }, [
      el("span", { class: "nav-icon", "aria-hidden": "true" }, [icon]),
      el("span", { class: "nav-label" }, [t(key)]),
      ...(route === "approvals" ? [el("span", { class: "badge", hidden: pending < 1 }, [String(pending)])] : []),
    ]));
  }
  let active = [];
  let ended = [];
  try {
    [active, ended] = await Promise.all([
      api("/sessions?group=active").then(body => body.sessions || []),
      api("/sessions?group=ended").then(body => body.sessions || []),
    ]);
  } catch { /* Keep navigation available when the supervisor is offline. */ }
  root.append(el("h2", { class: "nav-section-title" }, [t("nav.sessions")]));
  root.append(el("div", { class: "session-list" }, active.map(sessionLink)));
  if (ended.length) {
    root.append(el("h2", { class: "nav-section-title" }, [t("sessions.ended_today")]));
    root.append(el("div", { class: "session-list" }, ended.map(sessionLink)));
  }
  if (focusHref) [...root.querySelectorAll("a")].find(link => link.getAttribute("href") === focusHref)?.focus({ preventScroll: true });

  function sessionLink(session) {
    const href = `#/sessions/${session.id}`;
    const isCurrent = ["sessions", "member", "trajectory"].includes(page) && parts[0] === session.id;
    const status = session.status_words || {};
    return el("a", { class: "session-link", href, "aria-current": isCurrent ? "page" : false }, [
      el("span", { class: "session-link-title" }, [session.title || t("sessions.untitled")]),
      el("span", { class: `status-line tone-${status.tone || "neutral"}` }, [el("span", { class: "status-dot", "aria-hidden": "true" }), t(status.key || "status.unknown", status.args || {})]),
    ]);
  }
}
