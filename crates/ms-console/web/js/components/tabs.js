import { append, el, setAttr } from "../dom.js";

export function tabs(items, selected, onChange, t) {
  const tablist = el("div", { class: "member-tabs", role: "tablist", "aria-label": t("member.tablist") }, []);
  const buttons = items.map(([id, key, args = {}]) => el("button", {
    id: `member-tab-${id}`, class: "member-tab", type: "button", role: "tab", "aria-selected": selected === id,
    "aria-controls": `member-panel-${id}`, tabindex: selected === id ? "0" : "-1",
    onclick: () => onChange(id),
    onkeydown: event => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const current = items.findIndex(item => item[0] === id);
      const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
        : (current + (event.key === "ArrowRight" ? 1 : items.length - 1)) % items.length;
      onChange(items[next][0]);
      tablist.querySelectorAll('[role="tab"]')[next]?.focus();
    },
  }, [t(key, args)]));
  append(tablist, buttons);
  tablist.update = id => buttons.forEach((button, index) => {
    const isSelected = items[index][0] === id;
    setAttr(button, "aria-selected", String(isSelected));
    setAttr(button, "tabindex", isSelected ? "0" : "-1");
  });
  return tablist;
}
