import { el, clear } from "./dom.js";
import { currentRoute, onRoute } from "./router.js";
import { loadLocale, t } from "./i18n.js";

const app = document.getElementById("app");
const nav = document.querySelector(".primary-nav");

async function render() {
  await loadLocale(localStorage.getItem("ms-lang") || "en");
  const { page } = currentRoute();
  for (const link of nav.querySelectorAll("a[data-route]")) {
    if (link.dataset.route === page) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  }
  clear(app);
  app.append(el("section", { class: "page-main panel" }, [
    el("p", { class: "label" }, [t("console.eyebrow")]),
    el("h1", { class: "page-title" }, [t(`nav.${page}`)]),
    el("p", { class: "muted" }, [t("console.loading")]),
  ]));
}

document.getElementById("language-toggle").addEventListener("click", async event => {
  const next = localStorage.getItem("ms-lang") === "zh" ? "en" : "zh";
  localStorage.setItem("ms-lang", next);
  event.currentTarget.setAttribute("aria-pressed", String(next === "zh"));
  await render();
});
onRoute(render);
