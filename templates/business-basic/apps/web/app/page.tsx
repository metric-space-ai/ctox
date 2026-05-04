import Link from "next/link";
import { cookies } from "next/headers";
import { localeRegistry, resolveLocale, resolveThemeMode, themeModes, withLocale, withThemeMode } from "@ctox-business/ui";
import { resolveBusinessAccessFromCookies } from "../lib/business-auth";
import { businessOsName, companyNameCookieName } from "../lib/company-settings";

const homeCopy = {
  de: {
    eyebrow: "Workspace",
    subtitle: "Operative Arbeitsflaechen fuer Marketing, Vertrieb, Betrieb, Geschaeft und CTOX.",
    primary: "App oeffnen",
    marketing: "Marketing oeffnen",
    marketingText: "Webseite, Materialien, Wettbewerbsanalyse, Recherche und Commerce bilden die Nachfrage-Seite vor Sales.",
    sales: "Vertrieb oeffnen",
    campaigns: "Kampagnen",
    pipeline: "Pipeline",
    leads: "Leads",
    offers: "Angebote",
    customers: "Customers",
    salesText: "Kampagnen, Pipeline, Leads, Angebote und Kunden funktionieren einzeln und als optionaler Funnel.",
    operations: "Betrieb",
    operationsText: "Onboarding, Projekte, Work Items und Knowledge laufen nach dem Sales-Handoff weiter.",
    business: "Geschaeft",
    businessText: "Kunden, Produkte, Rechnungen, Buchhaltung und Reports bleiben im Business-Modul.",
    ctox: "CTOX",
    ctoxText: "Tasks, Sync, Agent Runs und Bug Reports halten die Module verbunden."
  },
  en: {
    eyebrow: "Workspace",
    subtitle: "Operational workspaces for Marketing, Sales, Operations, Business, and CTOX.",
    primary: "Open app",
    marketing: "Open Marketing",
    marketingText: "Website, assets, competitive analysis, research, and commerce form the demand side before Sales.",
    sales: "Open Sales",
    campaigns: "Campaigns",
    pipeline: "Pipeline",
    leads: "Leads",
    offers: "Offers",
    customers: "Customers",
    salesText: "Campaigns, pipeline, leads, offers, and customers work independently and as an optional funnel.",
    operations: "Operations",
    operationsText: "Onboarding, projects, work items, and knowledge continue after the Sales handoff.",
    business: "Business",
    businessText: "Customers, products, invoices, bookkeeping, and reports stay in the Business module.",
    ctox: "CTOX",
    ctoxText: "Tasks, sync, agent runs, and bug reports keep the modules connected."
  }
};

export default async function PublicHome({
  searchParams
}: {
  searchParams: Promise<{ locale?: string; theme?: string }>;
}) {
  const { locale, theme } = await searchParams;
  const activeLocale = resolveLocale(locale) as keyof typeof homeCopy;
  const activeTheme = resolveThemeMode(theme);
  const copy = homeCopy[activeLocale];
  const cookieStore = await cookies();
  const brandName = businessOsName(cookieStore.get(companyNameCookieName)?.value);
  const appHref = withThemeMode(withLocale("/app", activeLocale), activeTheme);
  const marketingHref = withThemeMode(withLocale("/app/marketing/website", activeLocale), activeTheme);
  const salesHref = withThemeMode(withLocale("/app/sales/leads", activeLocale), activeTheme);
  const businessAccess = await resolveBusinessAccessFromCookies(cookieStore);
  const isLoggedIn = Boolean(businessAccess);

  return (
    <div className="public-shell" data-theme={activeTheme}>
      <header className="public-header">
        {isLoggedIn ? <Link className="public-brand" href={appHref}>{brandName}</Link> : <span aria-hidden="true" />}
        <div className="public-header-actions">
          {isLoggedIn ? (
            <>
              <div className="language-switcher" aria-label="Language">
                {localeRegistry.map((entry) => (
                  <Link
                    className={entry.code === activeLocale ? "active" : ""}
                    href={withThemeMode(withLocale("/", entry.code), activeTheme)}
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
                    href={withThemeMode(withLocale("/", activeLocale), entry.id)}
                    key={entry.id}
                  >
                    {entry.label}
                  </Link>
                ))}
              </div>
            </>
          ) : null}
          <Link className="login-link" href={withThemeMode(withLocale("/login", activeLocale), activeTheme)}>Login</Link>
        </div>
      </header>
      {isLoggedIn ? <main className="public-home" aria-label="Public website content">
        <section className="public-home-hero">
          <span>{copy.eyebrow}</span>
          <h1>{brandName}</h1>
          <p>{copy.subtitle}</p>
          <div className="public-home-actions">
            <Link className="public-home-primary" href={appHref}>{copy.primary}</Link>
            <Link className="public-home-secondary" href={salesHref}>{copy.sales}</Link>
          </div>
        </section>

        <section className="public-home-grid" aria-label="Workspace modules">
          <Link href={marketingHref}>
            <span>Marketing</span>
            <strong>{copy.marketing}</strong>
            <p>{copy.marketingText}</p>
            <small>Website · Assets · Competitive Analysis · Research · Commerce</small>
          </Link>
          <Link href={salesHref}>
            <span>Sales</span>
            <strong>{copy.sales}</strong>
            <p>{copy.salesText}</p>
            <small>{copy.campaigns} · {copy.pipeline} · {copy.leads} · {copy.offers} · {copy.customers}</small>
          </Link>
          <Link href={withThemeMode(withLocale("/app/operations/projects", activeLocale), activeTheme)}>
            <span>Operations</span>
            <strong>{copy.operations}</strong>
            <p>{copy.operationsText}</p>
            <small>Projects · Work Items · Knowledge</small>
          </Link>
          <Link href={withThemeMode(withLocale("/app/business/customers", activeLocale), activeTheme)}>
            <span>Business</span>
            <strong>{copy.business}</strong>
            <p>{copy.businessText}</p>
            <small>Customers · Products · Invoices</small>
          </Link>
          <Link href={withThemeMode(withLocale("/app/ctox/queue", activeLocale), activeTheme)}>
            <span>CTOX</span>
            <strong>{copy.ctox}</strong>
            <p>{copy.ctoxText}</p>
            <small>Tasks · Sync · Runs · Bugs</small>
          </Link>
        </section>
      </main> : <main className="public-empty" aria-label="Public website content" />}
    </div>
  );
}
