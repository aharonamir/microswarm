import { api } from "../api.js";
import { el } from "../dom.js";

const statusLabel = (session, t) => t(session.status_words?.key || "") || session.status_words?.en || t("status.unknown");

export async function renderOverview(root, t) {
  let sessions, approvals;
  try {
    [sessions, approvals] = await Promise.all([api("/sessions?group=active"), api("/approvals?state=pending")]);
  } catch (error) {
    root.append(el("section", { class: "panel overview-error", role: "alert" }, [error.message]));
    return;
  }
  const active = sessions.sessions || [];
  const pending = approvals.approvals || [];
  const activities = await Promise.all(active.slice(0, 8).map(async session => {
    try {
      const result = await api(`/sessions/${session.id}/activity`);
      return (result.events || []).map(event => ({ ...event, session_title: session.title || session.id }));
    } catch { return []; }
  }));
  const events = activities.flat().sort((a, b) => String(b.at || "").localeCompare(String(a.at || ""))).slice(0, 8);
  root.append(
    el("header", { class: "overview-heading" }, [el("div", {}, [el("p", { class: "label" }, [t("console.eyebrow")]), el("h1", { class: "page-title" }, [t("overview.title")]), el("p", { class: "muted" }, [t("overview.subtitle")])])]),
    el("section", { class: "overview-attention panel", "aria-labelledby": "overview-attention-title" }, [
      el("div", { class: "overview-section-heading" }, [el("h2", { id: "overview-attention-title", class: "section-title" }, [t("overview.attention")]), el("span", { class: pending.length ? "attention-count" : "ready-count" }, [pending.length ? String(pending.length) : t("common.ready")])]),
      pending.length ? el("div", { class: "overview-approval-list" }, pending.map(item => el("a", { class: "overview-approval", href: `#/approvals/${item.code}` }, [el("strong", {}, [`${item.requesting_role || t("approvals.member")} · ${item.tool_call?.tool || item.kind}`]), el("span", { class: "muted" }, [item.code]), el("span", {}, [t("overview.review_request")])])) ) : el("p", { class: "muted" }, [t("overview.no_attention")]),
    ]),
    el("section", { class: "overview-sessions panel", "aria-labelledby": "overview-sessions-title" }, [el("div", { class: "overview-section-heading" }, [el("h2", { id: "overview-sessions-title", class: "section-title" }, [t("overview.active_sessions")]), el("a", { href: "#/sessions" }, [t("overview.all_sessions")])]), active.length ? el("div", { class: "overview-session-grid" }, active.map(session => el("a", { class: "overview-session-card", href: `#/sessions/${session.id}` }, [el("strong", {}, [session.title || t("sessions.new")]), el("span", { class: "status-line" }, [el("span", { class: "status-dot" }), statusLabel(session, t)]), el("span", { class: "session-meta" }, [`${session.member_count || 0} ${t("sessions.members")} · ${session.channel || "web"}`])])) ) : el("p", { class: "muted" }, [t("overview.no_sessions")]),
    ]),
    el("section", { class: "overview-activity panel", "aria-labelledby": "overview-activity-title" }, [el("div", { class: "overview-section-heading" }, [el("h2", { id: "overview-activity-title", class: "section-title" }, [t("overview.recent_activity")]), el("a", { href: "#/trace" }, [t("sessions.all_events")])]), events.length ? el("ol", { class: "overview-event-list" }, events.map(event => el("li", {}, [el("span", { class: "mono" }, [event.at || ""]), el("span", {}, [event.observation ? t("sessions.observation") : event.type || t("overview.event")]), el("a", { href: `#/sessions/${event.session_id}` }, [event.session_title])])) ) : el("p", { class: "muted" }, [t("overview.no_activity")]),
    ]),
  );
}
