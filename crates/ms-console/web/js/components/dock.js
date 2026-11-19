import { api } from "../api.js";
import { el, setProp, setText } from "../dom.js";
import { relativeTime } from "../format.js";
import { markDecisionHere } from "../pages/approvals.js";
import { avatar } from "./avatar.js";
import { sentenceNodes } from "./step.js";

export function approvalDock(items, sessionId, t, onChange = () => {}) {
  const pending = (items || []).filter(item => item.state === "pending" && (item.root_session_id || item.session_id) === sessionId)
    .sort((a, b) => String(a.asked_at || a.created_at || "").localeCompare(String(b.asked_at || b.created_at || "")));
  if (!pending.length) return el("div", { class: "approval-dock", "data-approval-dock": "", hidden: true }, []);
  const item = pending[0];
  const agent = item.agent || { role: item.requesting_role || t("sessions.member"), initial: String(item.requesting_role || "M").slice(0, 1).toUpperCase(), colour_index: 0 };
  const command = item.sentence?.args?.command || item.tool_call?.arguments?.command || "";
  const code = item.code || "";
  return el("section", { class: "approval-dock", "data-approval-dock": "", "aria-label": t("dock.approval_heading") }, [
    el("article", { class: "dock-card" }, [
      el("header", { class: "dock-header" }, [
        avatar(agent),
        el("strong", {}, item.sentence ? sentenceNodes(item.sentence, t) : [t("dock.request", { role: agent.role })]),
        el("span", { class: "mono dock-code" }, [code]),
        item.expires_at ? el("span", { class: "mono muted" }, [relativeTime(Date.parse(item.expires_at))]) : el("span", {}, []),
        pending.length > 1 ? el("span", { class: "pill" }, [t("dock.more", { count: pending.length - 1 })]) : el("span", {}, []),
      ]),
      command ? el("pre", { class: "dock-command mono" }, [el("code", {}, [`$ ${command}`])]) : el("span", {}, []),
      el("ul", { class: "dock-facts" }, reachFacts(item.runs_in || {}, t)),
      el("p", { class: "dock-error", role: "status", hidden: true }, []),
      el("footer", { class: "dock-actions" }, [
        el("button", { class: "secondary-button dock-deny", type: "button", onclick: () => decide(item, "deny", t, onChange) }, [t("dock.deny")]),
        el("a", { class: "dock-why", href: `#/approvals/${code}` }, [t("dock.why")]),
        el("button", { class: "primary-button", type: "button", onclick: () => decide(item, "approve", t, onChange) }, [t("dock.approve_command")]),
      ]),
    ]),
  ]);
}

function reachFacts(reach, t) {
  const facts = [];
  for (const folder of reach.folders || []) facts.push(el("li", {}, [el("code", { class: "mono" }, [folder.path]), ` · ${t(folder.mode === "rw" ? "reach.read_write" : "reach.read_only")}`]));
  if (!reach.folders?.length) facts.push(el("li", {}, [t("reach.no_file_access")]));
  facts.push(el("li", {}, [reach.network === "none" || !reach.network ? t("reach.no_network") : `${t("reach.network")}: ${reach.network.replaceAll("_", " ")}`]));
  facts.push(el("li", {}, [reach.credentials?.length ? `${t("reach.credentials")}: ${reach.credentials.join(", ")}` : t("reach.no_credentials")]));
  if (reach.tools_ask?.length) facts.push(el("li", {}, [`${t("reach.asks_you")}: ${reach.tools_ask.join(", ")}`]));
  return facts;
}

async function decide(item, action, t, onChange) {
  const card = document.querySelector(`[data-approval-dock] .dock-card`);
  const buttons = card?.querySelectorAll("button") || [];
  buttons.forEach(button => setProp(button, "disabled", true));
  const message = card?.querySelector(".dock-error");
  try {
    await api(`/approvals/${encodeURIComponent(item.code)}/${action}`, { method: "POST", body: "{}" });
    markDecisionHere(item.id);
    await onChange();
  } catch (error) {
    if (error.code === "already_resolved" || error.code === "already_decided") {
      setText(message, t("approvals.already_decided"));
      setProp(message, "hidden", false);
    } else {
      setText(message, error.message);
      setProp(message, "hidden", false);
    }
  } finally { buttons.forEach(button => setProp(button, "disabled", false)); }
}
