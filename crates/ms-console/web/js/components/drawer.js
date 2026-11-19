import { el } from "../dom.js";

export function drawer() {
  const dialog = el("dialog", { class: "member-drawer", "aria-labelledby": "member-title", "aria-describedby": "member-task" }, []);
  dialog.addEventListener("keydown", event => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    dialog.close();
  });
  return dialog;
}

export function openDrawer(dialog) {
  if (matchMedia("(max-width: 1099px)").matches) dialog.showModal();
  else dialog.show();
  dialog.querySelector(".member-drawer-close")?.focus();
}
