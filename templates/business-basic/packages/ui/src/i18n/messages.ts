export type ShellMessageKey =
  | "brand"
  | "language"
  | "filter"
  | "new"
  | "workingView"
  | "now"
  | "next"
  | "done";

export type ShellMessages = Record<ShellMessageKey, string>;

export const shellMessages: Record<string, ShellMessages> = {
  de: {
    brand: "CTOX Business OS",
    language: "Sprache",
    filter: "Filter",
    new: "Neu",
    workingView: "Arbeitsansicht",
    now: "Jetzt",
    next: "Als nächstes",
    done: "Erledigt"
  },
  en: {
    brand: "CTOX Business OS",
    language: "Language",
    filter: "Filter",
    new: "New",
    workingView: "Working view",
    now: "Now",
    next: "Next",
    done: "Done"
  }
};

export function shellT(locale: string, key: ShellMessageKey) {
  return shellMessages[locale]?.[key] ?? shellMessages.de[key];
}
