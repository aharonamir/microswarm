import { el, style } from "../dom.js";
import { localTime } from "../format.js";
import { avatar } from "./avatar.js";

const stateKeys = {
  working: "status.working", waiting_you: "status.waiting_for_you", waiting_for_you: "status.waiting_for_you",
  reported: "status.done", idle: "status.idle", done: "status.done", failed: "status.failed", denied: "status.denied",
};

export function laneStrip(timeline = {}, t) {
  const range = timeline.range || {};
  const start = Date.parse(range.start || "") || Date.now() - 900_000;
  const end = Math.max(start + 1, Date.parse(range.end || "") || Date.now());
  const lanes = timeline.lanes || [];
  const pct = at => Math.max(0, Math.min(100, (Date.parse(at || "") - start) / (end - start) * 100));
  return el("section", { class: "lane-strip", "aria-label": t("team.lane_strip") }, [
    el("div", { class: "lane-times" }, [
      el("span", {}, [localTime(new Date(start).toISOString())]),
      el("span", {}, [localTime(new Date(start + (end - start) / 2).toISOString())]),
      el("span", {}, [t("team.now")]),
    ]),
    ...lanes.filter(lane => lane.agent?.kind !== "you").map(lane => el("div", { class: "lane-row" }, [
      avatar(lane.agent, { className: "lane-avatar" }),
      el("span", { class: "lane-role" }, [lane.agent?.role || t("sessions.member")]),
      el("div", { class: "lane-track", role: "img", "aria-label": `${lane.agent?.role || t("sessions.member")}: ${(lane.spans || []).map(span => t(stateKeys[span.state] || "status.unknown")).join(", ")}` }, [
        ...(lane.spans || []).map(span => {
          const left = pct(span.start);
          const right = pct(span.end);
          const state = span.state || "working";
          const segment = el("span", { class: `lane-span lane-span-${state} lane-span-colour-${Number(lane.agent?.colour_index) || 0}`, "aria-hidden": "true" }, []);
          style(segment, { left: `${left}%`, width: `${Math.max(1, right - left)}%` });
          return segment;
        }),
        ...(lane.marks || []).map(mark => {
          const marker = el("span", { class: `lane-mark lane-mark-${mark.kind || "tool"}`, "aria-hidden": "true" }, []);
          style(marker, { left: `${pct(mark.at)}%` });
          return marker;
        }),
      ]),
    ])),
    el("p", { class: "lane-legend" }, [el("span", { class: "lane-legend-swatch lane-span-waiting_you" }, []), t("team.striped_waiting")]),
  ]);
}

export function fullLanes(timeline, t, selectedId, select) {
  const start = Date.parse(timeline.range?.start) || Date.now();
  const end = Math.max(start + 1, Date.parse(timeline.range?.end) || start + 1);
  const percent = at => Math.max(0, Math.min(100, (Date.parse(at) - start) / (end - start) * 100));
  return el("section", { class: "trajectory-lanes", "aria-label": t("trajectory.lanes") }, [
    el("h2", {}, [t("trajectory.lanes")]),
    el("div", { class: "trajectory-lanes-scroll", tabindex: "0" }, [el("div", { class: "trajectory-lanes-grid" }, [
      el("div", { class: "trajectory-ticks" }, [0, .25, .5, .75, 1].map(ratio => el("span", { class: "mono" }, [ratio === 1 ? t("team.now") : localTime(start + ratio * (end - start))]))),
      ...(timeline.lanes || []).map(lane => el("div", { class: "trajectory-lane" }, [
        el("div", { class: "trajectory-lane-label" }, [avatar(lane.agent, { className: "lane-avatar" }), el("span", {}, [lane.agent?.role || t("sessions.you")])]),
        el("div", { class: "trajectory-lane-track" }, [
          ...(lane.spans || []).map(span => {
            const stepId = span.step_id || (lane.marks || []).find(mark => mark.step_id && Date.parse(mark.at) >= Date.parse(span.start) && Date.parse(mark.at) <= Date.parse(span.end))?.step_id;
            const node = el(stepId ? "button" : "span", { class: `trajectory-span lane-span-${span.state} lane-span-colour-${Number(lane.agent?.colour_index) || 0}${stepId ? " trajectory-lane-control" : ""}`, ...(stepId ? { type: "button", "aria-label": `${lane.agent?.role || t("sessions.you")}: ${t(stateKeys[span.state] || "status.unknown")}`, "aria-pressed": String(selectedId === stepId), onclick: () => select(stepId) } : { "aria-hidden": "true" }) }, [el("span", {}, [t(stateKeys[span.state] || "status.unknown")])]);
            style(node, { left: `${percent(span.start)}%`, width: `${Math.max(1, percent(span.end) - percent(span.start))}%` });
            return node;
          }),
          ...(lane.marks || []).map(mark => {
            const node = el(mark.step_id ? "button" : "span", { class: `trajectory-mark trajectory-mark-${mark.kind}${mark.step_id ? " trajectory-lane-control" : ""}`, ...(mark.step_id ? { type: "button", "aria-label": t("trajectory.select_step", { role: lane.agent?.role || t("sessions.you"), time: localTime(mark.at) }), "aria-pressed": String(selectedId === mark.step_id), onclick: () => select(mark.step_id) } : { "aria-hidden": "true" }) }, []);
            style(node, { left: `${percent(mark.at)}%` });
            return node;
          }),
        ]),
      ])),
    ])]),
    el("p", { class: "lane-legend" }, [t("trajectory.legend")]),
  ]);
}
