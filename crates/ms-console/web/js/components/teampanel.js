import { el } from "../dom.js";
import { sentenceNodes } from "./step.js";
import { avatar } from "./avatar.js";
import { statusPill } from "./pill.js";
import { laneStrip } from "./lanes.js";

export function teamPanel(agents, groups, timeline, sessionId, t) {
  return el("aside", { class: "session-team-sidebar", "aria-label": t("team.panel_title") }, [
    el("header", {}, [el("h2", { class: "section-title" }, [t("team.panel_title")]), el("p", { class: "muted" }, [t("team.panel_helper")])]),
    laneStrip(timeline, t),
    el("div", { class: "team-agent-cards" }, agents.map(agent => {
      const member = findMember(groups, agent.id) || {};
      return el("article", { class: `team-agent-card${member.state === "waiting_you" ? " team-agent-waiting" : ""}` }, [
        el("div", { class: "team-agent-title" }, [avatar(agent), agent.kind === "lead" ? el("strong", {}, [agent.role]) : el("a", { href: `#/sessions/${sessionId}/members/${agent.id}`, "data-member-open": agent.id }, [agent.role]), statusPill(member.status || { key: statusKey(member.state || (agent.kind === "lead" ? "working" : "unknown")) }, t)]),
        member.current_step ? el("p", { class: "muted" }, [t("team.current_step"), ": ", ...sentenceNodes(member.current_step, t)]) : el("span", {}, []),
        el("p", { class: "team-reach-summary" }, [reachSummary(member.reach_summary || {}, t)]),
        Number(member.reduced_count) > 0 ? el("span", { class: "pill pill-reduced" }, [t("team.reduced_count", { count: Number(member.reduced_count) })]) : el("span", {}, []),
      ]);
    })),
    el("p", { class: "team-panel-footnote" }, [t("team.panel_footnote")]),
  ]);
}

function findMember(groups, id) {
  for (const group of groups || []) {
    const found = (group.members || []).find(member => member.agent?.id === id);
    if (found) return found;
  }
  return null;
}

function statusKey(state) {
  return `status.${state === "waiting_you" ? "waiting_for_you" : state}`;
}

function reachSummary(reach, t) {
  const folders = (reach.folders || []).map(folder => `${folder.path} ${folder.mode === "rw" ? t("reach.read_write") : t("reach.read_only")}`);
  const network = reach.network === "none" || !reach.network ? t("reach.no_network") : `${t("reach.network")}: ${String(reach.network).replaceAll("_", " ")}`;
  const credentials = reach.credentials?.length ? `${reach.credentials.length} ${t("reach.credentials")}` : t("reach.no_credentials");
  return [...folders, network, credentials].join(" · ");
}
