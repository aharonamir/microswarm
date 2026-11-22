import { avatar } from "../components/avatar.js";
import { sentenceNodes } from "../components/step.js";
import { derivation, reachSummarySentence } from "./member.js";
import { localTime } from "../format.js";
import { api } from "../api.js";
import { append, clear, el, setProp } from "../dom.js";

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
  catch (error) { append(root, el("section", { class: "panel", role: "alert" }, [error.message])); return; }
  const items = result.approvals || [];
  let selected = items.find(item => item.code === selectedId || item.id === selectedId) || (!selectedId && !matchMedia("(max-width: 699px)").matches ? items[0] : null);
  if (selectedId || selected) {
    try { selected = (await api(`/approvals/${selected?.code || selectedId}`)).approval || selected; }
    catch (error) { if (!selected) append(root, el("p", { role: "alert" }, [error.message])); }
  }
  const queue = el("aside", { class: "approval-queue", "aria-label": t("nav.approvals") }, [
    el(selected ? "h2" : "h1", { class: "page-title" }, [t("nav.approvals")]),
    el("div", { class: "approval-tabs", role: "group", "aria-label": t("nav.approvals") }, ["pending", "decided", "expired"].map((value, index) => el("button", {
      class: "tab", type: "button", "aria-pressed": String(value === state), onclick: async () => {
        state = value;
        if (location.hash !== "#/approvals") location.hash = "#/approvals";
        else { await renderApprovals(root, null, t); root.querySelectorAll(".approval-tabs button")[index]?.focus(); }
      }, onkeydown: event => {
        if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const next = event.key === "Home" ? 0 : event.key === "End" ? 2 : (index + (event.key === "ArrowRight" ? 1 : 2)) % 3;
        root.querySelectorAll(".approval-tabs button")[next].click();
      },
    }, [t(`approvals.${value === "pending" ? "waiting" : value}`)]))),
    el("div", { class: "approval-queue-items" }, items.length ? items.map(item => el("a", {
      class: `approval-queue-item${item.state === "expired" ? " expired" : ""}`, href: `#/approvals/${item.code}`, "aria-current": item.code === selected?.code ? "page" : false,
    }, [avatar(item.agent || { role: item.requesting_role }), el("div", {}, [
      el("strong", {}, item.spawn ? [t("approval.spawn_title", { role: item.spawn.role })] : sentenceNodes(item.sentence || { key: "approval.run_title", args: { role: item.requesting_role || t("approvals.member") } }, t)),
      el("p", { class: "mono" }, [item.tool_call?.arguments?.command || item.spawn?.task || ""]),
      el("span", { class: "muted" }, [item.state === "pending" ? t("approval.expires", { time: localTime(item.expires_at) }) : t(`decision.${item.state === "approved" ? "allowed" : item.state === "expired" ? "expired" : "denied_by_you"}`)]),
    ])])) : [el("p", { class: "muted" }, [t(`approvals.empty.${state}`)])]),
  ]);
  const workspace = el("section", { class: `approval-workspace${selected ? " has-selection" : ""}` }, [queue]);
  append(root, workspace);
  if (!selected) { append(workspace, el("section", { class: "approval-decision empty-state" }, [t("approvals.select")])); return; }
  const detail = selected, call = detail.tool_call, spawn = detail.spawn;
  const role = detail.agent?.role || detail.requesting_role || t("approvals.member");
  const title = spawn ? t("approval.spawn_title", { role: spawn.role }) : t("approval.run_title", { role });
  const scope = detail.runs_in || {};
  const facts = detail.facts || [];
  const whatRuns = spawn ? [el("p", {}, [spawn.task || ""])] : facts.length
    ? [el("ul", {}, facts.map(fact => el("li", {}, [t(fact.key, fact.args || {})])))]
    : [el("p", {}, [t("approvals.parse_unavailable")])];
  const whereRuns = spawn
    ? [el("p", {}, [reachSummarySentence(detail.spawn_preview?.summary, t)])]
    : [el("p", {}, [t("approval.container")]), ...(scope.folders || []).map(folder => el("p", {}, [el("code", {}, [folder.path]), " · ", t(folder.mode === "rw" ? "reach.read_write" : "reach.read_only")])), el("p", {}, [t(scope.network === "none" || !scope.network ? "reach.no_network" : `reach.network.${scope.network}`)]), el("p", {}, [scope.credentials?.length ? t("approval.credentials", { count: scope.credentials.length }) : t("reach.no_credentials")])];
  const tiles = [
    [spawn ? "approval.task" : "approval.what_runs", whatRuns],
    [spawn ? "approval.reach" : "approvals.runs_in", whereRuns],
    ["approvals.why", [el("p", {}, [spawn ? t("approval.requires_answer") : detail.policy?.rule || t("approval.requires_answer")]), el("details", {}, [el("summary", {}, [t("approval.policy")]), el("p", {}, [detail.policy?.source_ref || detail.policy?.source || t("approval.requires_answer")])])]],
  ];
  const body = el("div", { class: "approval-detail-scroll" }, [
    el("a", { class: "approval-back", href: "#/approvals" }, [t("approval.all_requests")]),
    el("header", { class: "approval-decision-header" }, [avatar(detail.agent || { role }), el("div", {}, [el("h1", { class: "approval-title" }, [title]), el("p", { class: "muted" }, [el("a", { href: `#/sessions/${detail.root_session_id}` }, [t("approval.open_session")]), " · ", t("approval.asked", { time: localTime(detail.created_at) }), " · ", t("approval.expires", { time: localTime(detail.expires_at) })])]), el("span", { class: "approval-code mono" }, [detail.code])]),
    call ? el("pre", { class: "approval-command command-block mono" }, [call.arguments?.command || t("approval.tool_request")]) : el("p", {}, [spawn?.task || ""]),
    el("div", { class: "decision-tiles" }, tiles.map(([key, content]) => el("article", {}, [el("h2", {}, [t(key)]), ...content]))),
    el("p", { class: "binding-notice" }, [t(spawn ? "approval.spawn_binding" : "approvals.binding_notice")]),
    detail.brief?.text && !spawn ? el("section", { class: "approval-brief" }, [el("h2", {}, [t("approval.brief", { role })]), el("blockquote", {}, [detail.brief.text])]) : el("span"),
    spawn ? el("details", { class: "approval-access-preview" }, [el("summary", {}, [t("approval.preview")]), derivation(detail.spawn_preview?.derivation, t)]) : el("span"),
    el("details", { class: "raw-detail", "data-raw": "" }, [el("summary", {}, [t("approvals.exact_call")]), el("pre", { class: "command-block mono" }, [JSON.stringify(call || spawn || {}, null, 2)])]),
    detail.state === "expired" ? el("p", {}, [detail.reissued_as ? t("approval.reissued", { code: detail.reissued_as }) : t("approval.expired")]) : el("span"),
    detail.state !== "pending" ? el("p", { role: "status" }, [!wasDecidedHere(detail.id) ? t("approvals.already_decided") : t("approval.saved")]) : el("span"),
  ]);
  const errorBox = el("p", { class: "inline-error", role: "alert", hidden: true });
  const actions = ["deny", "approve"].map(action => el("button", { class: action === "approve" ? "primary-button" : "secondary-button deny-button", type: "button", onclick: async () => {
    actions.forEach(button => setProp(button, "disabled", true));
    try { await api(`/approvals/${detail.code}/${action}`, { method: "POST", body: "{}" }); markDecisionHere(detail.id); await renderApprovals(root, detail.code, t); }
    catch (error) { setProp(errorBox, "hidden", false); clear(errorBox); append(errorBox, error.code === "already_resolved" ? t("approvals.already_decided") : error.message); actions.forEach(button => setProp(button, "disabled", false)); }
  } }, [t(action === "deny" ? "approvals.deny" : spawn ? "approval.approve_spawn" : "approvals.approve_call")]));
  append(workspace, el("section", { class: "approval-decision" }, [body, errorBox, el("footer", { class: "approval-footer" }, [el("p", {}, [t("approval.supervisor_note")]), ...(detail.state === "pending" ? actions : [])])]));
}
