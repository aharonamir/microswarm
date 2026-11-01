let locale = "en";
let dictionaries = {};
export async function loadLocale(lang = "en") { locale = lang === "zh" ? "zh" : "en"; dictionaries[locale] ||= await fetch(`/i18n/${locale}.json`).then(response => response.json()); }
export function t(key) { return dictionaries[locale]?.[key] || key; }
export function language() { return locale; }
