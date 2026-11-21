import { el, clear, svg } from "../dom.js";
import { api } from "../api.js";

// Path data selected from Tabler Icons' outline set; see THIRD_PARTY_NOTICES.md.
const navIconPaths = {
  back: ["M5 12l14 0", "M5 12l6 6", "M5 12l6 -6"],
  sessions: ["M12 5l0 14", "M5 12l14 0"],
  home: ["M5 12l-2 0l9 -9l9 9l-2 0", "M5 12v7a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-7", "M9 21v-6a2 2 0 0 1 2 -2h2a2 2 0 0 1 2 2v6"],
  approvals: ["M9 5h-2a2 2 0 0 0 -2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-12a2 2 0 0 0 -2 -2h-2", "M9 5a2 2 0 0 1 2 -2h2a2 2 0 0 1 2 2a2 2 0 0 1 -2 2h-2a2 2 0 0 1 -2 -2", "M9 14l2 2l4 -4"],
  trajectories: ["M3 19a2 2 0 1 0 4 0a2 2 0 0 0 -4 0", "M19 7a2 2 0 1 0 0 -4a2 2 0 0 0 0 4", "M11 19h5.5a3.5 3.5 0 0 0 0 -7h-8a3.5 3.5 0 0 1 0 -7h4.5"],
  host: ["M10.325 4.317c.426 -1.756 2.924 -1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543 -.94 3.31 .826 2.37 2.37a1.724 1.724 0 0 0 1.065 2.572c1.756 .426 1.756 2.924 0 3.35a1.724 1.724 0 0 0 -1.066 2.573c.94 1.543 -.826 3.31 -2.37 2.37a1.724 1.724 0 0 0 -2.572 1.065c-.426 1.756 -2.924 1.756 -3.35 0a1.724 1.724 0 0 0 -2.573 -1.066c-1.543 .94 -3.31 -.826 -2.37 -2.37a1.724 1.724 0 0 0 -1.065 -2.572c-1.756 -.426 -1.756 -2.924 0 -3.35a1.724 1.724 0 0 0 1.066 -2.573c-.94 -1.543 .826 -3.31 2.37 -2.37c1 .608 2.296 .07 2.572 -1.065", "M9 12a3 3 0 1 0 6 0a3 3 0 0 0 -6 0"],
};

export function navIcon(name) {
  return svg("svg", {
    class: "nav-icon", "aria-hidden": "true", focusable: "false", viewBox: "0 0 24 24",
    fill: "none", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round",
  }, navIconPaths[name].map(d => svg("path", { d })));
}

export async function renderNav(root, { page, parts, pending, t, focusHref }) {
  clear(root);
  const links = [
    ["sessions", "nav.new_session", "#/sessions"],
    ["home", "nav.home", "#/home"],
    ["approvals", "nav.approvals", "#/approvals"],
    ["trajectories", "nav.trajectories", "#/trajectories"],
    ["host", "nav.host", "#/host"],
  ];
  for (const [route, key, href] of links) {
    const active = route === page || (route === "trajectories" && page === "trajectory");
    root.append(el("a", { href, "data-route": route, "aria-current": active ? "page" : false, class: route === "sessions" ? "new-session-link" : "" }, [
      navIcon(route),
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
