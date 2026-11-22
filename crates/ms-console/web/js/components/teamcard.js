import { api } from "../api.js";
import { append, clear, el, setAttr, setProp } from "../dom.js";
import { duration } from "../format.js";
import { sentenceNodes, stepRow } from "./step.js";
import { avatar } from "./avatar.js";
import { statusPill } from "./pill.js";

export function teamCard(group, sessionId, t) {
  const members = group.members || [];
  const rows = members.slice(0, 4).map(member => memberRow(member, group, sessionId, t));
  const more = Number(group.more || Math.max(0, members.length - rows.length));
  const seconds = Math.max(0, Number(group.ended_at ? Date.parse(group.ended_at) - Date.parse(group.started_at) : Date.now() - Date.parse(group.started_at)));
  const timeline = el("div", { class: "team-card-timeline" }, [
    el("span", { class: "team-branch", "aria-hidden": "true" }, []),
    el("div", {}, [
      el("header", { class: "team-card-heading" }, [
        el("strong", {}, [t("team.title", { count: members.length + more })]),
        el("span", { class: "muted" }, [t("team.running_for", { duration: duration(seconds) })]),
        el("a", { href: `#/sessions/${sessionId}/trajectory` }, [t("team.open_trajectory")]),
      ]),
      el("ul", { class: "team-member-list" }, rows),
      more > 0 ? el("p", { class: "muted team-more" }, [t("team.more_members", { count: more })]) : el("span", {}, []),
    ]),
  ]);
  return el("section", { class: "team-card", "aria-label": t("team.card"), "data-team-group": group.group_id || "" }, [timeline]);
}

function memberRow(member, group, sessionId, t) {
  const agent = member.agent || {};
  const role = agent.role || t("sessions.member");
  const waiting = ["waiting_you", "denied"].includes(member.state);
  const buttonId = `member-steps-${String(agent.id || role).replace(/[^a-z0-9_-]/gi, "-")}`;
  const row = el("li", { class: `team-member-row${waiting ? " team-member-waiting" : ""}`, "data-member-id": agent.id || "" }, [
    el("div", { class: "team-member-topline" }, [
      avatar(agent),
      el("a", { class: "team-member-role", href: `#/sessions/${sessionId}/members/${agent.id}`, "data-member-open": agent.id || "" }, [role]),
      statusPill(member.status || { key: `status.${member.state === "waiting_you" ? "waiting_for_you" : member.state || "unknown"}`, tone: waiting ? "attention" : "neutral" }, t),
      el("span", { class: "team-step-count" }, [t("team.step_count", { count: Number(member.step_count || 0) })]),
      el("span", { class: "team-elapsed mono" }, [duration(member.elapsed_ms || 0)]),
    ]),
    el("p", { class: "team-task" }, [String(member.task || "").slice(0, 160) + (member.task_truncated ? "…" : "")]),
    member.current_step ? el("p", { class: "team-current-step" }, sentenceNodes(member.current_step, t)) : el("span", {}, []),
    Number(member.reduced_count) > 0 ? el("span", { class: "pill pill-reduced" }, [t("team.reduced_count", { count: Number(member.reduced_count) })]) : el("span", {}, []),
    waiting && member.pending_approval ? el("p", { class: "team-waiting-note" }, sentenceNodes(member.pending_approval.summary || {}, t)) : el("span", {}, []),
    Number(member.step_count) > 0 ? el("button", {
      id: buttonId, class: "team-expand", type: "button", "aria-expanded": "false", "aria-label": t("team.show_steps_for", { role }),
      onclick: async event => toggleSteps(event.currentTarget, member, group, sessionId, t),
    }, [t("team.show_steps")]) : el("span", {}, []),
    el("div", { class: "team-expanded-steps", id: `${buttonId}-content`, hidden: true }, []),
  ]);
  return row;
}

async function toggleSteps(button, member, group, sessionId, t) {
  const target = button.parentElement.querySelector(".team-expanded-steps");
  const expanded = button.getAttribute("aria-expanded") === "true";
  setAttr(button, "aria-expanded", String(!expanded));
  setProp(target, "hidden", expanded);
  if (expanded) { clear(button); append(button, t("team.show_steps")); return; }
  if (target.dataset.loaded === "true") { clear(button); append(button, t("team.hide_steps")); return; }
  setProp(button, "disabled", true);
  try {
    const data = await api(`/sessions/${sessionId}/steps?agent=${encodeURIComponent(member.agent.id)}&limit=20`);
    const visible = data.steps || [];
    clear(target);
    append(target, el("ol", { class: "step-list" }, visible.map(step => stepRow(step, t, { compact: true }))));
    if (Number(member.step_count) > 20) append(target, el("a", { href: `#/sessions/${sessionId}/trajectory?agent=${encodeURIComponent(member.agent.id)}` }, [t("team.show_all_steps", { count: Number(member.step_count) })]));
    setAttr(target, "data-loaded", "true");
    clear(button); append(button, t("team.hide_steps"));
  } catch (error) {
    clear(target); append(target, el("p", { class: "inline-error", role: "alert" }, [error.message]));
    clear(button); append(button, t("team.show_steps"));
    setAttr(button, "aria-expanded", "false");
    setProp(target, "hidden", true);
  } finally { setProp(button, "disabled", false); }
}
