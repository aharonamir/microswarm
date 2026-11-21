import { api } from "../api.js";
import { append, clear, el, setAttr, toggleClass } from "../dom.js";
import { localTime, shortId } from "../format.js";
import { avatar } from "../components/avatar.js";
import { statusPill } from "../components/pill.js";
import { stepRow } from "../components/step.js";
import { tabs } from "../components/tabs.js";
import { drawer, openDrawer } from "../components/drawer.js";
import { focusMemberOnSession, prefillSessionMessage } from "./session.js";

const tabSelections = new Map();

export function captureMemberTab(root) {
  const panel = root.querySelector(".member-tabpanel");
  return panel?.id.replace("member-panel-", "") || null;
}

export async function renderMember(root, sessionId, memberId, t, { selectedTab: restoredTab = null } = {}) {
  let team, reach, report, steps;
  try {
    [team, reach, report, steps] = await Promise.all([
      api(`/sessions/${sessionId}/team`), api(`/members/${memberId}/reach`),
      api(`/members/${memberId}/report`), api(`/sessions/${sessionId}/steps?agent=${encodeURIComponent(memberId)}&limit=200`),
    ]);
  } catch (error) {
    append(root, el("p", { class: "member-error", role: "alert" }, [error.message]));
    return;
  }
  const member = (team.groups || []).flatMap(group => group.members || []).find(row => row.agent?.id === memberId) || {};
  const agent = member.agent || { id: memberId, role: t("sessions.member"), initial: "M", colour_index: 0 };
  const sourceRef = reach.raw?.granted?.source_ref || member.source_ref || member.skill_ref || "";
  const dialog = drawer();
  const closeButton = el("button", { class: "secondary-button member-drawer-close", type: "button", "aria-label": t("member.close"), autofocus: true, onclick: () => dialog.close() }, [t("common.close")]);
  const headingTag = matchMedia("(max-width: 1099px)").matches ? "h1" : "h2";
  const header = el("header", { class: "member-drawer-header" }, [
    avatar(agent),
    el("div", {}, [el(headingTag, { id: "member-title" }, [agent.role]), el("p", {}, [t("member.source_skill", { skill: sourceRef })])]),
    statusPill(member.status || { key: `status.${member.state === "waiting_you" ? "waiting_for_you" : member.state || "unknown"}` }, t),
    closeButton,
  ]);
  const task = el("section", { class: "member-task-box", id: "member-task" }, [el("strong", {}, [t("member.task_from_lead")]), el("p", {}, [member.task || ""])]);
  const tabItems = [["steps", "member.tab_steps", { count: (steps.steps || []).length }], ["reach", "member.tab_reach"], ["report", "member.tab_report"]];
  const selectionKey = `${sessionId}/${memberId}`;
  let selectedTab = restoredTab || tabSelections.get(selectionKey) || "reach";
  let returnToMember = true;
  const panel = el("section", { class: "member-tabpanel", role: "tabpanel", id: "member-panel-reach", "aria-labelledby": "member-tab-reach", tabindex: "0" }, []);
  const tablist = tabs(tabItems, selectedTab, id => {
    selectedTab = id;
    tabSelections.set(selectionKey, id);
    tablist.update(id);
    renderTab(panel, id, { reach, report, steps, member, agent, sessionId, memberId }, t);
  }, t);
  const footer = el("footer", { class: "member-drawer-footer" }, [
    el("button", { class: "secondary-button", type: "button", onclick: () => {
      returnToMember = false;
      prefillSessionMessage(sessionId, t("member.dismiss_prefill", { role: agent.role }));
      dialog.close();
    } }, [t("member.ask_dismiss")]),
    el("a", { class: "secondary-button button-link", href: `#/sessions/${sessionId}/trajectory?agent=${encodeURIComponent(memberId)}` }, [t("member.see_trajectory")]),
    el("span", { class: "mono" }, [`${t("member.short_id")}: ${shortId(memberId)}`]),
    el("span", { class: "mono" }, [`${t("member.scope_hash")}: ${String(reach.raw?.granted?.hash || "").slice(0, 16)}`]),
  ]);
  const content = el("div", { class: "member-drawer-panel" }, [header, task, tablist, panel, footer]);
  append(dialog, content);
  append(root, dialog);
  renderTab(panel, selectedTab, { reach, report, steps, member, agent, sessionId, memberId }, t);
  const memberLink = [...root.querySelectorAll("[data-member-open]")].find(link => link.getAttribute("data-member-open") === memberId);
  toggleClass(root.querySelector(".conversation-scroll"), "member-content-dim", true);
  if (memberLink) {
    toggleClass(memberLink.closest(".team-member-row"), "team-member-highlight", true);
    memberLink.focus();
  }
  dialog.addEventListener("close", () => {
    toggleClass(document.querySelector(".console-shell"), "member-drawer-open", false);
    if (returnToMember) focusMemberOnSession(sessionId, memberId);
    location.hash = `#/sessions/${sessionId}`;
  });
  openDrawer(dialog);
}

function renderTab(panel, id, data, t) {
  setAttr(panel, "id", `member-panel-${id}`);
  setAttr(panel, "aria-labelledby", `member-tab-${id}`);
  clear(panel);
  if (id === "steps") append(panel, (data.steps.steps || []).map(step => stepRow(step, t)));
  else if (id === "report") {
    append(panel, data.report.text ? el("article", { class: "member-report" }, [data.report.text]) : el("p", { class: "muted" }, [t("member.report_empty")]));
    data.report.at ? append(panel, el("p", { class: "message-meta" }, [t("member.report_at", { time: localTime(data.report.at) })])) : null;
  } else append(panel, reachContent(data.reach, data.agent, t));
}

function reachContent(reach, agent, t) {
  const content = el("div", { class: "reach-content" }, [
    el("p", { class: "member-reach-summary" }, [reachSummarySentence(reach.summary, t)]),
    el("section", { class: "reach-section" }, [
      el("h3", {}, [t("reach.can_reach")]),
      (reach.can_reach || []).length ? el("ul", { class: "reach-items" }, reach.can_reach.map(item => el("li", { class: "reach-item" }, [
        el("span", { class: "reach-check", "aria-hidden": "true" }, ["✓"]),
        el("span", {}, [t(item.text?.key || "reach.item", item.text?.args || {})]),
        el("span", { class: "reach-tag" }, [t(`reach.tag.${String(item.tag || "").replaceAll(" ", "_")}`)]),
      ]))) : el("p", { class: "muted" }, [t("reach.none")]),
    ]),
    el("section", { class: "reach-section" }, [
      el("h3", {}, [t("reach.reduced")]),
      (reach.reduced || []).length ? el("ul", { class: "reach-reductions" }, reach.reduced.map(item => el("li", { class: "reach-reduction" }, [
        el("code", { class: "mono" }, [item.item || ""]),
        el("span", { class: "pill pill-reduced" }, [t(`reach.reason.${item.reason}`)]),
      ]))) : el("p", { class: "muted" }, [t("reach.none_reduced")]),
    ]),
    derivation(reach.derivation, t),
    el("p", { class: "reach-explanation" }, [t("reach.derivation_explanation")]),
    containerProfile(reach.container || {}, t),
    el("details", { class: "raw-detail", "data-raw": "" }, [el("summary", {}, [t("member.reach_raw")]), el("pre", { class: "command-block mono", tabindex: "0", role: "region", "aria-label": t("member.reach_raw") }, [JSON.stringify(reach.raw || {}, null, 2)])]),
  ]);
  return content;
}

export function reachSummarySentence(summary, t) {
  const args = summary?.args || {};
  if (summary?.key === "reach.summary" && args.network === "none" && Number(args.credentials || 0) === 0) {
    return t("reach.summary.no_network", { folders: Number(args.folders || 0) });
  }
  return t(summary?.key || "reach.no_file_access", args);
}

export function derivation(value = {}, t) {
  const keys = [["requested", "member.derivation_requested"], ["lead", "member.derivation_lead"], ["ceiling", "member.derivation_ceiling"], ["granted", "member.derivation_granted"]];
  return el("section", { class: "reach-section" }, [
    el("h3", {}, [t("reach.derivation")]),
    value ? el("div", { class: "reach-derivation" }, keys.map(([key, title]) => el("div", { class: "reach-derivation-column" }, [
      el("h4", {}, [t(title)]),
      ...(value[key] || []).map(chip => el("code", { class: `reach-chip${chip.struck ? " struck" : ""}${chip.lowered ? " lowered" : ""}` }, [chip.text || ""])),
      ...(value[key] || []).length ? [] : [el("span", { class: "muted" }, [t("reach.none")])],
    ]))) : el("p", { class: "muted" }, [t("reach.lead_only")]),
  ]);
}

function containerProfile(profile, t) {
  const args = profile.args || {};
  const localized = {
    ...args,
    root_filesystem: t(args.root_read_only ? "reach.root_read_only" : "reach.root_writable"),
    new_privileges: t(args.no_new_privileges ? "reach.new_privileges_disabled" : "reach.new_privileges_available"),
    network: t(`reach.network.${args.network || "none"}`),
  };
  return el("section", { class: "reach-section" }, [
    el("h3", {}, [t("member.container_profile")]),
    el("div", { class: "container-profile" }, [
      el("article", {}, [el("h4", {}, [t("reach.files")]), el("p", {}, [t(profile.files_key || "reach.container.files", localized)])]),
      el("article", {}, [el("h4", {}, [t("reach.process")]), el("p", {}, [t(profile.process_key || "reach.container.process", localized)])]),
      el("article", {}, [el("h4", {}, [t("reach.network")]), el("p", {}, [t(profile.network_key || "reach.container.network", localized)])]),
    ]),
  ]);
}
