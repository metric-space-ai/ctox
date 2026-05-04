export type LocaleDefinition = {
  code: string;
  label: string;
  nativeLabel: string;
  direction: "ltr" | "rtl";
};

export const defaultLocale = "en";

export const localeRegistry: LocaleDefinition[] = [
  { code: "de", label: "German", nativeLabel: "Deutsch", direction: "ltr" },
  { code: "en", label: "English", nativeLabel: "English", direction: "ltr" }
];

export function isSupportedLocale(locale: string) {
  return localeRegistry.some((entry) => entry.code === locale);
}

export function resolveLocale(locale?: string | null) {
  return locale && isSupportedLocale(locale) ? locale : defaultLocale;
}

export function withLocale(href: string, locale: string) {
  const [path = "", query = ""] = href.split("?", 2);
  const params = new URLSearchParams(query);
  params.set("locale", resolveLocale(locale));
  const serialized = params.toString();
  return serialized ? `${path}?${serialized}` : path;
}
