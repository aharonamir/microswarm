import { el } from "../dom.js";
import { duration } from "../format.js";
import { translationTemplate } from "../i18n.js";
import { decisionPill } from "./pill.js";

const monoArgs = {
  "step.read": ["path"], "step.read_many": ["first"], "step.list": ["path"],
  "step.write": ["path"], "step.edit": ["path"], "step.search": ["pattern"],
  "step.shell": ["command"], "step.shell.ask": ["command"], "step.fetch": ["host"],
  "step.web_search": ["query"], "step.tool": ["server", "tool"],
};

export function sentenceNodes(sentence, t) {
  const args = sentence?.args || {};
  const template = translationTemplate(sentence?.key || "step.other");
  const mono = new Set(monoArgs[sentence?.key] || []);
  const parts = [];
  let cursor = 0;
  for (const match of template.matchAll(/\{([a-z_]+)\}/g)) {
    parts.push(template.slice(cursor, match.index));
    const value = String(args[match[1]] ?? `{${match[1]}}`);
    parts.push(mono.has(match[1]) ? el("code", { class: "sentence-arg mono" }, [value]) : value);
    cursor = match.index + match[0].length;
  }
  parts.push(template.slice(cursor));
  return parts;
}

export function stepRow(step, t, { compact = false } = {}) {
  const durationValue = step?.duration_ms == null ? "" : duration(step.duration_ms);
  return el("li", { class: `step-row${compact ? " step-row-compact" : ""}`, "data-step-id": step?.step_id || "" }, [
    el("span", { class: `step-mark step-mark-${step?.kind || "other"}`, "aria-hidden": "true" }, []),
    el("span", { class: "step-sentence" }, sentenceNodes(step?.sentence, t)),
    durationValue ? el("span", { class: "step-duration mono" }, [durationValue]) : el("span", { class: "step-duration" }, []),
    step?.decision ? decisionPill(step.decision, t) : el("span", {}, []),
  ]);
}
