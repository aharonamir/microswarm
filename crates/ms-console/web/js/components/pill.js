import { el } from "../dom.js";

const decisionKeys = {
  allowed: "decision.allowed", asked_you: "decision.asked_you", approved: "decision.approved",
  denied_by_you: "decision.denied_by_you", denied_by_policy: "decision.denied_by_policy",
  failed: "decision.failed", timed_out: "decision.timed_out", reduced: "decision.reduced", expired: "decision.expired",
};

export function statusPill(value, t) {
  const words = value?.status_words || value?.status || value || {};
  const key = words.key || (words.state ? `status.${words.state}` : "status.unknown");
  return el("span", { class: `pill pill-${words.tone || "neutral"}` }, [
    el("span", { class: "status-dot", "aria-hidden": "true" }), t(key, words.args || {}),
  ]);
}

export function decisionPill(decision, t) {
  const value = typeof decision === "string" ? decision : decision?.value;
  if (!value || !decisionKeys[value]) return el("span", {}, []);
  return el("span", { class: `pill decision-pill decision-${value}` }, [t(decisionKeys[value])]);
}
