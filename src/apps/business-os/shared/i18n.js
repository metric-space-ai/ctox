export async function loadModuleMessages(moduleUrl, locale, fallback = {}) {
  const lang = locale === 'en' ? 'en' : 'de';
  const base = new URL('./', moduleUrl);
  const messages = await fetchJson(new URL(`locales/${lang}.json`, base)).catch(() => ({}));
  const fallbackMessages = fallback[lang] || fallback.de || {};
  return { ...fallbackMessages, ...messages };
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return response.json();
}
