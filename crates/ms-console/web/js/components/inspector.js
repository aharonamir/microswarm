import { el } from "../dom.js";
import { localTime, duration } from "../format.js";
import { avatar } from "./avatar.js";
import { sentenceNodes } from "./step.js";
import { decisionPill } from "./pill.js";

export function inspector(data, sessionId, t, ordinal = {}) {
  if (!data?.step) return el("aside", { class: "trajectory-inspector" }, [el("h2", {}, [t("trajectory.inspect")]), el("p", {}, [t("trajectory.pick_step")])]);
  const step = data.step, decision = step.decision?.value;
  const why = ["asked_you", "failed", "timed_out", "expired", "reduced"].includes(decision) ? "why.stopped" : decision?.startsWith("denied") ? "why.denied" : "why.allowed";
  const explanation = data.explanation || {};
  const explanationKey = explanation.text?.key || explanation.key || `trajectory.why.${decision || "other"}`;
  const facts = [["trajectory.actor", step.agent?.role], ["trajectory.at", localTime(step.at)], ["trajectory.duration", step.duration_ms == null ? t("trajectory.unfinished") : duration(step.duration_ms)], ...(step.decision?.rule ? [["approval.policy", step.decision.rule]] : [])];
  return el("aside", { class: "trajectory-inspector", "aria-label": t("trajectory.inspect") }, [
    el("header", { class: "inspector-header" }, [avatar(step.agent), el("h2", {}, sentenceNodes(step.sentence, t))]),
    el("p", { class: "mono" }, [t("trajectory.ordinal", { index: ordinal.index || 1, total: ordinal.total || 1 }), " · ", localTime(step.at)]),
    decisionPill(step.decision, t),
    el("section", { class: "inspector-why" }, [el("h3", {}, [t(why)]), el("p", {}, [t(explanationKey, explanation.text?.args || explanation.args || {})])]),
    el("dl", { class: "inspector-facts" }, facts.flatMap(([key, value]) => [el("dt", {}, [t(key)]), el("dd", {}, [value || t("reach.none")])])),
    ...(explanation.facts || []).map(fact => el("p", {}, [t(fact.key, fact.args || {})])),
    el("div", { class: "inspector-actions" }, [
      ...(step.approval?.code ? [el("a", { class: "secondary-button button-link", href: `#/approvals/${step.approval.code}` }, [t("trajectory.review")])] : []),
      ...(step.agent?.kind === "member" ? [el("a", { href: `#/sessions/${sessionId}/members/${step.agent.id}` }, [t("trajectory.open_member", { role: step.agent.role })])] : []),
    ]),
    el("details", { class: "raw-detail", "data-raw": "" }, [el("summary", {}, [t("trajectory.raw", { count: (data.events || []).length })]), el("pre", { class: "command-block mono", tabindex: "0" }, [JSON.stringify(data.events || [], null, 2)])]),
  ]);
}
