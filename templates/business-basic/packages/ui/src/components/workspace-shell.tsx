import type { ReactNode } from "react";
import { localeRegistry, resolveLocale, withLocale } from "../i18n/locales";
import { shellT as translateShell } from "../i18n/messages";
import { businessModules, type BusinessModuleId } from "../navigation/model";
import { resolveThemeMode, themeModes, withThemeMode } from "../theme/modes";

export type WorkspaceShellLinkComponent = (props: {
  href: string;
  className?: string;
  children: ReactNode;
}) => ReactNode;

export function WorkspaceShell({
  children,
  currentHref,
  brandName,
  moduleId,
  submoduleId,
  locale,
  theme,
  moduleIds,
  LinkComponent
}: {
  children: ReactNode;
  currentHref?: string;
  brandName?: string;
  moduleId?: BusinessModuleId;
  submoduleId?: string;
  locale?: string;
  theme?: string;
  moduleIds?: string[];
  LinkComponent: WorkspaceShellLinkComponent;
}) {
  const visibleModules = moduleIds?.length
    ? businessModules.filter((module) => moduleIds.includes(module.id))
    : businessModules;
  const activeModule = businessModules.find((module) => module.id === moduleId);
  const activeSubmodule = activeModule?.submodules.find((submodule) => submodule.id === submoduleId);
  const Link = LinkComponent;
  const activeLocale = resolveLocale(locale);
  const activeTheme = resolveThemeMode(theme);
  const activeHref = currentHref ?? activeSubmodule?.href ?? activeModule?.href ?? "/app";
  const moduleLabel = (id: string, fallback: string) => navLabels[activeLocale]?.modules[id] ?? fallback;
  const submoduleLabel = (id: string, fallback: string) => navLabels[activeLocale]?.submodules[id] ?? fallback;
  const showSubmoduleNav = moduleId !== "ctox" && (activeModule?.submodules.length ?? 0) > 1;

  return (
    <div className="app-shell" data-module={moduleId ?? "workspace"} data-theme={activeTheme}>
      <header className="workspace-header">
        <div className="workspace-top-row">
          <Link className="brand" href={withThemeMode(withLocale("/app", activeLocale), activeTheme)}>
            {brandName ?? translateShell(activeLocale, "brand")}
          </Link>
          <nav className="module-nav" aria-label="Business modules">
            {visibleModules.map((module) => (
              <Link
                className={module.id === moduleId ? "active" : ""}
                href={withThemeMode(withLocale(module.href, activeLocale), activeTheme)}
                key={module.id}
              >
                {moduleLabel(module.id, module.label)}
              </Link>
            ))}
          </nav>
          <div className="language-switcher" aria-label={translateShell(activeLocale, "language")}>
            {localeRegistry.map((entry) => (
              <Link
                className={entry.code === activeLocale ? "active" : ""}
                href={withThemeMode(withLocale(activeHref, entry.code), activeTheme)}
                key={entry.code}
              >
                {entry.code.toUpperCase()}
              </Link>
            ))}
          </div>
          <div className="theme-switcher" aria-label="Theme">
            {themeModes.map((entry) => (
              <Link
              className={entry.id === activeTheme ? "active" : ""}
              href={withThemeMode(withLocale(activeHref, activeLocale), entry.id)}
              key={entry.id}
            >
                {activeLocale === "de" ? themeLabelsDe[entry.id] ?? entry.label : entry.label}
              </Link>
            ))}
          </div>
        </div>
        {showSubmoduleNav ? (
          <nav className="submodule-nav" aria-label="Module sections">
            {(activeModule?.submodules ?? []).map((submodule) => (
              <Link
                className={submodule.id === submoduleId ? "active" : ""}
                href={withThemeMode(withLocale(submodule.href, activeLocale), activeTheme)}
                key={submodule.id}
              >
                {submoduleLabel(submodule.id, submodule.label)}
              </Link>
            ))}
          </nav>
        ) : null}
      </header>
      <main className="main">{children}</main>
    </div>
  );
}

const themeLabelsDe: Record<string, string> = {
  light: "Hell",
  dark: "Dunkel"
};

const navLabels: Record<string, {
  modules: Record<string, string>;
  submodules: Record<string, string>;
}> = {
  de: {
    modules: {
      sales: "Vertrieb",
      marketing: "Marketing",
      operations: "Betrieb",
      business: "Geschäft",
      documents: "Dokumente",
      content: "Content Studio",
      developer: "Developer Studio",
      deployment: "Deployment",
      security: "Security",
      integrations: "Integrationen",
      research: "Research Desk",
      support: "Support Desk",
      ctox: "CTOX"
    },
    submodules: {
      library: "Bibliothek",
      spreadsheets: "Tabellen",
      slides: "Slides",
      drawings: "Zeichnungen",
      transcripts: "Transkripte",
      images: "Bilder",
      video: "Video",
      voice: "Voice",
      design: "Design",
      web: "Web UI",
      apps: "Apps",
      frameworks: "Frameworks",
      notebooks: "Notebooks",
      "source-control": "Source Control",
      quality: "Qualität",
      overview: "Übersicht",
      vercel: "Vercel",
      cloudflare: "Cloudflare",
      netlify: "Netlify",
      render: "Render",
      "best-practices": "Best Practices",
      ownership: "Ownership",
      "threat-models": "Threat Models",
      linear: "Linear",
      notion: "Notion",
      desk: "Desk",
      "openai-docs": "OpenAI Docs",
      "notion-research": "Notion Research",
      tickets: "Tickets",
      monitoring: "Monitoring",
      zammad: "Zammad",
      pipeline: "Pipeline",
      accounts: "Konten",
      contacts: "Kontakte",
      leads: "Leads",
      offers: "Angebote",
      tasks: "Aufgaben",
      website: "Webseite",
      assets: "Materialien",
      campaigns: "Kampagnen",
      "competitive-analysis": "Wettbewerbsanalyse",
      research: "Recherche",
      commerce: "Commerce",
      projects: "Projekte",
      "work-items": "Arbeitsobjekte",
      boards: "Boards",
      wiki: "Wiki",
      meetings: "Meetings",
      customers: "Kunden",
      products: "Produkte",
      invoices: "Rechnungen",
      ledger: "Ledger",
      "fixed-assets": "Anlagen",
      receipts: "Eingangsbelege",
      payments: "Zahlungen",
      bookkeeping: "Buchhaltung",
      reports: "Berichte",
      runs: "Läufe",
      queue: "Queue",
      knowledge: "Wissen",
      bugs: "Fehler",
      sync: "Sync",
      settings: "Settings"
    }
  },
  en: {
    modules: {},
    submodules: {}
  }
};
