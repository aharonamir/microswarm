import { api } from "../api.js";
import { clear, el } from "../dom.js";

let message = "";
export async function renderHost(root, t) {
  clear(root);
  let data;
  try { data = (await api("/host")).host; }
  catch (error) { root.append(el("section", { class: "panel page-main" }, [el("p", { role: "alert" }, [error.message])])); return; }
  const doctor = el("section", { class: "panel host-section" }, [el("h1", { class: "page-title" }, [t("host.doctor")]), ...(data.doctor || []).map(check => el("article", { class: "host-row" }, [el("strong", {}, [check.check]), el("span", {}, [check.result]), el("p", { class: "muted" }, [check.detail || ""])]))]);
  const channels = el("section", { class: "panel host-section" }, [el("h2", { class: "section-title" }, [t("host.channels")]), ...(data.channels || []).map(item => el("p", {}, [`${item.kind}: ${item.detail}`]))]);
  const credentials = el("section", { class: "panel host-section" }, [
    el("h2", { class: "section-title" }, [t("host.credentials")]),
    el("p", { class: "muted" }, [t("host.values_never_shown")]),
    ...(data.credentials || []).map(item => el("p", { class: "credential-row" }, [el("strong", {}, [item.name]), el("span", { class: "muted" }, [(item.used_by || []).join(", ")])])),
    el("form", { class: "host-form", onsubmit: event => setCredential(event, root, t) }, [
      el("label", { class: "label", for: "credential-name" }, [t("host.credential_name")]), el("input", { id: "credential-name", name: "name", required: true, pattern: "[A-Za-z0-9_\\-]+" }),
      el("label", { class: "label", for: "credential-value" }, [t("host.set_value")]), el("input", { id: "credential-value", name: "value", type: "password", required: true, autocomplete: "new-password" }),
      el("button", { class: "primary-button", type: "submit" }, [t("host.save_credential")]),
    ]),
  ]);
  const skills = el("section", { class: "panel host-section" }, [el("h2", { class: "section-title" }, [t("host.skills")]), ...(data.skills || []).map(skill => el("p", {}, [`${skill.name} · ${skill.version}`])),
    el("form", { class: "host-form", onsubmit: event => installSkill(event, root, t) }, [el("label", { class: "label", for: "skill-path" }, [t("host.skill_path")]), el("input", { id: "skill-path", name: "path", required: true, placeholder: t("host.skill_path_placeholder") }), el("button", { class: "secondary-button", type: "submit" }, [t("host.install_skill")])]),
  ]);
  root.append(el("header", { class: "host-header" }, [el("h1", { class: "page-title" }, [t("nav.host")]), message ? el("p", { role: "status" }, [message]) : el("span", {}, [])]), doctor, channels, credentials, skills,
    el("section", { class: "panel host-section" }, [el("h2", { class: "section-title" }, [t("host.limits")]), el("pre", { class: "command-block mono" }, [JSON.stringify({ limits: data.limits, ceiling: data.ceiling }, null, 2)])]),
  );
}
async function setCredential(event, root, t) {
  event.preventDefault(); const form = event.currentTarget; const name = form.elements.name.value; const value = form.elements.value.value;
  try { await api(`/credentials/${encodeURIComponent(name)}`, { method: "POST", body: JSON.stringify({ value }) }); message = t("host.credential_saved"); form.reset(); }
  catch (error) { message = t("host.operation_failed"); }
  await renderHost(root, t);
}
async function installSkill(event, root, t) {
  event.preventDefault(); const form = event.currentTarget;
  try { await api("/skills/install", { method: "POST", body: JSON.stringify({ path: form.elements.path.value }) }); message = t("host.skill_installed"); form.reset(); }
  catch { message = t("host.operation_failed"); }
  await renderHost(root, t);
}
