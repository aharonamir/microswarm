import { api } from "../api.js";
import { clear, el } from "../dom.js";

let state = "pending";
const localDecisions = new Set();

export function markDecisionHere(id) {
  if (!id) return;
  localDecisions.add(id);
  try { sessionStorage.setItem(`ms-console-decided:${id}`, "1"); } catch { /* Private browsing may disable storage. */ }
}

function wasDecidedHere(id) {
  if (localDecisions.has(id)) return true;
  try { return sessionStorage.getItem(`ms-console-decided:${id}`) === "1"; } catch { return false; }
}

export async function renderApprovals(root, selectedId, t) {
  clear(root);
  let result;
  try { result = await api(`/approvals?state=${state}`); }
  catch (error) { root.append(el("section", { class: "panel page-main" }, [el("h1", { class: "page-title" }, [t("nav.approvals")]), el("p", { role: "alert" }, [error.message])])); return; }
  const approvals = result.approvals || [];
  const selected = selectedId ? approvals.find(item => item.code === selectedId || item.id === selectedId) : approvals[0];
  const rail = el("aside", { class: "page-rail panel approval-rail", "aria-label": t("nav.approvals") }, [
    el("h1", { class: "page-title" }, [t("nav.approvals")]),
    el("div", { class: "approval-tabs", role: "tablist" }, ["pending", "decided", "expired"].map(value => el("button", { class: value === state ? "tab active" : "tab", role: "tab", "aria-selected": String(value === state), onclick: async () => { state = value; await renderApprovals(root, null, t); } }, [t(`approvals.${value}`)]))),
    el("div", { class: "session-list" }, approvals.length ? approvals.map(item => el("a", { class: "session-link", href: `#/approvals/${item.code}`, "aria-current": item.code === selected?.code ? "page" : false }, [
      el("strong", {}, [`${item.requesting_role || t("approvals.member")} · ${item.tool_call?.tool || item.kind}`]),
      el("span", { class: "session-meta" }, [item.code]),
      el("span", { class: `status-line tone-${item.state === "pending" ? "warning" : "neutral"}` }, [item.state]),
    ])) : [el("p", { class: "muted empty-copy" }, [t(`approvals.empty.${state}`)])]),
  ]);
  if (!selected) {
    root.append(rail, el("section", { class: "page-main panel approval-detail empty-state" }, [el("p", {}, [t("approvals.select")])]));
    return;
  }
  let detail = selected;
  try { detail = (await api(`/approvals/${selected.code}`)).approval; } catch { /* Keep the queue row visible. */ }
  const call = detail.tool_call;
  const detailBody = call ? [
    el("p", { class: "label" }, [t("approvals.exact_call")]),
    el("h2", { class: "page-title" }, [`${call.server} / ${call.tool}`]),
    el("p", { class: "binding-notice" }, [t("approvals.binding_notice")]),
    el("p", { class: "mono" }, [`binding ${call.argument_hash || ""}`]),
    el("h3", {}, [t("approvals.args")]),
    el("pre", { class: "command-block mono" }, [JSON.stringify(call.arguments || {}, null, 2)]),
    call.parsed ? el("section", { class: "parsed-command" }, [el("h3", {}, [t("approvals.parsed")]), el("p", {}, [`${t(`approvals.parsed_kind.${call.parsed.kind}`)} · ${t("approvals.program")}: ${call.parsed.commands[0]?.split(/\s+/)[0] || ""}`]), ...call.parsed.commands.map(command => el("code", { class: "parsed-part" }, [command]))]) : el("p", {}, [t("approvals.parse_unavailable")]),
    el("h3", {}, [t("approvals.runs_in")]),
    el("pre", { class: "command-block mono" }, [JSON.stringify(detail.runs_in || {}, null, 2)]),
  ] : [
    el("p", { class: "label" }, [t("approvals.spawn")]),
    el("h2", { class: "page-title" }, [detail.spawn?.role || t("approvals.member")]),
    el("p", {}, [detail.spawn?.task || ""]),
    el("h3", {}, [t("approvals.requested_scope")]),
    el("pre", { class: "command-block mono" }, [JSON.stringify(detail.spawn?.requested || {}, null, 2)]),
  ];
  const pending = detail.state === "pending";
  root.append(rail, el("section", { class: "page-main panel approval-detail" }, [
    el("div", { class: "approval-detail-scroll" }, [
      el("div", { class: "approval-meta" }, [`${t("approvals.asked_by")} ${detail.requesting_role || t("approvals.member")} · ${detail.code}`]),
      ...detailBody,
      (detail.state === "approved" || detail.state === "denied") && !wasDecidedHere(detail.id) ? el("p", { class: "muted" }, [t("approvals.already_decided")]) : el("span", {}, []),
    ]),
    pending ? el("footer", { class: "approval-footer" }, [
      el("button", { class: "secondary-button deny-button", onclick: () => decide(detail, "deny", root, selectedId, t) }, [t("approvals.deny")]),
      el("button", { class: "primary-button", onclick: () => decide(detail, "approve", root, selectedId, t) }, [t("approvals.approve_call")]),
    ]) : el("span", {}, []),
  ]));
}

async function decide(approval, action, root, selectedId, t) {
  try { await api(`/approvals/${approval.code}/${action}`, { method: "POST", body: "{}" }); markDecisionHere(approval.id); }
  catch (error) { root.append(el("p", { class: "inline-error", role: "alert" }, [error.code === "already_resolved" ? t("approvals.already_decided") : error.message])); }
  await renderApprovals(root, selectedId, t);
}
