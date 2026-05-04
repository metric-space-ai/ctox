export type ThemeMode = "light" | "dark";

export const defaultThemeMode: ThemeMode = "light";

export const themeModes: Array<{ id: ThemeMode; label: string }> = [
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" }
];

export function resolveThemeMode(mode?: string | null): ThemeMode {
  return mode === "dark" ? "dark" : defaultThemeMode;
}

export function withThemeMode(href: string, mode: string) {
  const [path = "", query = ""] = href.split("?", 2);
  const params = new URLSearchParams(query);
  params.set("theme", resolveThemeMode(mode));
  const serialized = params.toString();
  return serialized ? `${path}?${serialized}` : path;
}

