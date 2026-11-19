import { el } from "../dom.js";

export function avatar(agent, { className = "", label = false } = {}) {
  const name = agent?.role || "?";
  return el("span", {
    class: `avatar ${className}`.trim(),
    "data-colour": Number(agent?.colour_index) || 0,
    "aria-label": label ? name : false,
    "aria-hidden": label ? false : "true",
  }, [agent?.initial || name.slice(0, 1).toUpperCase() || "?"]);
}

export function avatarStack(agents, t) {
  return el("span", { class: "avatar-stack", "aria-label": t("session.agent_count", { count: agents.length }) },
    agents.map(agent => avatar(agent, { className: "avatar-stack-item" })));
}
