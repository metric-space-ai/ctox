import { cookies } from "next/headers";
import { WorkSurface, type BusinessModuleId } from "@ctox-business/ui";
import { AppShell } from "../../../../components/app-shell";
import { businessOsName, companyNameCookieName, normalizeCompanyName } from "../../../../lib/company-settings";

const pipelineStages = [
  {
    id: "company",
    title: "Unternehmen identifiziert",
    description: "Fit und Stammdaten sind belastbar genug, aber es fehlt noch ein Ansprechpartner."
  },
  {
    id: "contact",
    title: "Ansprechpartner vorhanden",
    description: "Ein Kontakt ist bekannt, seine Entscheidungsnaehe ist aber noch nicht bestaetigt."
  },
  {
    id: "decision",
    title: "Richtiger Ansprechpartner",
    description: "Der richtige Ansprechpartner ist plausibel, Kontaktweg oder Kontext fehlen noch."
  },
  {
    id: "conversation",
    title: "Gespraechsbereitschaft klaeren",
    description: "Der naechste Schritt ist die echte Antwort: will die Person ueber das Anliegen sprechen?"
  },
  {
    id: "lead-ready",
    title: "Lead ready",
    description: "Alle Vorqualifizierungs-Gates sind erfuellt. Uebergabe ins Leads-Modul."
  }
];

const pipelineCards = [
  {
    id: "starter-row-talentbridge",
    companyName: "TalentBridge Consulting",
    website: "https://talentbridge.example",
    contact: "Noch kein Ansprechpartner",
    fit: "HIGH",
    score: 62,
    stageId: "company",
    nextStep: "Ansprechpartner recherchieren und Kontaktweg verifizieren."
  },
  {
    id: "starter-row-rheinrecruit",
    companyName: "RheinRecruit GmbH",
    website: "https://rheinrecruit.example",
    contact: "Lena Hartmann, Head of Recruiting Operations",
    fit: "MEDIUM",
    score: 70,
    stageId: "contact",
    nextStep: "Entscheidungsnaehe pruefen."
  },
  {
    id: "starter-row-novastaff",
    companyName: "NovaStaff Partners",
    website: "https://novastaff.example",
    contact: "Marcel Vogt, Managing Partner",
    fit: "HIGH",
    score: 81,
    stageId: "decision",
    nextStep: "Direkten Kontaktweg und passenden Gespraechsanlass klaeren."
  },
  {
    id: "starter-row-atlaspersonal",
    companyName: "Atlas Personalservice",
    website: "https://atlas-personal.example",
    contact: "Nora Stein, Geschaeftsfuehrerin",
    fit: "HIGH",
    score: 88,
    stageId: "conversation",
    nextStep: "Ansprechen und Termininteresse klaeren."
  },
  {
    id: "starter-row-fieldops",
    companyName: "FieldOps Recruiting",
    website: "https://fieldops-recruiting.example",
    contact: "Amira Sayed, Managing Director",
    fit: "HIGH",
    score: 94,
    stageId: "lead-ready",
    nextStep: "Mit Kampagnenkontext in Leads uebergeben."
  }
];

export default async function SalesPipelinePage({
  searchParams
}: {
  searchParams: Promise<{ locale?: string; selectedId?: string; theme?: string }>;
}) {
  const query = await searchParams;
  const locale = query.locale === "en" ? "en" : "de";
  const cookieStore = await cookies();
  const companyName = normalizeCompanyName(cookieStore.get(companyNameCookieName)?.value);
  const selected = pipelineCards.find((card) => card.id === query.selectedId);

  return (
    <AppShell
      currentHref={`/app/sales/pipeline?locale=${locale}${query.theme ? `&theme=${query.theme}` : ""}`}
      brandName={businessOsName(companyName)}
      moduleId={"sales" as BusinessModuleId}
      submoduleId="pipeline"
      locale={locale}
      theme={query.theme}
    >
      <WorkSurface hideHeader moduleId="sales" submoduleId="pipeline" title="Pipeline" description="Sales workspace">
        <section className="kunstmen-pipeline lead-pipeline" data-context-module="sales" data-context-submodule="pipeline">
          <header className="kunstmen-work-header">
            <div className="kunstmen-work-title">
              <h1>Pipeline</h1>
              <p>{locale === "en" ? "Pre-qualify campaign candidates before they become leads." : "Kampagnen-Kandidaten vorqualifizieren, bevor sie Leads werden."}</p>
            </div>
            <section className="kunstmen-work-summary" aria-label="Pipeline summary">
              <span><strong>{pipelineCards.length}</strong> {locale === "en" ? "active" : "aktiv"}</span>
              <span><strong>{pipelineCards.filter((card) => card.stageId === "lead-ready").length}</strong> Lead-ready</span>
              <span>starter-prospects.xlsx · 5/5</span>
            </section>
          </header>
          <main className="lead-pipeline-layout">
            <div className="kanban-wrap">
              <p className="kanban-status" aria-live="polite">
                {locale === "en"
                  ? "This board is the qualification layer between campaign research and the Leads module."
                  : "Dieses Board ist die Qualifizierungsstufe zwischen Kampagnen-Recherche und Leads-Modul."}
              </p>
              <div className="sales-board lead-sales-board" aria-label="Sales lead qualification pipeline">
                {pipelineStages.map((stage) => {
                  const stageCards = pipelineCards.filter((card) => card.stageId === stage.id);
                  return (
                    <section className="sales-column" data-stage-id={stage.id} key={stage.id}>
                      <header className="column-head">
                        <div>
                          <h2>{stage.title}</h2>
                          <p>{stage.description}</p>
                        </div>
                        <span className="column-count">{stageCards.length}</span>
                      </header>
                      <div className="card-stack">
                        {stageCards.map((card) => (
                          <a
                            className={`sales-card lead-card ${selected?.id === card.id ? "selected" : ""}`}
                            data-context-item
                            data-context-label={card.companyName}
                            data-context-module="sales"
                            data-context-record-id={card.id}
                            data-context-record-type="pipeline-candidate"
                            data-context-submodule="pipeline"
                            href={`/app/sales/pipeline?locale=${locale}&selectedId=${card.id}`}
                            key={card.id}
                          >
                            <span className="card-topline"><strong>{card.companyName}</strong><span>{card.fit}</span></span>
                            <span className="deal-name">{card.website}</span>
                            <span className="card-context">{card.contact}</span>
                            <span className="card-next"><small>Naechster Qualifizierungsschritt</small>{card.nextStep}</span>
                            <span className={`transition-badge ${card.score >= 75 ? "ready" : "running"}`}>{card.score}</span>
                          </a>
                        ))}
                      </div>
                    </section>
                  );
                })}
              </div>
            </div>
            {selected ? (
              <aside className="bottom-drawer open lead-inspector" aria-label={`${selected.companyName} details`}>
                <div className="drawer-head">
                  <strong>{selected.companyName}</strong>
                  <a href={`/app/sales/pipeline?locale=${locale}`}>{locale === "en" ? "Close" : "Schliessen"}</a>
                </div>
                <section className="inspector">
                  <header className="panel-head">
                    <div><h2>{selected.fit} fit</h2><p>{selected.website}</p></div>
                  </header>
                  <dl className="compact-defs">
                    <dt>{locale === "en" ? "Contact" : "Ansprechpartner"}</dt><dd>{selected.contact}</dd>
                    <dt>{locale === "en" ? "Score" : "Score"}</dt><dd>{selected.score}</dd>
                    <dt>{locale === "en" ? "Next step" : "Naechster Schritt"}</dt><dd>{selected.nextStep}</dd>
                  </dl>
                </section>
              </aside>
            ) : null}
          </main>
        </section>
      </WorkSurface>
    </AppShell>
  );
}
