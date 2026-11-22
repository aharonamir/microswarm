let locale = "en";
let dictionaries = {};
export async function loadLocale(lang = "en") { locale = lang === "zh" ? "zh" : "en"; dictionaries[locale] ||= await fetch(`/i18n/${locale}.json`).then(response => response.json()); }
export function t(key, args = {}) {
  const value = translationTemplate(key);
  return value.replace(/\{([a-z_]+)\}/g, (_, name) => String(args[name] ?? `{${name}}`));
}
export function translationTemplate(key) { return dictionaries[locale]?.[key] || dictionaries.en?.[key] || key; }
export function language() { return locale; }
