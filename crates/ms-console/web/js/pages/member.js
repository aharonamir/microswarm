import { api } from "../api.js";
import { el, clear } from "../dom.js";
import { prefillComposer } from "./sessions.js";

export async function renderMember(root, id, t) {
  clear(root);
  let data;
  try { data = await api(`/members/${id}/scope`); }
  catch (error) { root.append(el("section", { class: "panel page-main" }, [el("p", { role: "alert" }, [error.message])])); return; }
  const comparison = data.rows || [];
  root.append(el("header", { class: "session-heading" }, [el("div", {}, [el("h1", { class: "page-title" }, [data.role || t("sessions.member")]), el("p", { class: "muted" }, [`${t("scope.source")}: ${data.source_ref || ""}`])]), el("a", { class: "secondary-button button-link", href: `#/sessions/${data.lead_id}` }, [t("scope.ask_lead")])]),
    el("section", { class: "panel scope-panel" }, [el("h2", { class: "section-title" }, [t("scope.comparison")]),
      ...comparison.map(row => el("article", { class: "scope-row" }, [el("h3", {}, [t(`scope.dimension.${row.dimension}`)]),
        el("div", { class: "scope-columns" }, [
          scopeColumn(t("scope.requested"), row.requested), scopeColumn(t("scope.lead_scope"), row.lead), scopeColumn(t("scope.granted"), row.granted),
        ]), ...(row.narrowed || []).map(item => el("p", { class: "narrowed-reason" }, [`${item.item}: ${item.reason}`])),
      ])),
    ]),
    el("section", { class: "panel scope-panel" }, [el("h2", { class: "section-title" }, [t("scope.profile")]), el("pre", { class: "command-block mono" }, [JSON.stringify(data.profile || {}, null, 2)])]),
    el("button", { class: "primary-button", onclick: () => { prefillComposer(t("scope.dismiss_prefill", { role: data.role || "member" })); location.hash = `#/sessions/${data.lead_id}`; } }, [t("scope.ask_dismiss")]),
  );
}
function scopeColumn(title, value) { return el("div", { class: "scope-column" }, [el("strong", {}, [title]), el("pre", { class: "mono" }, [JSON.stringify(value ?? [], null, 2)])]); }
