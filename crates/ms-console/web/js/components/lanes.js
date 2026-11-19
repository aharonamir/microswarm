import { el, style } from "../dom.js";
import { localTime } from "../format.js";
import { avatar } from "./avatar.js";

const stateKeys = {
  working: "status.working", waiting_you: "status.waiting_for_you", waiting_for_you: "status.waiting_for_you",
  done: "status.done", failed: "status.failed", denied: "status.denied",
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
      el("div", { class: "lane-track", "aria-label": `${lane.agent?.role || t("sessions.member")}: ${(lane.spans || []).map(span => t(stateKeys[span.state] || "status.unknown")).join(", ")}` }, [
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
