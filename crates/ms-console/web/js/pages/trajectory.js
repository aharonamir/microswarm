import { api } from "../api.js";
import { el, append, clear, setProp, remove } from "../dom.js";
import { localTime, duration } from "../format.js";
import { avatar } from "../components/avatar.js";
import { sentenceNodes } from "../components/step.js";
import { decisionPill } from "../components/pill.js";
import { fullLanes } from "../components/lanes.js";
import { inspector } from "../components/inspector.js";

const states = new Map();
let renderVersion = 0;
export async function renderTrajectory(root, sessionId, selectedId, t) {
  const version = ++renderVersion;
  clear(root);
  if (!sessionId) { append(root, el("section", { class: "panel" }, [el("h1", {}, [t("nav.trajectory")]), el("p", {}, [t("trace.choose_session")]), el("a", { href: "#/sessions" }, [t("nav.sessions")])])); return; }
  const [id, query = ""] = sessionId.split("?");
  const agent = new URLSearchParams(query || selectedId?.split("?")[1] || "").get("agent");
  const selected = selectedId ? decodeURIComponent(selectedId.split("?")[0]) : null;
  const stateKey = `${id}:${agent || ""}`;
  if (!states.has(stateKey)) states.set(stateKey, { filter: "everything", steps: [], phases: [], next: null, loaded: false });
  const state = states.get(stateKey);
  const stepQuery = before => `filter=${state.filter}&limit=200${agent ? `&agent=${encodeURIComponent(agent)}` : ""}${before == null ? "" : `&before_seq=${before}`}`;
  let timeline, session, detail;
  try {
    const [timelineData, sessionData, stepsData] = await Promise.all([api(`/sessions/${id}/timeline`), api(`/sessions/${id}`), state.loaded ? null : api(`/sessions/${id}/steps?${stepQuery()}`)]);
    timeline = timelineData; session = sessionData.session || {};
    if (stepsData) { state.steps = stepsData.steps || []; state.phases = stepsData.phases || []; state.next = stepsData.next_before_seq; state.loaded = true; }
    const chosen = selected || state.steps.at(-1)?.step_id;
    if (chosen) detail = await api(`/sessions/${id}/steps/${encodeURI(chosen)}`);
    if (version !== renderVersion) return;
  } catch (error) { if (version === renderVersion) append(root, el("section", { class: "panel", role: "alert" }, [error.message])); return; }
  const chosen = detail?.step?.step_id || selected || state.steps.at(-1)?.step_id;
  const select = stepId => { location.hash = `#/sessions/${id}/trajectory/${encodeURI(stepId)}${agent ? `?agent=${encodeURIComponent(agent)}` : ""}`; };
  const stats = timeline.stats || {};
  const filters = ["everything", "decisions", "problems"];
  const filterButtons = filters.map((value, index) => el("button", { class: "tab", type: "button", "aria-pressed": String(state.filter === value), onclick: () => {
    state.filter = value; state.loaded = false;
    if (selectedId) location.hash = `#/sessions/${id}/trajectory${agent ? `?agent=${encodeURIComponent(agent)}` : ""}`;
    else renderTrajectory(root, sessionId, null, t).then(() => root.querySelectorAll(".trajectory-filters button")[index]?.focus());
  }, onkeydown: event => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault(); const next = event.key === "Home" ? 0 : event.key === "End" ? 2 : (index + (event.key === "ArrowRight" ? 1 : 2)) % 3; filterButtons[next].click();
  } }, [t(`trajectory.${value}`)]));
  const main = el("section", { class: "trajectory-main" }, [
    el("header", { class: "trajectory-header" }, [el("div", {}, [el("a", { href: `#/sessions/${id}` }, [session.title || t("approval.open_session")]), el("h1", {}, [t("nav.trajectory")]), el("p", { class: "muted" }, [t("trajectory.helper")])]), el("button", { class: "secondary-button", onclick: async event => {
      const button = event.currentTarget; setProp(button, "disabled", true);
      try { exportJson(await api(`/sessions/${id}/trace`)); }
      catch (error) { append(main, el("p", { role: "alert" }, [error.message])); }
      finally { setProp(button, "disabled", false); }
    } }, [t("trace.export")])]),
    el("div", { class: "trajectory-filters", role: "group", "aria-label": t("trace.filters") }, filterButtons),
    el("div", { class: "trajectory-tiles" }, [["running", duration(stats.duration_ms)], ["agents", stats.agents || 0], ["steps", stats.steps || 0], ["waiting", duration(stats.waiting_you_ms)], ["reduced", stats.reduced || 0], ["denied", stats.denied_or_failed || 0]].map(([key, value]) => el("div", { class: `trajectory-stat trajectory-stat-${key}` }, [el("span", {}, [t(`trajectory.${key}`)]), el("strong", {}, [String(value)])]))),
    fullLanes(timeline, t, chosen, select),
  ]);
  const list = el("div", { class: "trajectory-steps-list" });
  const loadButton = el("button", { class: "secondary-button", type: "button", onclick: () => loadOlder() }, [t("trajectory.load_older")]);
  const stepsCard = el("section", { class: "trajectory-steps-card" }, [el("h2", {}, [t("trajectory.steps")]), loadButton, list]);
  let loading = false;
  function renderSteps() {
    clear(list); setProp(loadButton, "hidden", state.next == null);
    let previousPhase = null;
    for (const step of state.steps) {
      if (step.phase_id !== previousPhase) {
        const phase = state.phases.find(item => item.phase_id === step.phase_id);
        append(list, el("h3", { class: "trajectory-phase" }, [t(phase?.title_key || "phase.lead_works", phase?.title_args || {}), phase?.started_at ? ` · ${localTime(phase.started_at)}` : ""])); previousPhase = step.phase_id;
      }
      append(list, el("button", { type: "button", class: `trajectory-step${step.decision?.value === "asked_you" ? " waiting" : ""}`, "data-step-id": step.step_id, "aria-current": step.step_id === chosen ? "step" : false, onclick: () => select(step.step_id) }, [el("span", { class: "mono" }, [localTime(step.at)]), avatar(step.agent, { className: "lane-avatar" }), el("span", { class: "step-sentence" }, sentenceNodes(step.sentence, t)), decisionPill(step.decision, t), el("span", { class: "mono" }, [step.duration_ms == null ? "" : duration(step.duration_ms)])]));
    }
    if (!state.steps.length) append(list, el("p", { class: "empty-state" }, [t("trajectory.no_steps")]));
  }
  async function loadOlder() {
    if (loading || state.next == null) return;
    loading = true; setProp(loadButton, "disabled", true);
    try {
      const page = await api(`/sessions/${id}/steps?${stepQuery(state.next)}`);
      if (version !== renderVersion) return;
      const seen = new Set(state.steps.map(step => step.step_id));
      state.steps = [...(page.steps || []).filter(step => !seen.has(step.step_id)), ...state.steps];
      state.phases = [...(page.phases || []), ...state.phases]; state.next = page.next_before_seq;
      renderSteps();
    } catch (error) { append(stepsCard, el("p", { role: "alert" }, [error.message])); }
    finally { loading = false; setProp(loadButton, "disabled", false); }
  }
  renderSteps(); append(main, stepsCard);
  const workspace = el("div", { class: "trajectory-workspace" }, [main, inspector(detail, id, t, { index: detail?.position || state.steps.findIndex(step => step.step_id === chosen) + 1, total: detail?.total || stats.steps || state.steps.length })]);
  append(root, workspace);
  const sentinel = el("div", { class: "trajectory-page-sentinel", "aria-hidden": "true" });
  append(stepsCard, sentinel);
  const observer = new IntersectionObserver(entries => {
    if (!workspace.isConnected) { observer.disconnect(); return; }
    if (entries.some(entry => entry.isIntersecting)) loadOlder();
  }, { rootMargin: "120px" });
  observer.observe(sentinel);
  workspace.addEventListener("scroll", () => { if (workspace.scrollTop + workspace.clientHeight >= workspace.scrollHeight - 120) loadOlder(); });
}
function exportJson(data) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
  const link = el("a", { href: url, download: "microswarm-trace.json", hidden: true }); append(document.body, link); link.click(); remove(link); setTimeout(() => URL.revokeObjectURL(url), 1000);
}
