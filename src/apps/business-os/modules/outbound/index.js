import {
  decodeBase64Utf8,
  extractCompanyRowsFromText,
  normalizeCompanyRow,
  openUniversalImporter,
  parseDelimitedText,
} from '../../shared/universal-importer.js';
import { showBusinessAlert, showBusinessConfirm, showBusinessPrompt } from '../../shared/dialogs.js';
import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';
import {
  configureActiveOutreach,
  loadActiveOutreachData,
  renderActiveOutreachShell,
  handleActiveOutreachAction,
  handleActiveOutreachInput,
  activeOutreachCounts,
} from './active-outreach.js?v=20260527-outbound-review-fixes1';

const BUILD = '20260527-outbound-review-fixes1';
let t = (key, fallback, ...args) => {
  let val = fallback ?? key;
  if (args.length) {
    args.forEach((arg, i) => {
      val = val.replace(`{${i}}`, arg);
    });
  }
  return val;
};

function getTabLabel(key, fallback) {
  return ({
    'message_mail_subject': t('subject', 'Betreff'),
    'message_mail_body': t('email', 'E-Mail'),
    'message_followup_1': t('followup1', 'Follow-up 1'),
    'message_followup_2': t('followup2', 'Follow-up 2'),
    'note_general': t('general', 'Allgemein'),
    'note_next_step': t('nextStep', 'Nächster Schritt'),
  })[key] || fallback;
}

function getResearchFieldLabel(id, fallback) {
  const key = id === 'email' ? 'email_field' : id;
  return t(key, fallback);
}

function getContactFieldLabel(id, fallback) {
  const cleanId = id.replace('.', '_');
  return t(cleanId, fallback);
}

function getContactFieldDesc(id, fallback) {
  const cleanId = id.replace('.', '_');
  return t(`${cleanId}_desc`, fallback);
}
const DEFAULT_CAMPAIGN_ID = 'outbound_default_campaign';
const DEFAULT_CAMPAIGN_NAME = 'Outbound Firmenqualifizierung';
const OUTBOUND_LAYOUT_KEY = 'ctox.businessOs.outbound.columnLayout';
const OUTBOUND_CENTER_SPLIT_KEY = 'ctox.businessOs.outbound.centerSplit';
const OUTBOUND_COL_MIN = Object.freeze({ left: 260, center: 420 });
const OUTBOUND_COL_LEFT_MAX = 760;
const OUTBOUND_CENTER_MIN = Object.freeze({ left: 360, right: 360 });
const OUTBOUND_EXPORT_NS = 'urn:schemas-microsoft-com:office:spreadsheet';
const OUTBOUND_TABLE_RENDER_LIMIT = 250;
const OUTBOUND_BATCH_LIMIT = 100;
const OUTBOUND_BATCH_DEFAULT = 50;
const OUTBOUND_KNOWLEDGE_DOMAIN = 'outbound';
const OUTBOUND_KNOWLEDGE_SKILLBOOK = 'business-os.outbound.campaigns.v1';
const OUTBOUND_KNOWLEDGE_APPEND_CHUNK = 250;
const AUTOMATION_STAGES = Object.freeze({
  company_research: {
    label: 'Company Research',
    description: 'Unternehmensdaten mit CTOX Web Research nachrecherchieren.',
    cta: 'Company Research starten',
  },
  pipeline: {
    label: 'Pipeline vorbereiten',
    description: 'Qualifizierte Unternehmen in die Ansprechpartner-Stufe übernehmen.',
    cta: 'In Pipeline übernehmen',
  },
  contact_research: {
    label: 'Ansprechpartner Research',
    description: 'Relevante Ansprechpartner und Rollen mit CTOX recherchieren.',
    cta: 'Ansprechpartner Research starten',
  },
  lead_qualification: {
    label: 'Lead-Qualifizierung',
    description: 'Recherchierte Ansprechpartner gegen Scope/ICP als Lead qualifizieren.',
    cta: 'Lead-Qualifizierung starten',
  },
});

function getAutomationStageDefinition(stage) {
  const base = AUTOMATION_STAGES[stage];
  if (!base) return null;
  return {
    label: t(`automation_stage.${stage}.label`, base.label),
    description: t(`automation_stage.${stage}.desc`, base.description),
    cta: t(`automation_stage.${stage}.cta`, base.cta),
  };
}
const RESEARCH_FIELD_DEFS = Object.freeze([
  ['legal_form', 'Rechtsform'],
  ['country', 'Land'],
  ['postal_code', 'PLZ'],
  ['city', 'Ort'],
  ['street', 'Straße'],
  ['registry_court', 'Firmenbuchgericht'],
  ['registry_id', 'Register-ID'],
  ['status', 'Status'],
  ['phone', 'Tel'],
  ['fax', 'Fax'],
  ['email', 'E-Mail'],
  ['domain', 'Domain'],
  ['vat_id', 'USt-Id'],
  ['industry_wz', 'Branche (WZ)'],
  ['representative_1', 'Ges. Vertreter 1'],
  ['representative_2', 'Ges. Vertreter 2'],
  ['representative_3', 'Ges. Vertreter 3'],
  ['business_purpose', 'Gegenstand'],
  ['tickers', 'Tickers'],
  ['financials_date', 'Finanzkennzahlen Datum'],
  ['share_capital_eur', 'Stamm-/Grundkapital EUR'],
  ['balance_sheet_total_eur', 'Bilanzsumme EUR'],
  ['profit_eur', 'Gewinn EUR'],
  ['revenue_eur', 'Umsatz EUR'],
  ['equity_eur', 'Eigenkapital EUR'],
  ['employee_count', 'Mitarbeiterzahl'],
]);
const DEFAULT_RESEARCH_FIELD_IDS = Object.freeze(RESEARCH_FIELD_DEFS.map(([id]) => id));
const DEFAULT_RESEARCH_FIELD_IDS_COMPACT = Object.freeze([
  'legal_form', 'country', 'city', 'phone', 'email', 'domain', 'revenue_eur', 'employee_count'
]);
const CONTACT_FIELD_DEFS = Object.freeze([
  ['contact.people', 'Ansprechpartner', 'Gefundene relevante Personen, Rollen und kurze Einordnung.'],
  ['contact.role', 'Rolle', 'Funktion, Seniorität und Verantwortungsbereich der wichtigsten Ansprechpartner.'],
  ['contact.email', 'E-Mail', 'Öffentlich belegbare E-Mail-Adressen der Ansprechpartner.'],
  ['contact.linkedin', 'LinkedIn', 'Öffentlich belegbare LinkedIn- oder Profil-URLs.'],
  ['contact.phone', 'Telefon', 'Öffentlich belegbare Direktwahl oder zentrale Telefonnummer für den Kontakt.'],
  ['contact.fit', 'Kontakt Fit', 'Warum diese Person für den Campaign Scope relevant ist.'],
  ['contact.status', 'Kontakt', 'Status der Ansprechpartner-Qualifizierung.'],
  ['lead.reason', 'Lead Grund', 'Warum der Ansprechpartner als Lead qualifiziert oder abgelehnt wurde.'],
  ['lead.status', 'Lead', 'Status der Lead-Qualifizierung.'],
]);
const DEFAULT_CONTACT_FIELD_IDS = Object.freeze(['contact.people', 'contact.status', 'lead.status']);
const DEFAULT_CONTACT_FIELD_IDS_COMPACT = Object.freeze(['contact.people', 'contact.status']);


const DEFAULT_API_URL = 'https://leadgenservice.ngrok.io';
const DEFAULT_TOKEN_ID = '2QjI3u9txqyB9chbDt1rUJF5th4_31BsnCZFuiZTUoeVMeyui';

const DEFAULT_ICP_CORE = `Wholix ist ein anpassbares CRM + Light-ERP für Solo-Unternehmer bis KMU. Es bündelt Vertrieb, Projekte und Service in einem System und erlaubt eine flexible Datenstruktur (eigene Objekte wie Immobilien/Objekte, Aufträge, Verträge, Tickets), Relationen, Felder und Ansichten. Teams oder einzelne Nutzer erhalten rollenbasierte Menüs/Workspaces. Pipelines (Board/Liste/Kalender) für Sales, Service und Projekte sowie Kollaboration (Kommentare, Aufgaben, Erwähnungen) sind integriert. Automationen und KI-Assistenz unterstützen beim Zusammenfassen, Priorisieren und bei Vorschlägen für nächste Schritte. Reporting über gespeicherte, filterbare Ansichten. Einführung in Tagen statt Monaten ohne Großprojekt. Kommerziell: nutzungsbasierte, paketierte Preise (keine Abrechnung pro User) zur Konsolidierung mehrerer Tools und zur besseren Kostenkontrolle.`;

const DEFAULT_CHECKLIST = `B2B-Ansprache klar erkennbar (Leistungs-/Branchen-/Lösungsseiten).
Objekt-/Auftragslogik sichtbar (z. B. Immobilien/Exposés, Aufträge, Tickets, Wartung, Projekte).
Navigation/Content zu Vertrieb (Pipeline/Leads/Anfragen), Projekten/Leistungen und Service/Support.
Hinweise auf bestehende Tool-Landschaft (CRM, Projekt-/Ticket-Tool, Tabellen/Excel) oder geplanten Wechsel.
Kontakt- oder Termin-Widgets (Lead-Erfassung), ggf. Login-/Kundenbereich.
Karriere-/Teamseite mit Rollen wie Sales/CRM, Projektleitung/PMO, Service/Dispatcher, Techniker.
Mehrere Standorte/Regionen oder mobile Teams (z. B. Außendienst/Field Service).
Referenzen im KMU-Umfeld (DACH), mehrsprachige Site optional.
Preis-/Kostenargumente gegen Seat-Modelle (viele Nutzerrollen, interne/externe Mitnutzer).
Bei Maklern: Hinweise auf Objektaufnahme, Exposé-/Terminkoordination, Nachbetreuung/Service.`;

const DEFAULT_CTA = 'Soll ich Ihnen 12 potenzielle Kunden inklusive der fertigen Anschreiben zusenden?';

const DEFAULT_SIGNATURE = `Beste Grüße
Max Mustermann`;

const ICP_PROMPT_TEMPLATE = `Du bist ein Sales Specialist und berätst Unternehmen, ideale Kundenprofile zu definieren, Ich gebe dir hierfür mehre <Muster ICPs> Beschreibungen, an den du dich orientieren kannst, sowie eine <Taxonomie> als Hilfe. Deine Aufgabe ist, es aus der <Homepage des Kunden> einen ICP für den Kunden zu entwickeln. Folge dazu der <Instruktion> Schritt für Schritt. Du gibst ein JSON Objekt zurück, ohne weitere Kommentare

<Instruktion>:
Lies die <Homepage des Kunden> und finde in der <Taxonomie> Nummern für mögliche Kunden des eigenen Angebots. Liste die passenden Taxonomie-Nummer mit Bezeichnung in der Sprache der <Homepage des Kunden> auf.

Formuliere eine Produkt und Dienstleistungsberschreibung. Hol dir dazu Inspiration von den <Muster ICPs>: Es kommt darauf an, dass der Match anhand von öffentlichen Informationen durchgeführt werden kann.Gib ein Json aus mit Rechtsform, Ausrichtung (B2B/B2C). Mitarbeiter (min) Mitarbeiter (max). Umsatz (min), Umsatz (max), der Taxonomie mit dem Array der Nummer sowie der Beschreibung. sowie eine Checkliste für den Abgleich mit der Landingpage des potentiellen Kunden, sowie eine Produkt&Dienstleistungs beschreibung auf Basis der <Homepage des Kunden>

<Muster ICPs>:
{"legal_form": null,"orientation": "B2B","employees_num": { "min": 100, "max": 50000 },"fin_revenue": { "min": 10000000, "max": 10000000000 },"Taxonomie": [{ "id": "4.1", "name": "SaaS" },{ "id": "4.2", "name": "On-Prem-Software-Lizenz + Wartung" },{ "id": "4.4", "name": "PaaS/IaaS" },{ "id": "4.12", "name": "Data/Analytics-SaaS (BI, MLOps, Observability)" },{ "id": "4.13", "name": "Managed SaaS/Managed Service zu Software" },
{ "id": "7.2", "name": "IT-Beratung/Systemhaus/Integration" },
{ "id": "7.9", "name": "Managed Services/MSP/MSSP (inkl. Cybersecurity)" },
{ "id": "11.9", "name": "Rechenzentren/Colocation" },
{ "id": "1.8", "name": "EMS/Elektronikfertiger" },
{ "id": "1.4", "name": "Komponenten-/Zulieferer (B2B)" },
{ "id": "1.1", "name": "OEM/Hersteller (eigene Marke)" },
{ "id": "11.1", "name": "Energieversorger (Tarifmodell)" },
{ "id": "11.3", "name": "Netz-/Infrastruktur-Betreiber (Zugang/Fees)" },
{ "id": "11.4", "name": "Telekommunikation/ISP (Tarife)" },
{ "id": "11.10", "name": "Masten/Tower-Co/Passive Infra" },
{ "id": "12.1", "name": "Finanzdienstleister – Bank/Fintech (Zins/Fees/Spread)" },
{ "id": "12.4", "name": "Versicherung (Versicherer/MGA/Insurtech)" },
{ "id": "8.2", "name": "Gesundheitsdienstleister (Praxis/Klinik/Therapie)" },
{ "id": "8.4", "name": "Diagnostik/Labore/Bildgebung" },
{ "id": "16.3", "name": "Öffentliche Einrichtung/Behörde – Gebühren/Steuern" },
{ "id": "10.3", "name": "Logistik: Lager/Fulfillment" }
],
"Checkliste_Landingpage": ["B2B-/B2G-Ansprache eindeutig? (\\"Für Unternehmen\\", Branchen-/Lösungsseiten)","Branche klar benannt und in obiger Taxonomie vertreten?","Hinweise auf IT-/Service-/Support-Prozesse (Begriffe: Service Desk, Tickets, SLA, Störung, ITSM) vorhanden?","Technologie-/Tool-Erwähnungen sichtbar (z. B. Microsoft/Active Directory, SAP/ERP, ServiceNow/Jira/OTRS, Monitoring/Alerting)?","Zertifikate/Normen-Seite vorhanden (z. B. ISO 9001/27001, BSI/DSGVO, IPC, ASiG/BGM)?","Komplexität sichtbar (mehrere Standorte, 'bundesweit', international, 24/7, Rechenzentrum, Cloud/Hybrid)?","Mehrsprachige Website (z. B. DE/EN) vorhanden?","Karrierebereich mit IT-Rollen (z. B. IT-Admin, DevOps, System Engineer) sichtbar?","Partner-/Technologie- oder Referenzseite vorhanden?","Content zu Monitoring/Automatisierung/IT-Betrieb/Qualität (Blog, Magazin, Whitepaper, Webinare) vorhanden?","Kunden-/Branchenreferenzen oder Zertifizierungs-Logos (z. B. DEKRA/UL) sichtbar?","Eigenes Service-/Kundenportal oder Login/Support-Bereich verlinkt?"],
"Produkt_und_Dienstleistungsbeschreibung": "MILTON ist eine KI-gestützte Plattform für IT-Automatisierung im Service-Desk und IT-Betrieb. Sie liest Tickets (auch aus E-Mail), korreliert diese mit Monitoringdaten, führt Standardaufgaben und Störungsbehebungen automatisch aus und dokumentiert revisionssicher im vorhandenen System. Einsatz in Cloud-, On-Prem- und Hybrid-Umgebungen; kompatibel mit gängigen Systemen/Applikationen; Prozesse und Datenhoheit bleiben erhalten; konform zu ISO 20000, ISO 27001, BSI IT-Grundschutz, ISAE 3402 und DSGVO. Nutzen: hohe Automatisierungsquote bei IT-Regelaufgaben, weniger Störungen, deutlich kürzere Ticket-Bearbeitungszeit."}

{
  "Rechtsform": ["Einzelunternehmen", "e.K.", "GmbH", "UG (haftungsbeschränkt)", "GmbH & Co. KG", "AG", "SE"],
  "Ausrichtung": "B2B",
  "Mitarbeiter": { "min": 1, "max": 500 },
  "Umsatz": { "min": 50000, "max": 100000000 },
  "Taxonomie": [
    { "id": "6.6", "name": "Vermittler/Makler" },
    { "id": "14.2", "name": "Immobilien – Bestandshalter/Vermietung" },
    { "id": "14.4", "name": "Property-/Asset-/Facility-Management (Eigentümer-nah)" },
    { "id": "7.3", "name": "Kreativ-/Marketing-Agentur" },
    { "id": "7.1", "name": "Unternehmensberatung/Management Consulting" },
    { "id": "7.2", "name": "IT-Beratung/Systemhaus/Integration" },
    { "id": "7.7", "name": "Ingenieurwesen/Planung/Architektur" },
    { "id": "7.9", "name": "Managed Services/MSP/MSSP (inkl. Cybersecurity)" },
    { "id": "9.1", "name": "Handwerk/Installation" },
    { "id": "9.6", "name": "Reparatur/Werkstatt/Wartung" },
    { "id": "9.3", "name": "Facility-/Gebäudemanagement" },
    { "id": "4.1", "name": "SaaS" },
    { "id": "4.2", "name": "On-Prem-Software-Lizenz + Wartung" },
    { "id": "4.13", "name": "Managed SaaS/Managed Service zu Software" }
  ],
  "Checkliste_Landingpage": [
    "B2B-Ansprache klar erkennbar (Leistungs-/Branchen-/Lösungsseiten).",
    "Objekt-/Auftragslogik sichtbar (z. B. Immobilien/Exposés, Aufträge, Tickets, Wartung, Projekte).",
    "Navigation/Content zu Vertrieb (Pipeline/Leads/Anfragen), Projekten/Leistungen und Service/Support.",
    "Hinweise auf bestehende Tool-Landschaft (CRM, Projekt-/Ticket-Tool, Tabellen/Excel) oder geplanten Wechsel.",
    "Kontakt- oder Termin-Widgets (Lead-Erfassung), ggf. Login-/Kundenbereich.",
    "Karriere-/Teamseite mit Rollen wie Sales/CRM, Projektleitung/PMO, Service/Dispatcher, Techniker.",
    "Mehrere Standorte/Regionen oder mobile Teams (z. B. Außendienst/Field Service).",
    "Referenzen im KMU-Umfeld (DACH), mehrsprachige Site optional.",
    "Preis-/Kostenargumente gegen Seat-Modelle (viele Nutzerrollen, interne/externe Mitnutzer).",
    "Bei Maklern: Hinweise auf Objektaufnahme, Exposé-/Terminkoordination, Nachbetreuung/Service."
  ],
  "Produkt_und_Dienstleistungsbeschreibung": "Wholix ist ein anpassbares CRM + Light-ERP für Solo-Unternehmer bis KMU. Es bündelt Vertrieb, Projekte und Service in einem System und erlaubt eine flexible Datenstruktur (eigene Objekte wie Immobilien/Objekte, Aufträge, Verträge, Tickets), Relationen, Felder und Ansichten. Teams oder einzelne Nutzer erhalten rollenbasierte Menüs/Workspaces. Pipelines (Board/Liste/Kalender) für Sales, Service und Projekte sowie Kollaboration (Kommentare, Aufgaben, Erwähnungen) sind integriert. Automationen und KI-Assistenz unterstützen beim Zusammenfassen, Priorisieren und bei Vorschlägen für nächste Schritte. Reporting über gespeicherte, filterbare Ansichten. Einführung in Tagen statt Monaten ohne Großprojekt. Kommerziell: nutzungsbasierte, paketierte Preise (keine Abrechnung pro User) zur Konsolidierung mehrerer Tools und zur besseren Kostenkontrolle."
}


<Taxonomie>:
{
"taxonomy": [
{
"id": "1",
"name": "Produktion & Beschaffung",
"subcategories": [
{ "id": "1.1", "name": "OEM/Hersteller (eigene Marke)" },
{ "id": "1.2", "name": "Auftragsfertiger/Contract Manufacturing" },
{ "id": "1.3", "name": "White-Label/Private-Label" },
{ "id": "1.4", "name": "Komponenten-/Zulieferer (B2B)" },
{ "id": "1.5", "name": "3D-Druck/On-Demand-Manufacturing" },
{ "id": "1.6", "name": "ODM/Design-&-Fertigung aus einer Hand" },
{ "id": "1.7", "name": "Lizenz-/Technologietransfer (Lizenzfertigung)" },
{ "id": "1.8", "name": "EMS/Elektronikfertiger" },
{ "id": "1.9", "name": "Co-Packing/Co-Manufacturing (Abfüllung/Verpackung)" },
{ "id": "1.10", "name": "Refurbishment/Remanufacturing/Recycling" },
{ "id": "1.11", "name": "Fabless/Foundry (Halbleiter)" }
]
},
{
"id": "2",
"name": "Handel & Retail",
"subcategories": [
{ "id": "2.1", "name": "Großhandel/Distributor" },
{ "id": "2.2", "name": "Import/Export-Handel" },
{ "id": "2.3", "name": "Stationärer Einzelhandel" },
{ "id": "2.4", "name": "E-Commerce (eigener Online-Shop, D2C/B2C)" },
{ "id": "2.5", "name": "B2B-E-Procurement/Kataloglieferant" },
{ "id": "2.6", "name": "Dropshipping-Shop" },
{ "id": "2.7", "name": "Abo-Box (physisch)" },
{ "id": "2.8", "name": "Flash-Sale/Off-Price/Rabatt-Club" },
{ "id": "2.9", "name": "Print-on-Demand/Mass Customization" },
{ "id": "2.10", "name": "Recommerce/Second-Hand/Refurbished" },
{ "id": "2.11", "name": "Social Commerce/Live-Shopping" },
{ "id": "2.12", "name": "Automaten/Vending/Unattended Retail" },
{ "id": "2.13", "name": "Omnichannel (Click & Collect/Ship-from-Store)" }
]
},
{
"id": "3",
"name": "Franchise, Lizenz & IP",
"subcategories": [
{ "id": "3.1", "name": "Franchise-Geber" },
{ "id": "3.2", "name": "Lizenzierung/IP-Vergabe" },
{ "id": "3.3", "name": "Technologie-/Patentlizenz (ohne Marke)" },
{ "id": "3.4", "name": "Content-/Formatlizenz (TV/Medien/Character)" },
{ "id": "3.5", "name": "Markenlizenz/Co-Branding" }
]
},
{
"id": "4",
"name": "Software & Digitale Dienste",
"subcategories": [
{ "id": "4.1", "name": "SaaS" },
{ "id": "4.2", "name": "On-Prem-Software-Lizenz + Wartung" },
{ "id": "4.3", "name": "API-/Usage-based-Service" },
{ "id": "4.4", "name": "PaaS/IaaS" },
{ "id": "4.5", "name": "Open-Source + Support/Enterprise Edition" },
{ "id": "4.6", "name": "Mobile App – In-App-Kauf" },
{ "id": "4.7", "name": "Freemium" }
]
}
]
}

<Homepage des Kunden>:
\${homepage}`;

const MESSAGE_PROMPT_TEMPLATE = `Du bist ein Sales Specialist und berätst Unternehmen, bessere Kaltakquise-E-Mails zu formulieren.
Ich gebe dir hierfür mehrere <Muster E-Mails Positiv> und <Muster E-Mails Negativ> als Referenz sowie eine <Negativ-Checkliste> als Hilfe.
Deine Aufgabe ist es, aus der <Produkt und Dienstleistungsbeschreibung>, und falls vorhanden auch <zusätzliche Anweisung>: enthalten kann, z.B. für CTA und Absender, der <Checkliste für Infos von Kunden>, <Infos zum Kontakt> und der <Infos von Hompage des Kunden> ein relevantes Anschreiben zu entwickeln.
Folge dazu der <Instruktion> Schritt für Schritt.

<Instruktion>:
Führe alle Schritte nacheinander aus.

Schritt 1:
Lies die <Infos von Hompage des Kunden>, die <Checkliste für Infos von Kunden> und die <Produkt und Dienstleistungsbeschreibung>. Erstelle eine strukturierte Übersicht mit vier Teilen:

A) Produktbeschreibung (nur aus der Homepage, nicht erfinden):
- Ein-Satz-Wertversprechen in Klartext (kein Jargon).
- 3 wichtigste Nutzen für die anvisierte Zielrolle (nicht Features).
- 1–2 belastbare Belege (Zahl, Referenz, Zertifikat), falls vorhanden.
- Einsatzform (SaaS/On-Prem/Hybrid), Integrationen, Einführungsaufwand (nur wenn klar genannt).

B) Kundenkontext (nur aus Touchpoints, nicht erfinden):
- Rolle/Team, Prioritäten, KPI-Signale (z. B. offene Stellen, OKRs, Blogthemen, Finanzzahlen).
- Aktuelle Tools/Prozesse/Constraints, die sich aus Touchpoints ableiten lassen (z. B. „On-Prem only“, „Salesforce im Einsatz“).
- 2–3 belegbare Schmerzpunkte/Chancen (z. B. „doppelte Datenpflege“, „Ticketflut im Service“, „ungepflegte Fassade“).

C) Anrede- & Tonalitätsbestimmung:
- Analysiere Sprache, Stil und Kultur der Homepage und Touchpoints (z. B. „per Du“ im Karrierebereich, lockere Blogposts, konservative B2B-Sprache).
- Entscheide: soll die Ansprache in den Mails mit „Sie“ (formell) oder „du“ (informell) erfolgen?
- Leite außerdem die Tonalität ab: konservativ, locker, technisch, pragmatisch.
- Berüchsichtige und priorisiere <zusätzliche Anweisung>, falls vorhanden.
- Dokumentiere diese Entscheidung klar mit Begründung und mindestens 1 Belegstelle aus den Quellen (z. B. „Karriereseite spricht Bewerber durchgehend mit Du an → ‘du’ wählen“).

D) Verbindungspunkte (Hypothesen aus A + B):
- Formuliere pro Verbindungspunkt:
  - *Touchpoint*: konkrete Quelle (z. B. Stellenausschreibung, Blog, Finanzbericht, Angaben im Social Media Profil).
  - *Bezug zum Produkt*: welche Produktfunktion/Leistung adressiert das.
  - *Pain oder Chance*: welches Problem oder Ziel steckt dahinter.
  - *Formulierungsvorschlag*: 1 Satz, wie du das in einer Mail ansprechen würdest.
  - *Produkt Pitch*: Überlege, inwwieweit ein direker Produkt/Dienstleistungs Pitch besser ist und erstmal nur eine rein problemlösungsorientierte Ansprache.
- 3 solcher Verbindungspunkte, damit Variationen entstehen können.

Schritt 2:
Formuliere drei sehr unterschiedliche Kaltakquise-E-Mail-Entwürfe auf Basis der Verbindungspunkte aus Schritt 1.
- Verwende die in Schritt 1C abgeleitete Anspracheform (Du oder Sie) und Tonalität.
- Länge: 60–120 Wörter, 4–6 Sätze.
- Struktur je Entwurf:
  1) Betreff: spezifisch + nüchtern, Bezug auf Pain/Use Case.
  2) Erster Satz: echter Touchpoint-Verweis (konkret, belegbar).
  3) 1–2 Sätze: Problem → Nutzen (in Sprache der Zielrolle; kein Jargon).
  4) Optional 1 Mini-Beleg (nur wenn aus Inputs ableitbar; keine Fantasiezahlen).
  5) Leichter CTA (eine Frage, kein Termin-Push, nur 1 CTA).
- Variiere Hook/CTA bewusst (z. B. „Insight anbieten“, „Beispiel schicken“, „kurz prüfen“).
- Jede Mail muss explizit 1 Produktnutzen + 1 Kunden-Touchpoint benennen.
- Weiche von der Struktur ab, falls das <zusätzliche Anweisung> verlangt.

Schritt 3:
Überprüfe die drei Entwürfe anhand der <Negativ-Checkliste>.
- Schreibe pro Variante 2–3 Sätze Kritik (konkret, textstellenbasiert).
- Leite daraus 3 präzise Verbesserungsanweisungen für das Finale ab (imperativ, z. B. „Touchpoint in Satz 1 konkretisieren: ‘Blogbeitrag vom 12.05.: …’“; „Zahl entfernen, da unbelegt“).

Schritt 4:
Schreibe EIN finales Anschreiben (Betreff + Text), das:
- die bestbewerteten Elemente übernimmt,
- alle Verbesserungsanweisungen aus Schritt 3 umsetzt,
- ausschließlich belegbare Aussagen enthält,
- max. 110 Wörter umfasst.
- Anrede & Tonalität: exakt so, wie in Schritt 1C bestimmt.

<Muster E-Mails Positiv>:
Betreff: Eure KI-Studie als YouTube-Serie für mehr warme Leads
Hallo Kathrin,
eure KI-Traffic-Studie (Blog, 31.07.2025) zeigt starke Expertise. Als kurze YouTube-Serie würden die Kern-Insights noch mehr Vertrauen stiften und regelmäßig warme Anfragen in eure Erstberatung lenken. Das setzen wir für Agenturen um: Themenplan, einfache Skripte, Titel/Thumbnails und Kanal-Optimierung aus einer Hand. Beleg: Wir haben 600+ Business-YouTube-Kanäle begleitet.
Offen für ein 20‑minütiges, kostenloses YouTube-Strategiegespräch, um Potenziale eurer bestehenden Inhalte zu heben?

Betreff: YouTube Ads habt ihr – wie nutzt ihr organisch für warme Anfragen?
Hi Maren,
auf eurer Leistungsseite zu YouTube Werbung und als Google Premium Partner setzt ihr Paid stark ein. Ergänzend sorgt ein eigener, organischer YouTube‑Kanal für Vertrauen vor dem Erstgespräch – mit regelmäßiger Reichweite statt kurzer Peaks. Wir richten das schlank ein: Analyse, Setup und Equipment, einfache Skripte sowie passende Themen, Titel und Thumbnails. Beleg: 600+ betreute Business‑Kanäle in 5+ Jahren.
Hast du Lust auf ein 15‑minütiges Beratungsgespräch, in dem ich dir 2–3 konkrete Formatideen für Hanseranking zeige?

<Muster E-Mails Negativ>:
[
  { "Nr": 1, "Typ": "Selbstdarstellung überladen", "Betreff": "Kurze Vorstellung – Softwarefirma aus Köln", "Text": "Hallo Herr Meier, mein Name ist ... Wann hätten Sie Zeit?" },
  { "Nr": 2, "Typ": "Pseudo-persönlich, austauschbar", "Betreff": "Kurze Frage zu Ihrer Website", "Text": "Hallo Frau Keller, mir ist aufgefallen, dass ..." }
]

<Negativ-Checkliste>:
- Generische Aussagen („Wir helfen Unternehmen wie Ihrem…“)
- Zu viel „Wir“, zu wenig „Sie“
- Überlänge, Romane, mehrere CTAs
- Aggressiver Druck / Dringlichkeit
- Falsche Personalisierung
- Jargon / Buzzwords
- Unbelegte Superlative
- Angst-Rhetorik
- Emojis / flapsiger Humor

<Produkt und Dienstleistungsbeschreibung>:
{{Produkt_und_Dienstleistungsbeschreibung}}

<zusätzliche Anweisung>:
{{CTA}}

<Checkliste für Infos von Kunden>:
{{Checkliste_Landingpage}}

<Infos von Hompage des Kunden>:
{{Landing_Page_Inhalt}}

<Infos zum Kontakt>:
{{Kontakt_info}}`;

const EXTRACT_EMAIL_PROMPT = `Du bist ein präziser Extraktions-Parser. Du erhältst unten die KOMPLETTE Ausgabe eines vorherigen LLM-Durchlaufs (Message_Prompt), die eine mehrstufige Analyse (Schritt 1–3) und am Ende EIN finales Anschreiben (Schritt 4) enthält.

AUFGABE:
- Finde ausschließlich das finale Anschreiben aus Schritt 4 (oft überschrieben mit "Schritt 4", "Final", "Finales Anschreiben" o.ä.).
- Extrahiere daraus den Betreff und den eigentlichen E-Mail-Text.
- Kürze, wo möglich; streiche Wiederholungen.
- Inhalte NICHT verändern oder erweitern. Nichts Neues erfinden. Aber wärmer und empathischer und nicht so technisch formulieren.
- Keine Anführungszeichen oder wörtlichen Zitate aus der Empfänger-Seite; paraphrasiere kurz und sinngemäß, damit es nicht hölzern oder aufgezählt wirkt
- Verwende einfache, aktive Sätze. Vermeide Substantivketten.
- Leite passende und kurze Texte für Folow Up Nachrichten ab. Der erste sollte nicht zu aufdringlich sein, aber als CTA klar und höflich, charmant darauf verweisen, ob und warum der Lead sich mit dem Vorschlag beschäftigen möchte bzw. sollte? Bei zweiten Followup dann charmant einfodern, dass eine Response helps, ob sich die Beschäftigung mit dem Theam überhaupt lohnt mit schon der Erwartung, dass sich der Lead hierzu äußert, bzw. warum er kein Interesse hat?
- Gib als EINZIGE Ausgabe ein gültiges JSON-Objekt mit genau diesen vier Schlüsseln zurück:
  {
    "Betreff": "<Betreff ohne das Label 'Betreff:'>",
    "Text": "<E-Mail-Text, Zeilenumbrüche beibehalten>",
    "FollowUp1": "<FollowUp1-Text, Zeilenumbrüche beibehalten>",
    "FollowUp2": "<FollowUp2-Text, Zeilenumbrüche beibehalten>"
  }

REGELN:
1) Nutze nur den Abschnitt des finalen Anschreibens (Schritt 4). Erfinde keine Inhalte.
2) Falls im finalen Abschnitt kein explizites "Betreff:"-Label steht:
   - Nimm die erste Zeile als Betreff (bis zum ersten Zeilenumbruch).
   - Der restliche Teil ist der E-Mail-Text.
3) Entferne führende Labels wie "Betreff:" oder "Subject:" aus dem Betreff.
4) Trimme führende/abschließende Leerzeichen. Erhalte Zeilenumbrüche im Text.
5) Sprache, Tonalität und Schreibweise genau beibehalten (keine Umschreibungen).
6) Output muss striktes JSON sein: keine zusätzlichen Kommentare, kein Markdown, keine Codefences, keine nachgestellten Hinweise, keine Backticks, keine BOM.
7) Escape ggf. Anführungszeichen innerhalb der JSON-Strings korrekt. Keine trailing commas.

EINGABE (Rohtext der Message_Prompt-Antwort):
<<<BEGIN_MESSAGE_PROMPT_OUTPUT
{{Message_Prompt_Response}}
END_MESSAGE_PROMPT_OUTPUT>>>`;

const state = {
  ctx: null,
  campaigns: [],
  sources: [],
  companies: [],
  pipeline: [],
  runs: [],
  commands: [],
  queueTasks: [],
  selectedCampaignId: '',
  selectedCompanyId: '',
  selectedPipelineId: '',
  activeView: 'companies',
  filter: 'all',
  search: '',
  tableFilters: {},
  tableSort: null,
  editingCampaignId: '',
  knowledgeProjectionDisabled: false,
  knowledgeProjectionSignature: '',
  refreshTimer: null,
  knowledgeWatchTimer: null,
  centerRenderTimer: null,
  operationalRefreshPending: false,
  lastOperationalRefreshMs: 0,
  cleanup: [],
  centerResizeCleanup: null,
  researchSettingsActiveTab: 'columns',
  tempResearchSettings: null,
  generatingOutreach: new Map(),
  isScrolling: false,
  scrollTimeout: null,
  pendingRenderAfterScroll: false,
  statusFilter: '',
  tagFilter: '',
  lastTbodyHtml: '',
  lastActiveColsKey: '',
  outreachView: false,
};

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, {});
  t = (key, fallback, ...args) => {
    let val = messages[key] ?? fallback ?? key;
    if (args.length) {
      args.forEach((arg, i) => {
        val = val.replace(`{${i}}`, arg);
      });
    }
    return val;
  };
  if (!state.activeMsgByContact) state.activeMsgByContact = new Map();
  if (!state.activeNoteByContact) state.activeNoteByContact = new Map();
  if (!state.viewMode) state.viewMode = 'expanded';
  state.statusFilter = '';
  state.tagFilter = '';
  state.lastTbodyHtml = '';
  if (!state.viewMode) state.viewMode = 'expanded';

  state.isScrolling = false;
  state.scrollTimeout = null;
  state.pendingRenderAfterScroll = false;

  await ensureStyles();
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  await ensureDefaultCampaign();
  configureActiveOutreach({ state, t, escapeHtml, rerender: () => render(true) });
  await loadAll({ hydrateKnowledge: false });
  await loadActiveOutreachData().catch((error) => console.warn('[outbound] active outreach load failed', error));
  wireEvents(ctx.host);
  wireRealtime();

  const scrollListener = (event) => {
    const scrollContainer = event.target.closest('.outbound-table-scroll-unified');
    if (scrollContainer) {
      state.isScrolling = true;
      if (state.scrollTimeout) window.clearTimeout(state.scrollTimeout);
      state.scrollTimeout = window.setTimeout(() => {
        state.isScrolling = false;
        state.scrollTimeout = null;
        if (state.pendingRenderAfterScroll) {
          state.pendingRenderAfterScroll = false;
          render();
        }
      }, 500);
    }
  };
  ctx.host.addEventListener('scroll', scrollListener, { capture: true, passive: true });
  state.cleanup.push(() => {
    ctx.host.removeEventListener('scroll', scrollListener, true);
    if (state.scrollTimeout) window.clearTimeout(state.scrollTimeout);
  });

  const resizeCleanup = setupOutboundColumnResizing();
  if (resizeCleanup) state.cleanup.push(resizeCleanup);
  render();
  ensureCampaignKnowledge(selectedCampaign())
    .then(async () => {
      await loadAll({ hydrateKnowledge: false });
      render();
    })
    .catch((error) => {
      console.warn('[outbound] selected campaign knowledge setup failed', error);
    });
  return () => {
    state.centerResizeCleanup?.();
    state.centerResizeCleanup = null;
    if (state.centerRenderTimer) window.clearTimeout(state.centerRenderTimer);
    state.centerRenderTimer = null;
    closeHiddenCompaniesPanel();
    state.cleanup.forEach((fn) => fn?.());
    state.cleanup = [];
    ctx.host.replaceChildren();
  };
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  return doc.body.innerHTML;
}

function campaignKnowledgeRefs(campaign) {
  const slug = slugId(campaign?.id || campaign?.name || 'campaign');
  const current = campaign?.payload?.knowledge || {};
  return {
    domain: current.domain || OUTBOUND_KNOWLEDGE_DOMAIN,
    skillbookId: current.skillbook_id || OUTBOUND_KNOWLEDGE_SKILLBOOK,
    runbookId: current.runbook_id || `business-os.outbound.${slug}.runbook.v1`,
    companiesKey: current.companies_table_key || `campaign_${slug}_companies`,
    contactsKey: current.contacts_table_key || `campaign_${slug}_contacts`,
    runsKey: current.runs_table_key || `campaign_${slug}_research_runs`,
  };
}

function slugId(value) {
  const normalized = String(value || '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9_.-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 80);
  return normalized || `campaign_${Date.now()}`;
}

async function knowledgeCommand(args) {
  if (!state.ctx?.commandBus?.dispatch) {
    throw new Error('RxDB command bus is not available');
  }
  const commandId = `cmd_knowledge_${crypto.randomUUID()}`;
  const startedAtMs = Date.now();
  const dispatched = await state.ctx.commandBus.dispatch({
    id: commandId,
    module: 'knowledge',
    type: 'knowledge.command',
    record_id: commandId,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: 'Knowledge command',
      args,
    },
    client_context: {
      source_module: 'outbound',
      command_path: 'knowledge',
    },
  });
  if (dispatched?.status && dispatched.status !== 'pending_sync') return dispatched;
  const projection = await waitForBusinessCommandProjection(commandId, startedAtMs);
  if (!projection) {
    throw new Error(`Knowledge command ${commandId} was not acknowledged by the native RxDB peer`);
  }
  if (projection.status === 'failed') {
    throw new Error(projection.error || projection.result?.error || `Knowledge command ${commandId} failed`);
  }
  return projection.result || projection;
}

async function ensureKnowledgeDataTable(refs, key, title, description) {
  const describeArgs = ['data', 'describe', '--domain', refs.domain, '--key', key];
  try {
    return await knowledgeCommand(describeArgs);
  } catch (_) {
    return knowledgeCommand([
      'data', 'create',
      '--domain', refs.domain,
      '--key', key,
      '--source-system', 'business-os.outbound',
      '--title', title,
      '--description', description,
    ]);
  }
}

async function ensureCampaignKnowledge(campaign) {
  if (!campaign?.id || !nativeRxdbSyncReady()) return null;
  const refs = campaignKnowledgeRefs(campaign);
  await ensureKnowledgeDataTable(refs, refs.companiesKey, `${campaign.name} · Unternehmen`, 'Outbound Campaign Firmen, Importjobs, Qualifikation und Unternehmens-Research.');
  await ensureKnowledgeDataTable(refs, refs.contactsKey, `${campaign.name} · Ansprechpartner`, 'Outbound Campaign Ansprechpartner- und Lead-Qualifikation.');
  await ensureKnowledgeDataTable(refs, refs.runsKey, `${campaign.name} · Research Runs`, 'Outbound Campaign Research-Auftraege, Status und CTOX Command-Referenzen.');
  await knowledgeCommand([
    'skill', 'add-skillbook',
    '--id', refs.skillbookId,
    '--title', 'Business OS Outbound Campaigns',
    '--version', 'v1',
    '--mission', 'Outbound Campaigns führen Firmenquellen über Unternehmensqualifikation, Ansprechpartner-Recherche und Lead-Qualifikation.',
    '--runtime-policy', 'Nutze Knowledge DataFrames als einzige record-shaped Wissensquelle. Outbound speichert nur Workflow-State und Referenzen.',
    '--workflow-backbone', 'source-import,company-research,pipeline-contact-research,lead-qualification',
    '--linked-runbooks', refs.runbookId,
  ]);
  await knowledgeCommand([
    'skill', 'add-runbook',
    '--id', refs.runbookId,
    '--skillbook', refs.skillbookId,
    '--title', `${campaign.name} Campaign Runbook`,
    '--version', 'v1',
    '--problem-domain', 'outbound-campaign',
    '--status', 'active',
    '--item-labels', 'CAMPAIGN-SCOPE,DATAFRAMES,FUNNEL-RUNS',
  ]);
  await knowledgeCommand([
    'skill', 'add-item',
    '--id', `${refs.runbookId}.scope`,
    '--runbook', refs.runbookId,
    '--skillbook', refs.skillbookId,
    '--label', 'CAMPAIGN-SCOPE',
    '--title', 'Campaign Scope und Datenvertrag',
    '--problem-class', 'outbound-campaign-scope',
    '--chunk-text', campaignRunbookChunk(campaign, refs),
    '--version', 'v1',
    '--status', 'active',
    '--skip-embedding',
  ]);
  const payload = {
    ...(campaign.payload || {}),
    knowledge: {
      domain: refs.domain,
      skillbook_id: refs.skillbookId,
      runbook_id: refs.runbookId,
      companies_table_key: refs.companiesKey,
      contacts_table_key: refs.contactsKey,
      runs_table_key: refs.runsKey,
    },
  };
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaign.id, {
    payload,
    updated_at_ms: Date.now(),
  });
  return refs;
}

function campaignRunbookChunk(campaign, refs) {
  return [
    `Campaign: ${campaign.name}`,
    `Market: ${campaign.market || 'DACH'}`,
    `Scope/ICP: ${campaign.payload?.scope || campaign.objective || 'nicht gesetzt'}`,
    '',
    'Record-shaped Knowledge:',
    `- Companies DataFrame: ctox knowledge data describe --domain ${refs.domain} --key ${refs.companiesKey}`,
    `- Contacts DataFrame: ctox knowledge data describe --domain ${refs.domain} --key ${refs.contactsKey}`,
    `- Research Runs DataFrame: ctox knowledge data describe --domain ${refs.domain} --key ${refs.runsKey}`,
    '',
    'Funnel:',
    '1. Importjobs anlegen, daraus Unternehmen extrahieren und nur Unternehmen in den Companies DataFrame schreiben.',
    '2. Unternehmensdaten recherchieren, belegen und Firmen qualifizieren.',
    '3. Erst nach Unternehmensqualifikation Ansprechpartner im Contacts DataFrame recherchieren.',
    '4. Ansprechpartner gegen Scope/ICP qualifizieren und erst dann als Lead markieren.',
    '',
    'Grenze: Vor Pipeline-Stufe keine Personen recherchieren und keine Outreach-Nachrichten erzeugen.',
  ].join('\n');
}

async function appendKnowledgeRows(campaign, tableKey, rows) {
  if (!rows.length || !nativeRxdbSyncReady()) return null;
  const refs = await ensureCampaignKnowledge(campaign);
  if (!refs) return null;
  for (let index = 0; index < rows.length; index += OUTBOUND_KNOWLEDGE_APPEND_CHUNK) {
    const chunk = rows.slice(index, index + OUTBOUND_KNOWLEDGE_APPEND_CHUNK);
    await knowledgeCommand([
      'data', 'append',
      '--domain', refs.domain,
      '--key', tableKey,
      '--rows', JSON.stringify(chunk),
    ]);
  }
  return true;
}

async function readKnowledgeRows(refs, tableKey, limit = 5000) {
  if (state.knowledgeProjectionDisabled || !nativeRxdbSyncReady()) return [];
  try {
    const result = await knowledgeCommand([
      'data', 'select',
      '--domain', refs.domain,
      '--key', tableKey,
      '--limit', String(limit),
    ]);
    return Array.isArray(result.rows) ? result.rows : [];
  } catch (error) {
    const message = String(error?.message || error);
    if (message.includes('405') || message.includes('404') || message.includes('Failed to fetch')) {
      state.knowledgeProjectionDisabled = true;
    }
    console.warn('[outbound] knowledge projection unavailable; using local projections', error);
    return [];
  }
}

function nativeRxdbSyncReady() {
  return state.ctx?.sync?.mode === 'webrtc'
    || state.ctx?.sync?.config?.native_rxdb_peer_available === true;
}

async function waitForBusinessCommandProjection(commandId, startedAtMs) {
  const collection = state.ctx?.db?.raw?.business_commands;
  if (!collection) return null;
  const earliestUpdatedAt = Math.max(0, Number(startedAtMs || Date.now()) - 1000);
  for (let attempt = 0; attempt < 24; attempt += 1) {
    try {
      const doc = await collection.findOne(commandId).exec();
      const match = typeof doc?.toJSON === 'function' ? doc.toJSON() : doc;
      if (
        match
        && Number(match.updated_at_ms || 0) >= earliestUpdatedAt
        && match.status
        && match.status !== 'pending_sync'
      ) {
        return match;
      }
    } catch (_) {
      // Retry below; replicated command results may arrive just after dispatch.
    }
    await new Promise((resolve) => window.setTimeout(resolve, 500));
  }
  return null;
}

function openCampaignRunbook(campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId) || selectedCampaign();
  const runbookId = campaignKnowledgeRefs(campaign).runbookId;
  if (!runbookId) return;
  sessionStorage.setItem('ctox.businessOs.knowledge.openId', `runbook:${runbookId}`);
  location.hash = 'knowledge';
}

async function ensureDefaultCampaign() {
  const collection = state.ctx?.db?.raw?.outbound_campaigns;
  if (!collection) return;
  const existing = await collection.find().exec();
  if (existing.some((doc) => doc.id === DEFAULT_CAMPAIGN_ID)) return;
  const defaults = existing
    .map((doc) => doc.toJSON ? doc.toJSON() : doc)
    .filter((doc) => doc.name === DEFAULT_CAMPAIGN_NAME);
  if (defaults.length) return;
  const now = Date.now();
  const campaign = {
    id: DEFAULT_CAMPAIGN_ID,
    name: DEFAULT_CAMPAIGN_NAME,
    objective: 'Unternehmen importieren, qualifizieren und erst danach in die Ansprechpartner-Pipeline übergeben.',
    market: 'DACH',
    status: 'active',
    owner_id: state.ctx?.session?.user?.id || '',
    source_count: 0,
    company_count: 0,
    qualified_count: 0,
    pipeline_count: 0,
    payload: { outbound_only: true },
    created_at_ms: now,
    updated_at_ms: now,
  };
  await collection.insert(campaign);
  ensureCampaignKnowledge(campaign).catch((error) => {
    console.warn('[outbound] default campaign knowledge setup failed', error);
  });
}

async function loadAll(options = {}) {
  const raw = state.ctx?.db?.raw || {};
  const [campaigns, sources, companies, pipeline, runs] = await Promise.all([
    findAll(raw.outbound_campaigns),
    findAll(raw.outbound_sources),
    findAll(raw.outbound_companies),
    findAll(raw.outbound_pipeline_items),
    findAll(raw.outbound_research_runs),
  ]);
  state.campaigns = campaigns;
  state.sources = sources;
  state.companies = companies;
  state.pipeline = dedupePipelineItems(pipeline);
  state.runs = runs;
  refreshOperationalStateInBackground();
  if (!state.selectedCampaignId && state.campaigns[0]) state.selectedCampaignId = state.campaigns[0].id;
  const visible = visibleCampaigns();
  if (visible.length && !visible.some((campaign) => campaign.id === state.selectedCampaignId)) {
    state.selectedCampaignId = visible[0].id;
  }
  if (options.hydrateKnowledge === true) await hydrateSelectedCampaignFromKnowledge();
  if (!state.selectedCompanyId && currentCompanies()[0]) state.selectedCompanyId = currentCompanies()[0].id;
  if (!state.selectedPipelineId && currentPipeline()[0]) state.selectedPipelineId = currentPipeline()[0].id;
}

function refreshOperationalStateInBackground() {
  const raw = state.ctx?.db?.raw || {};
  if (!raw.business_commands || !raw.ctox_queue_tasks) return;
  const now = Date.now();
  if (state.operationalRefreshPending || now - state.lastOperationalRefreshMs < 10000) return;
  state.operationalRefreshPending = true;
  state.lastOperationalRefreshMs = now;
  withTimeout(Promise.all([
    findAll(raw.business_commands),
    findAll(raw.ctox_queue_tasks),
  ]), 2500, null)
    .then((result) => {
      if (!result) return;
      const [commands, queueTasks] = result;
      state.commands = commands;
      state.queueTasks = queueTasks;
      render();
    })
    .catch((error) => console.warn('[outbound] CTOX activity refresh skipped', error))
    .finally(() => {
      state.operationalRefreshPending = false;
    });
}

function withTimeout(promise, timeoutMs, fallback) {
  return Promise.race([
    promise,
    new Promise((resolve) => window.setTimeout(() => resolve(fallback), timeoutMs)),
  ]);
}

async function hydrateSelectedCampaignFromKnowledge() {
  const campaign = selectedCampaign();
  if (!campaign || state.knowledgeProjectionDisabled || !campaign.payload?.knowledge) return;
  const refs = campaignKnowledgeRefs(campaign);
  const [companyRows, contactRows, runRows] = await Promise.all([
    readKnowledgeRows(refs, refs.companiesKey),
    readKnowledgeRows(refs, refs.contactsKey),
    readKnowledgeRows(refs, refs.runsKey, 1000),
  ]);
  if (companyRows.length) {
    const projectedCompanies = projectCompaniesFromKnowledgeRows(campaign, companyRows);
    state.companies = mergeProjectionById(state.companies, projectedCompanies);
  }
  if (contactRows.length) {
    const projectedPipeline = projectPipelineFromKnowledgeRows(campaign, contactRows);
    state.pipeline = dedupePipelineItems(mergeProjectionById(state.pipeline, projectedPipeline));
  }
  if (runRows.length) {
    const projectedRuns = projectRunsFromKnowledgeRows(campaign, runRows);
    state.runs = mergeProjectionById(state.runs, projectedRuns);
  }
}

function projectCompaniesFromKnowledgeRows(campaign, rows) {
  const latest = latestKnowledgeRows(rows, (row) => companyIdentityKeyFromKnowledgeRow(campaign, row));
  return latest.map((row) => {
    const raw = parseJsonObject(row.raw_json || row.imported_row_json || '{}');
    const research = parseJsonObject(row.company_data_json || row.research_json || row.result_json || '{}');
    const evidence = parseJsonArray(row.evidence_json || research.evidence_json || research.evidence || []);
    const name = stringValue(row.company_name || row.name || raw.company || raw.name || raw.Company || raw.Firma);
    const website = stringValue(row.website || raw.website || raw.url || raw.URL || raw.Website);
    const domain = stringValue(row.domain || raw.domain || domainFromUrl(website));
    const now = Date.now();
    return {
      id: companyIdFromKnowledgeRow(campaign, row),
      campaign_id: stringValue(row.campaign_id || campaign.id),
      source_id: stringValue(row.source_id),
      row_index: Number(row.row_index || 0),
      name: name || domain || website || 'Unbenanntes Unternehmen',
      website,
      domain,
      city: stringValue(row.city || row.ort || row.location || raw.city || raw.Ort),
      country: stringValue(row.country || row.land || raw.country || raw.Land),
      qualification_status: stringValue(row.qualification_status || row.company_qualification_status || 'new'),
      research_status: stringValue(row.research_status || statusFromKnowledgeResult(row) || 'pending'),
      pipeline_status: stringValue(row.pipeline_status || 'not_started'),
      fit_score: Number(row.fit_score || row.company_fit_score || 0),
      fit_status: stringValue(row.fit_status || 'unqualified'),
      company_data: { ...raw, ...research },
      evidence,
      payload: {
        imported_row: raw,
        knowledge_row: row,
        knowledge_projection: true,
      },
      created_at_ms: Number(row.created_at_ms || row.imported_at_ms || row.updated_at_ms || now),
      updated_at_ms: Number(row.updated_at_ms || row.researched_at_ms || row.imported_at_ms || now),
    };
  });
}

function projectPipelineFromKnowledgeRows(campaign, rows) {
  const grouped = new Map();
  for (const row of rows) {
    const companyId = stringValue(row.company_id) || companyIdFromKnowledgeRow(campaign, row);
    const pipelineId = stringValue(row.pipeline_id || row.record_id) || `pipe_${fingerprint(`${campaign.id}:${companyId}`)}`;
    const group = grouped.get(pipelineId) || {
      rows: [],
      contacts: [],
      latest: row,
      pipelineId,
      companyId,
    };
    group.rows.push(row);
    group.latest = newerKnowledgeRow(group.latest, row);
    const contact = contactFromKnowledgeRow(row);
    if (contact) group.contacts.push(contact);
    grouped.set(pipelineId, group);
  }
  return Array.from(grouped.values()).map((group) => {
    const row = group.latest || {};
    const now = Date.now();
    return {
      id: group.pipelineId,
      campaign_id: stringValue(row.campaign_id || campaign.id),
      company_id: group.companyId,
      company_name: stringValue(row.company_name || row.name || 'Unternehmen'),
      stage: stringValue(row.stage || (isLeadKnowledgeRow(row) ? 'lead_qualified' : 'contact_research')),
      contact_research_status: stringValue(row.contact_research_status || (group.contacts.length ? 'researched' : 'pending')),
      outreach_status: stringValue(row.outreach_status || row.lead_status || (isLeadKnowledgeRow(row) ? 'qualified' : 'not_started')),
      priority: stringValue(row.priority || 'normal'),
      contacts: dedupeContacts(group.contacts),
      payload: {
        knowledge_rows: group.rows,
        knowledge_projection: true,
      },
      created_at_ms: Number(row.created_at_ms || row.imported_at_ms || now),
      updated_at_ms: Number(row.updated_at_ms || row.researched_at_ms || now),
    };
  });
}

function projectRunsFromKnowledgeRows(campaign, rows) {
  const latest = latestKnowledgeRows(rows, (row) => stringValue(row.run_id || row.command_id || row.record_id || fingerprint(JSON.stringify(row))));
  return latest.map((row) => ({
    id: stringValue(row.run_id || row.command_id || `run_${fingerprint(JSON.stringify(row))}`),
    campaign_id: stringValue(row.campaign_id || campaign.id),
    company_id: stringValue(row.company_id || row.record_id),
    pipeline_id: stringValue(row.pipeline_id),
    run_type: stringValue(row.run_type || 'research'),
    status: stringValue(row.status || row.ctox_status || 'queued'),
    command_id: stringValue(row.command_id),
    request: parseJsonObject(row.request_json || '{}'),
    result: parseJsonObject(row.result_json || '{}'),
    error: stringValue(row.error || ''),
    created_at_ms: Number(row.created_at_ms || Date.now()),
    updated_at_ms: Number(row.updated_at_ms || row.created_at_ms || Date.now()),
  }));
}

function latestKnowledgeRows(rows, idForRow) {
  const map = new Map();
  for (const row of rows) {
    const id = idForRow(row);
    if (!id) continue;
    const existing = map.get(id);
    map.set(id, existing ? newerKnowledgeRow(existing, row) : row);
  }
  return Array.from(map.values());
}

function newerKnowledgeRow(a, b) {
  const timeA = Number(a?.updated_at_ms || a?.researched_at_ms || a?.created_at_ms || a?.imported_at_ms || 0);
  const timeB = Number(b?.updated_at_ms || b?.researched_at_ms || b?.created_at_ms || b?.imported_at_ms || 0);
  return timeB >= timeA ? b : a;
}

function mergeProjectionById(localRows, projectedRows) {
  if (!projectedRows.length) return localRows;
  const map = new Map(localRows.map((row) => [row.id, row]));
  for (const projected of projectedRows) {
    const existing = map.get(projected.id);
    map.set(projected.id, existing ? mergeProjectedRow(existing, projected) : projected);
  }
  return Array.from(map.values()).sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function dedupeCompanies(companies) {
  const map = new Map();
  for (const company of companies || []) {
    const key = companyIdentityKey(company);
    const current = map.get(key);
    map.set(key, current ? mergeDuplicateCompany(current, company) : { ...company, duplicate_company_ids: [company.id] });
  }
  return Array.from(map.values()).sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function mergeDuplicateCompany(a, b) {
  const winner = companyQualityScore(b) > companyQualityScore(a) ? b : a;
  const other = winner === b ? a : b;
  const ids = new Set([...(a.duplicate_company_ids || [a.id]), ...(b.duplicate_company_ids || [b.id])].filter(Boolean));
  const researchStatus = mergedResearchStatus(a.research_status, b.research_status);
  const qualificationStatus = mergedQualificationStatus(a.qualification_status, b.qualification_status);
  return {
    ...other,
    ...winner,
    id: winner.id,
    source_id: winner.source_id || other.source_id || '',
    website: betterCompanyUrl(winner.website, other.website),
    domain: betterCompanyDomain(winner.domain, other.domain),
    city: winner.city || other.city || '',
    country: winner.country || other.country || '',
    qualification_status: qualificationStatus,
    research_status: researchStatus,
    pipeline_status: ['pipeline', 'sent', 'queued'].includes(String(a.pipeline_status || '').toLowerCase()) ? a.pipeline_status : b.pipeline_status || a.pipeline_status,
    company_data: { ...(other.company_data || {}), ...(winner.company_data || {}) },
    evidence: [...(other.evidence || []), ...(winner.evidence || [])],
    payload: {
      ...(other.payload || {}),
      ...(winner.payload || {}),
      duplicate_company_ids: Array.from(ids),
    },
    duplicate_company_ids: Array.from(ids),
    updated_at_ms: Math.max(Number(a.updated_at_ms || 0), Number(b.updated_at_ms || 0)),
  };
}

function companyQualityScore(company) {
  let score = 0;
  if (isCompanyResearchDone(company)) score += 100;
  if (String(company.research_status || '').toLowerCase() === 'queued') score += 25;
  if (String(company.qualification_status || '').toLowerCase() === 'qualified') score += 40;
  if (company.domain) score += isEventListDomain(company.domain) ? 1 : 15;
  if (company.website) score += isEventListDomain(domainFromUrl(company.website)) ? 1 : 8;
  if (company.city) score += 3;
  return score + Math.min(10, Number(company.updated_at_ms || 0) / 1000000000000);
}

function mergedResearchStatus(a, b) {
  const statuses = [a, b].map((value) => String(value || '').toLowerCase());
  if (statuses.some((status) => ['researched', 'completed', 'done'].includes(status))) return 'researched';
  if (statuses.some((status) => isQueuedStatus(status))) return 'queued';
  return a || b || 'pending';
}

function mergedQualificationStatus(a, b) {
  const statuses = [a, b].map((value) => String(value || '').toLowerCase());
  if (statuses.includes('qualified')) return 'qualified';
  if (statuses.includes('rejected')) return 'rejected';
  return a || b || 'new';
}

function betterCompanyDomain(a, b) {
  if (!a) return b || '';
  if (!b) return a || '';
  if (isEventListDomain(a) && !isEventListDomain(b)) return b;
  return a;
}

function betterCompanyUrl(a, b) {
  if (!a) return b || '';
  if (!b) return a || '';
  if (isEventListDomain(domainFromUrl(a)) && !isEventListDomain(domainFromUrl(b))) return b;
  return a;
}

function isEventListDomain(domain) {
  return ['intersolar.de', 'thesmartere.de', 'messe-muenchen.de'].includes(String(domain || '').toLowerCase().replace(/^www\./, ''));
}

function mergeProjectedRow(existing, projected) {
  const merged = {
    ...existing,
    ...projected,
    payload: { ...(existing.payload || {}), ...(projected.payload || {}) },
  };
  for (const field of ['research_status', 'contact_research_status', 'outreach_status', 'pipeline_status']) {
    if (isQueuedStatus(existing[field]) && !isDoneLikeStatus(projected[field])) merged[field] = existing[field];
  }
  if (isQueuedStatus(existing.research_status)
    || isQueuedStatus(existing.contact_research_status)
    || isQueuedStatus(existing.outreach_status)
    || isQueuedStatus(existing.pipeline_status)) {
    merged.updated_at_ms = Math.max(Number(existing.updated_at_ms || 0), Number(projected.updated_at_ms || 0));
  }
  return merged;
}

function companyIdFromKnowledgeRow(campaign, row) {
  const explicit = stringValue(row.company_id || row.record_id || row.id);
  if (explicit) return explicit;
  const identity = companyIdentityKeyFromKnowledgeRow(campaign, row);
  if (identity) return `co_${fingerprint(identity)}`;
  const name = stringValue(row.company_name || row.name);
  const locator = stringValue(row.domain || row.website || row.row_index || '');
  return `co_${fingerprint(`${campaign.id}:${name}:${locator}`)}`;
}

function companyIdentityKeyFromKnowledgeRow(campaign, row) {
  const raw = parseJsonObject(row.raw_json || row.imported_row_json || '{}');
  return companyIdentityKey({
    campaign_id: stringValue(row.campaign_id || campaign.id),
    name: stringValue(row.company_name || row.name || raw.company || raw.name || raw.Company || raw.Firma),
    domain: stringValue(row.domain || raw.domain),
    website: stringValue(row.website || raw.website || raw.url || raw.URL || raw.Website),
    country: stringValue(row.country || row.land || raw.country || raw.Land),
  });
}

function companyIdentityKey(company) {
  const campaignId = stringValue(company?.campaign_id || state.selectedCampaignId);
  const name = normalizeCompanyIdentityName(company?.name || company?.company_name || '');
  if (name && name.length >= 3) return `${campaignId}:name:${name}`;
  const domain = normalizeCompanyIdentityDomain(company?.domain || domainFromUrl(company?.website || ''));
  if (domain) return `${campaignId}:domain:${domain}`;
  return `${campaignId}:id:${company?.id || fingerprint(JSON.stringify(company || {}))}`;
}

function normalizeCompanyIdentityName(value) {
  return String(value || '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/&/g, 'und')
    .replace(/\b(gmbh|ag|kg|kgaa|ug|ohg|ev|e\\.v\\.|co|ltd|limited|inc|corp|corporation|llc)\b/g, '')
    .replace(/[^a-z0-9]+/g, '')
    .trim();
}

function normalizeCompanyIdentityDomain(value) {
  return String(value || '').toLowerCase().replace(/^https?:\/\//, '').replace(/^www\./, '').split('/')[0].trim();
}

function contactFromKnowledgeRow(row) {
  const name = stringValue(row.contact_name || row.person_name || row.name);
  const role = stringValue(row.role || row.title || row.position);
  const email = stringValue(row.email || row.e_mail);
  const linkedIn = stringValue(row.linkedin || row.linkedin_url);
  if (!name && !role && !email && !linkedIn) return null;
  return {
    id: stringValue(row.contact_id || row.person_id || `contact_${fingerprint(`${name}:${role}:${email}:${linkedIn}`)}`),
    name,
    role,
    email,
    linkedin_url: linkedIn,
    qualification_status: stringValue(row.contact_qualification_status || row.qualification_status || ''),
    evidence: parseJsonArray(row.evidence_json || row.evidence || []),
  };
}

function dedupeContacts(contacts) {
  const map = new Map();
  for (const contact of contacts) {
    const id = contact.id || fingerprint(JSON.stringify(contact));
    map.set(id, { ...(map.get(id) || {}), ...contact });
  }
  return Array.from(map.values());
}

function dedupePipelineItems(items) {
  const map = new Map();
  for (const item of items || []) {
    const id = stringValue(item.company_id || item.id);
    if (!id) continue;
    const existing = map.get(id);
    map.set(id, existing ? newerKnowledgeRow(existing, item) : item);
  }
  return Array.from(map.values());
}

function isLeadKnowledgeRow(row) {
  return ['qualified', 'lead_qualified', 'ready'].includes(String(row.lead_status || row.outreach_status || row.stage || '').toLowerCase());
}

function statusFromKnowledgeResult(row) {
  if (row.researched_at_ms || row.company_data_json || row.research_json || row.result_json) return 'researched';
  return '';
}

function parseJsonObject(value) {
  if (!value) return {};
  if (typeof value === 'object' && !Array.isArray(value)) return value;
  try {
    const parsed = JSON.parse(String(value));
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function parseJsonArray(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value;
  try {
    const parsed = JSON.parse(String(value));
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function stringValue(value) {
  if (value == null) return '';
  return String(value).trim();
}

function wireRealtime() {
  const raw = state.ctx?.db?.raw || {};
  const collections = [
    raw.outbound_campaigns,
    raw.outbound_sources,
    raw.outbound_companies,
    raw.outbound_pipeline_items,
    raw.outbound_research_runs,
    raw.business_commands,
    raw.ctox_queue_tasks,
    raw.outbound_engagements,
    raw.outbound_messages,
    raw.outbound_approvals,
    raw.outbound_sequences,
    raw.outbound_sender_assignments,
    raw.outbound_meeting_requests,
    raw.outbound_suppression_entries,
    raw.outbound_account_limits,
    raw.outbound_skillbooks,
    raw.outbound_letter_templates,
  ].filter(Boolean);
  for (const collection of collections) {
    const subscription = collection.$?.subscribe?.(() => scheduleDataRefresh(20));
    if (subscription?.unsubscribe) state.cleanup.push(() => subscription.unsubscribe());
  }
  startKnowledgeProjectionWatch();
}

function scheduleDataRefresh(delay = 80) {
  if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
  state.refreshTimer = window.setTimeout(async () => {
    state.refreshTimer = null;
    await loadAll({ hydrateKnowledge: false });
    await loadActiveOutreachData().catch((error) => console.warn('[outbound] active outreach refresh failed', error));
    render();
  }, delay);
}

function startKnowledgeProjectionWatch() {
  if (state.knowledgeWatchTimer) window.clearInterval(state.knowledgeWatchTimer);
  const tick = async () => {
    const changed = await refreshKnowledgeProjectionIfChanged();
    if (changed) render();
  };
  state.knowledgeWatchTimer = window.setInterval(tick, 2500);
  state.cleanup.push(() => {
    if (state.knowledgeWatchTimer) window.clearInterval(state.knowledgeWatchTimer);
    state.knowledgeWatchTimer = null;
    if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
    state.refreshTimer = null;
  });
}

async function refreshKnowledgeProjectionIfChanged() {
  const campaign = selectedCampaign();
  if (!campaign || state.knowledgeProjectionDisabled || !campaign.payload?.knowledge) return false;
  const refs = campaignKnowledgeRefs(campaign);
  try {
    const [companyRows, contactRows, runRows] = await Promise.all([
      readKnowledgeRows(refs, refs.companiesKey),
      readKnowledgeRows(refs, refs.contactsKey),
      readKnowledgeRows(refs, refs.runsKey, 1000),
    ]);
    const signature = knowledgeRowsSignature(campaign.id, companyRows, contactRows, runRows);
    if (signature === state.knowledgeProjectionSignature) return false;
    state.knowledgeProjectionSignature = signature;
    if (companyRows.length) state.companies = mergeProjectionById(state.companies, projectCompaniesFromKnowledgeRows(campaign, companyRows));
    if (contactRows.length) state.pipeline = dedupePipelineItems(mergeProjectionById(state.pipeline, projectPipelineFromKnowledgeRows(campaign, contactRows)));
    if (runRows.length) state.runs = mergeProjectionById(state.runs, projectRunsFromKnowledgeRows(campaign, runRows));
    return true;
  } catch (error) {
    console.warn('[outbound] knowledge projection watch failed', error);
    return false;
  }
}

function knowledgeRowsSignature(campaignId, companyRows, contactRows, runRows) {
  const compact = (rows, idFn) => rows.map((row) => [
    idFn(row),
    row.updated_at_ms || row.researched_at_ms || row.imported_at_ms || row.created_at_ms || '',
    row.company_name || row.name || row.contact_name || row.run_type || '',
    row.research_status || row.qualification_status || row.contact_research_status || row.lead_status || row.status || '',
  ].join(':')).sort().join('|');
  return [
    campaignId,
    compact(companyRows, (row) => row.company_id || companyIdFromKnowledgeRow({ id: campaignId }, row)),
    compact(contactRows, (row) => row.pipeline_id || row.contact_id || row.company_id || ''),
    compact(runRows, (row) => row.run_id || row.command_id || row.company_id || ''),
  ].join('::');
}

function wireEvents(root) {
  root.addEventListener('click', async (event) => {
    const action = event.target.closest('[data-action]')?.dataset.action;
    const view = event.target.closest('[data-view]')?.dataset.view;
    const filter = event.target.closest('[data-filter]')?.dataset.filter;

    if (action === 'toggle-outreach') {
      event.preventDefault();
      state.outreachView = !state.outreachView;
      render(true);
      return;
    }
    if (action && action.startsWith('ao-')) {
      event.preventDefault();
      const target = event.target.closest('[data-action]');
      if (target) {
        try {
          await handleActiveOutreachAction(action, target);
        } catch (error) {
          console.warn('[outbound] active outreach action failed', error);
        }
      }
      return;
    }

    // Copy to clipboard helper
    const copyBtn = event.target.closest('[data-copy-text]');
    if (copyBtn) {
      event.preventDefault();
      event.stopPropagation();
      const textToCopy = copyBtn.dataset.copyText;
      if (textToCopy) {
        navigator.clipboard.writeText(textToCopy).then(() => {
          const originalText = copyBtn.innerText;
          const originalBg = copyBtn.style.background;
          const originalColor = copyBtn.style.color;
          const originalBorder = copyBtn.style.borderColor;

          copyBtn.innerText = t('copied', 'Kopiert!');
          copyBtn.style.background = '#10b981';
          copyBtn.style.color = '#ffffff';
          copyBtn.style.borderColor = '#10b981';

          setTimeout(() => {
            copyBtn.innerText = originalText;
            copyBtn.style.background = originalBg;
            copyBtn.style.color = originalColor;
            copyBtn.style.borderColor = originalBorder;
          }, 2000);
        }).catch(err => {
          console.error('Failed to copy text: ', err);
        });
      }
      return;
    }

    // Tab switching clicks
    const msgTab = event.target.closest('.msg-tab');
    if (msgTab) {
      event.preventDefault();
      event.stopPropagation();
      const tr = msgTab.closest('tr');
      if (tr) {
        const contactKey = tr.dataset.contactKey;
        const tabKey = msgTab.dataset.msg;
        state.activeMsgByContact.set(contactKey, tabKey);
        replaceRowDOMInline(tr);
      }
      return;
    }

    const noteTab = event.target.closest('.note-tab');
    if (noteTab) {
      event.preventDefault();
      event.stopPropagation();
      const tr = noteTab.closest('tr');
      if (tr) {
        const contactKey = tr.dataset.contactKey;
        const tabKey = noteTab.dataset.note;
        state.activeNoteByContact.set(contactKey, tabKey);
        replaceRowDOMInline(tr);
      }
      return;
    }

    // Toggle compact view checkbox
    if (event.target.id === 'toggle-compact') {
      state.viewMode = event.target.checked ? 'compact' : 'expanded';
      const container = root.querySelector('.outbound-unified-workbench');
      if (container) container.setAttribute('data-view', state.viewMode);
      renderCenter();
      return;
    }

    // Delete note type action
    if (action === 'delete-note-type') {
      event.preventDefault();
      event.stopPropagation();
      const tr = event.target.closest('tr');
      if (!tr) return;
      const itemId = tr.dataset.id;
      const contactIndex = Number(tr.dataset.contactIndex);
      const noteKey = event.target.dataset.noteKey;
      if (confirm('Diesen Notiz-Typ löschen?')) {
        await updateContactInPipelineItem(itemId, contactIndex, (c) => {
          if (c.notes) delete c.notes[noteKey];
        });
      }
      return;
    }

    // Add note type action
    if (action === 'add-note-type') {
      event.preventDefault();
      event.stopPropagation();
      const tr = event.target.closest('tr');
      if (!tr) return;
      const itemId = tr.dataset.id;
      const contactIndex = Number(tr.dataset.contactIndex);
      const raw = prompt('Neuen Notiz-Typ anlegen (Name):', '');
      if (!raw) return;
      const safeKey = 'note_' + raw.trim().toLowerCase().replace(/\s+/g, '_');
      await updateContactInPipelineItem(itemId, contactIndex, (c) => {
        if (!c.notes) c.notes = {};
        c.notes[safeKey] = '';
      });
      return;
    }

    // Status multi-select overlay click trigger
    if (action === 'edit-status' || event.target.closest('.col-status')) {
      const tr = event.target.closest('tr');
      if (tr && tr.dataset.contactIndex != null) {
        event.preventDefault();
        event.stopPropagation();
        const itemId = tr.dataset.id;
        const contactIndex = Number(tr.dataset.contactIndex);
        const item = state.pipeline.find(entry => entry.id === itemId);
        const contact = item?.contacts?.[contactIndex];
        if (contact) {
          const cell = tr.querySelector('.col-status');
          const curVals = chipsFromMultiSelect(contact.status_field || contact.status);
          const allOptions = extractUniqueStatuses(state.pipeline);
          showMultiSelectOverlay(cell, {
            currentValues: curVals,
            allOptions,
            labelSingular: 'Status',
            onSave: (finalVals) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
              c.status_field = { keys: finalVals };
            })
          });
        }
      }
      return;
    }

    // Tag multi-select overlay click trigger
    if (action === 'edit-tags' || event.target.closest('.col-tags')) {
      const tr = event.target.closest('tr');
      if (tr && tr.dataset.contactIndex != null) {
        event.preventDefault();
        event.stopPropagation();
        const itemId = tr.dataset.id;
        const contactIndex = Number(tr.dataset.contactIndex);
        const item = state.pipeline.find(entry => entry.id === itemId);
        const contact = item?.contacts?.[contactIndex];
        if (contact) {
          const cell = tr.querySelector('.col-tags');
          const curVals = chipsFromMultiSelect(contact.tags);
          const allOptions = extractUniqueTags(state.pipeline);
          showMultiSelectOverlay(cell, {
            currentValues: curVals,
            allOptions,
            labelSingular: 'Tag',
            onSave: (finalVals) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
              c.tags = { keys: finalVals };
            })
          });
        }
      }
      return;
    }

    if (!action && !view && !filter) return;
    const id = event.target.closest('[data-id]')?.dataset.id || '';
    if (action === 'select-campaign') {
      state.selectedCampaignId = id;
      state.selectedCompanyId = currentCompanies()[0]?.id || '';
      state.selectedPipelineId = currentPipeline()[0]?.id || '';
      render();
    }
    if (action === 'new-campaign') await createCampaign();
    if (action === 'import-source') {
      if (id) state.selectedCampaignId = id;
      await openCompanyImporter();
    }
    if (action === 'open-campaign-runbook') openCampaignRunbook(id || state.selectedCampaignId);
    if (action === 'edit-campaign') {
      state.editingCampaignId = id || state.selectedCampaignId;
      renderLeft();
    }
    if (action === 'cancel-campaign-edit') {
      state.editingCampaignId = '';
      renderLeft();
    }
    if (action === 'save-campaign-edit') await saveCampaignInlineEdit(id || state.selectedCampaignId);
    if (action === 'delete-campaign') await deleteCampaign(id || state.selectedCampaignId);
    if (action === 'select-company-row') {
      const companyId = event.target.closest('[data-company-id]')?.dataset.companyId || id;
      if (companyId) {
        state.selectedCompanyId = companyId;
        state.activeView = 'companies';
        render();
      }
    }
    if (action === 'select-company') {
      state.selectedCompanyId = id;
      render();
    }
    if (action === 'research-company') await queueCompanyResearch(id || state.selectedCompanyId);
    if (action === 'qualify-company') await setCompanyQualification(id || state.selectedCompanyId, 'qualified');
    if (action === 'reject-company') await setCompanyQualification(id || state.selectedCompanyId, 'rejected');
    if (action === 'send-pipeline') await sendCompanyToPipeline(id || state.selectedCompanyId);
    if (action === 'select-company-contact') {
      event.preventDefault();
      event.stopPropagation();
      const btn = event.target.closest('[data-action="select-company-contact"]');
      const companyId = btn.dataset.companyId;
      const index = Number(btn.dataset.index);
      setSelectedContactIndexForCompany(companyId, index);
      const tr = btn.closest('tr');
      if (tr) {
        replaceRowDOMInline(tr);
      }
      return;
    }
    if (action === 'delete-company-contact') {
      event.preventDefault();
      event.stopPropagation();
      const btn = event.target.closest('[data-action="delete-company-contact"]');
      const itemId = btn.dataset.itemId;
      const index = Number(btn.dataset.index);
      if (itemId && confirm('Diesen Ansprechpartner wirklich löschen?')) {
        await deleteContactFromPipelineItem(itemId, index);
      }
      return;
    }
    if (action === 'open-automation') openAutomationDrawer(event.target.closest('[data-stage]')?.dataset.stage, event.target.closest('[data-campaign-id]')?.dataset.campaignId || state.selectedCampaignId);
    if (action === 'close-automation') closeAutomationDrawer();
    if (action === 'start-automation') await startAutomationBatch();
    if (action === 'select-pipeline') {
      state.selectedPipelineId = id;
      render();
    }
    if (action === 'research-contacts') await queueContactResearch(id || state.selectedPipelineId);
    if (action === 'open-research-settings') openResearchSettingsDrawer();
    if (action === 'research-settings-tab') toggleResearchSettingsTab(event.target.dataset.tab);
    if (action === 'generate-outreach') {
      const tr = event.target.closest('tr');
      if (tr && tr.dataset.contactIndex != null) {
        event.preventDefault();
        event.stopPropagation();
        const itemId = tr.dataset.id;
        const contactIndex = Number(tr.dataset.contactIndex);
        await generateOutreachForContact(itemId, contactIndex);
      }
    }
    if (action === 'export-table') exportQualificationTable();
    if (action === 'sort-table') {
      const column = event.target.closest('[data-column]')?.dataset.column;
      if (column) {
        setTableSort(column);
        renderCenter();
      }
    }
    if (action === 'clear-table-filters') {
      state.tableFilters = {};
      state.tableSort = null;
      renderCenter();
    }
    if (action === 'close-research-settings') closeResearchSettingsDrawer();
    if (action === 'save-research-settings') await saveResearchSettings();
    if (action === 'research-settings-all') setResearchSettingsSelection(true);
    if (action === 'research-settings-core') setResearchSettingsCoreSelection();
    if (action === 'research-settings-add-field') addResearchSettingsField();
    if (action === 'research-settings-add-contact-field') addResearchSettingsField('contact');
    if (action === 'research-settings-remove-field') removeResearchSettingsField(event.target.closest('[data-custom-field-id]')?.dataset.customFieldId);
    if (action === 'research-settings-remove-contact-field') removeResearchSettingsField(event.target.closest('[data-custom-field-id]')?.dataset.customFieldId, 'contact');

    if (action === 'mailserver-save-domain') {
      event.preventDefault();
      event.stopPropagation();
      const drawer = root.querySelector('.outbound-research-panel') || root.querySelector('.outbound-research-drawer');
      const domainName = (drawer?.querySelector('#mailserver-new-domain-input')?.value || '').trim();
      if (!domainName) {
        showBusinessAlert(t('pleaseEnterDomain', 'Bitte gib einen Domain-Namen ein.'));
        return;
      }
      state.mailserver = state.mailserver || { domains: [], users: [], loading: false, error: null };
      state.mailserver.loading = true;
      const rPanel = root.querySelector('.outbound-research-drawer');
      if (rPanel) {
        rPanel.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
      }
      try {
        const commandId = `cmd_mailserver_save_domain_${crypto.randomUUID()}`;
        const startedAtMs = Date.now();
        const dispatched = await state.ctx.commandBus.dispatch({
          id: commandId,
          module: 'ctox',
          type: 'ctox.mailserver.save_domain',
          record_id: commandId,
          inbound_channel: 'business_os.outbound',
          payload: {
            domain_name: domainName,
            dkim_selector: 'default',
            dkim_private_key: '',
          },
          client_context: { source_module: 'outbound' }
        });
        let result = dispatched;
        if (!dispatched?.status || dispatched.status === 'pending_sync') {
          result = await waitForBusinessCommandProjection(commandId, startedAtMs);
        }
        if (!result || (result.status !== 'completed' && !result.result)) {
          throw new Error('Fehler beim Speichern der Domain.');
        }
      } catch (err) {
        showBusinessAlert(err.message || String(err));
      } finally {
        await loadMailserverConfig();
      }
      return;
    }

    if (action === 'mailserver-delete-domain') {
      event.preventDefault();
      event.stopPropagation();
      const btn = event.target.closest('[data-action="mailserver-delete-domain"]');
      const domainName = btn?.dataset.domain;
      if (!domainName) return;
      const confirmed = await showBusinessConfirm(
        t('deleteDomainConfirm', 'Möchtest du die Domain "{0}" wirklich löschen? Alle E-Mail-Konten dieser Domain könnten beeinträchtigt werden.', domainName),
        { danger: true }
      );
      if (!confirmed) return;
      state.mailserver = state.mailserver || { domains: [], users: [], loading: false, error: null };
      state.mailserver.loading = true;
      const rPanel = root.querySelector('.outbound-research-drawer');
      if (rPanel) {
        rPanel.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
      }
      try {
        const commandId = `cmd_mailserver_delete_domain_${crypto.randomUUID()}`;
        const startedAtMs = Date.now();
        const dispatched = await state.ctx.commandBus.dispatch({
          id: commandId,
          module: 'ctox',
          type: 'ctox.mailserver.delete_domain',
          record_id: commandId,
          inbound_channel: 'business_os.outbound',
          payload: { domain_name: domainName },
          client_context: { source_module: 'outbound' }
        });
        let result = dispatched;
        if (!dispatched?.status || dispatched.status === 'pending_sync') {
          result = await waitForBusinessCommandProjection(commandId, startedAtMs);
        }
        if (!result || (result.status !== 'completed' && !result.result)) {
          throw new Error('Fehler beim Löschen der Domain.');
        }
      } catch (err) {
        showBusinessAlert(err.message || String(err));
      } finally {
        await loadMailserverConfig();
      }
      return;
    }

    if (action === 'mailserver-save-user') {
      event.preventDefault();
      event.stopPropagation();
      const drawer = root.querySelector('.outbound-research-panel') || root.querySelector('.outbound-research-drawer');
      const username = (drawer?.querySelector('#mailserver-new-username-input')?.value || '').trim();
      const password = (drawer?.querySelector('#mailserver-new-password-input')?.value || '').trim();
      if (!username || !password) {
        showBusinessAlert(t('pleaseEnterUserPass', 'Bitte gib eine E-Mail-Adresse und ein Passwort ein.'));
        return;
      }
      state.mailserver = state.mailserver || { domains: [], users: [], loading: false, error: null };
      state.mailserver.loading = true;
      const rPanel = root.querySelector('.outbound-research-drawer');
      if (rPanel) {
        rPanel.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
      }
      try {
        const commandId = `cmd_mailserver_save_user_${crypto.randomUUID()}`;
        const startedAtMs = Date.now();
        const dispatched = await state.ctx.commandBus.dispatch({
          id: commandId,
          module: 'ctox',
          type: 'ctox.mailserver.save_user',
          record_id: commandId,
          inbound_channel: 'business_os.outbound',
          payload: { username, password },
          client_context: { source_module: 'outbound' }
        });
        let result = dispatched;
        if (!dispatched?.status || dispatched.status === 'pending_sync') {
          result = await waitForBusinessCommandProjection(commandId, startedAtMs);
        }
        if (!result || (result.status !== 'completed' && !result.result)) {
          throw new Error('Fehler beim Erstellen des E-Mail-Kontos.');
        }
      } catch (err) {
        showBusinessAlert(err.message || String(err));
      } finally {
        await loadMailserverConfig();
      }
      return;
    }

    if (action === 'mailserver-delete-user') {
      event.preventDefault();
      event.stopPropagation();
      const btn = event.target.closest('[data-action="mailserver-delete-user"]');
      const username = btn?.dataset.username;
      if (!username) return;
      const confirmed = await showBusinessConfirm(
        t('deleteUserConfirm', 'Möchtest du das E-Mail-Konto "{0}" wirklich löschen? Alle in diesem Postfach gespeicherten E-Mails gehen unwiderruflich verloren.', username),
        { danger: true }
      );
      if (!confirmed) return;
      state.mailserver = state.mailserver || { domains: [], users: [], loading: false, error: null };
      state.mailserver.loading = true;
      const rPanel = root.querySelector('.outbound-research-drawer');
      if (rPanel) {
        rPanel.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
      }
      try {
        const commandId = `cmd_mailserver_delete_user_${crypto.randomUUID()}`;
        const startedAtMs = Date.now();
        const dispatched = await state.ctx.commandBus.dispatch({
          id: commandId,
          module: 'ctox',
          type: 'ctox.mailserver.delete_user',
          record_id: commandId,
          inbound_channel: 'business_os.outbound',
          payload: { username },
          client_context: { source_module: 'outbound' }
        });
        let result = dispatched;
        if (!dispatched?.status || dispatched.status === 'pending_sync') {
          result = await waitForBusinessCommandProjection(commandId, startedAtMs);
        }
        if (!result || (result.status !== 'completed' && !result.result)) {
          throw new Error('Fehler beim Löschen des E-Mail-Kontos.');
        }
      } catch (err) {
        showBusinessAlert(err.message || String(err));
      } finally {
        await loadMailserverConfig();
      }
      return;
    }

    if (action === 'hide-company') {
      event.preventDefault();
      event.stopPropagation();
      hideCompany(id);
      render(true);
      return;
    }
    if (action === 'toggle-hidden-companies' || event.target.closest('#toggle-hidden-companies')) {
      event.preventDefault();
      event.stopPropagation();
      toggleHiddenCompaniesPanel();
      return;
    }

    if (view) {
      state.activeView = view;
      render();
    }
    if (filter) {
      state.filter = filter;
      const campaignId = event.target.closest('[data-campaign-id]')?.dataset.campaignId;
      if (campaignId) state.selectedCampaignId = campaignId;
      ensureSelectedCompanyInFilter();
      render();
    }
  });

  // Double-click delegated inline editors
  root.addEventListener('dblclick', async (event) => {
    const tr = event.target.closest('tr');
    if (!tr || tr.dataset.contactIndex == null) return;

    const itemId = tr.dataset.id;
    const contactIndex = Number(tr.dataset.contactIndex);
    const item = state.pipeline.find(entry => entry.id === itemId);
    const contact = item?.contacts?.[contactIndex];
    if (!contact) return;

    // Contact name editing
    const nameBtn = event.target.closest('.person-name-btn');
    if (nameBtn) {
      event.stopPropagation();
      startInlineEdit(nameBtn, {
        initialText: contact.name || contact.firstname || contact.lastname || '',
        multiline: false,
        onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
          c.name = newVal;
        })
      });
      return;
    }

    // Role/Position editing
    const roleTd = event.target.closest('.col-role');
    if (roleTd) {
      event.stopPropagation();
      startInlineEdit(roleTd, {
        initialText: contact.role || contact.title || contact.position || '',
        multiline: false,
        onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
          c.role = newVal;
        })
      });
      return;
    }

    // Department editing
    const deptTd = event.target.closest('.col-dept');
    if (deptTd) {
      event.stopPropagation();
      startInlineEdit(deptTd, {
        initialText: contact.departments || contact.department || '',
        multiline: false,
        onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
          c.departments = newVal;
        })
      });
      return;
    }

    // Email / Phone / LinkedIn stack editing
    const contactTd = event.target.closest('.col-contact');
    if (contactTd) {
      event.stopPropagation();
      const emailLink = event.target.closest('a[href^="mailto:"]');
      const linkedinLink = event.target.closest('a[href*="linkedin"]');

      if (emailLink) {
        startInlineEdit(emailLink, {
          initialText: contact.email || contact.e_mail || '',
          multiline: false,
          onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
            c.email = newVal;
          })
        });
      } else if (linkedinLink) {
        startInlineEdit(linkedinLink, {
          initialText: contact.linkedin_url || contact.linkedin || contact.profile_url || '',
          multiline: false,
          onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
            c.linkedin_url = newVal;
          })
        });
      } else {
        const phoneWrap = contactTd.querySelector('span') || contactTd;
        startInlineEdit(phoneWrap, {
          initialText: contact.phone || contact.telephone || '',
          multiline: false,
          onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
            c.phone = newVal;
          })
        });
      }
      return;
    }

    // Outreach draft editing
    const msgContent = event.target.closest('.msg-content');
    if (msgContent) {
      event.stopPropagation();
      const activeKey = msgContent.dataset.activeKey || 'message_mail_subject';
      const messages = contact.messages || {};
      startInlineEdit(msgContent, {
        initialText: messages[activeKey] || '',
        multiline: true,
        onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
          if (!c.messages) c.messages = {};
          c.messages[activeKey] = newVal;
        })
      });
      return;
    }

    // Contact notes editing
    const noteContent = event.target.closest('.note-content');
    if (noteContent) {
      event.stopPropagation();
      const activeNoteKey = noteContent.dataset.activeNoteKey || 'note_general';
      const notes = contact.notes || {};
      startInlineEdit(noteContent, {
        initialText: notes[activeNoteKey] || '',
        multiline: true,
        onSave: (newVal) => updateContactInPipelineItem(itemId, contactIndex, (c) => {
          if (!c.notes) c.notes = {};
          c.notes[activeNoteKey] = newVal;
        })
      });
      return;
    }
  });

  // Hover-peeking mouseover/mouseout listeners
  root.addEventListener('mouseover', (event) => {
    const tab = event.target.closest('.msg-tab');
    if (tab) {
      const tr = tab.closest('tr');
      if (!tr) return;
      const itemId = tr.dataset.id;
      const contactIndex = Number(tr.dataset.contactIndex);
      const item = state.pipeline.find(entry => entry.id === itemId);
      const contact = item?.contacts?.[contactIndex];
      if (!contact) return;

      const tabKey = tab.dataset.msg;
      const messages = contact.messages || {};
      const activeText = trim(messages[tabKey]);

      const textPre = tr.querySelector('.col-message .msg-text');
      if (textPre) {
        textPre.textContent = activeText || '—';
      }
      tr.classList.add('is-peeking-msg');
      return;
    }

    const noteTab = event.target.closest('.note-tab');
    if (noteTab) {
      const tr = noteTab.closest('tr');
      if (!tr) return;
      const itemId = tr.dataset.id;
      const contactIndex = Number(tr.dataset.contactIndex);
      const item = state.pipeline.find(entry => entry.id === itemId);
      const contact = item?.contacts?.[contactIndex];
      if (!contact) return;

      const noteKey = noteTab.dataset.note;
      const notes = contact.notes || {};
      const activeText = trim(notes[noteKey]);

      const textPre = tr.querySelector('.col-note .note-text');
      if (textPre) {
        textPre.textContent = activeText || '—';
      }
      tr.classList.add('is-peeking-note');
      return;
    }
  });

  root.addEventListener('mouseout', (event) => {
    const tab = event.target.closest('.msg-tab');
    if (tab) {
      const tr = tab.closest('tr');
      if (!tr) return;
      const itemId = tr.dataset.id;
      const contactIndex = Number(tr.dataset.contactIndex);
      const item = state.pipeline.find(entry => entry.id === itemId);
      const contact = item?.contacts?.[contactIndex];
      if (!contact) return;

      const contactKey = tr.dataset.contactKey;
      const activeMsgKey = state.activeMsgByContact?.get(contactKey) || 'message_mail_subject';
      const messages = contact.messages || {};
      const activeText = trim(messages[activeMsgKey]);

      const textPre = tr.querySelector('.col-message .msg-text');
      if (textPre) {
        textPre.textContent = activeText || '—';
      }
      tr.classList.remove('is-peeking-msg');
      return;
    }

    const noteTab = event.target.closest('.note-tab');
    if (noteTab) {
      const tr = noteTab.closest('tr');
      if (!tr) return;
      const itemId = tr.dataset.id;
      const contactIndex = Number(tr.dataset.contactIndex);
      const item = state.pipeline.find(entry => entry.id === itemId);
      const contact = item?.contacts?.[contactIndex];
      if (!contact) return;

      const contactKey = tr.dataset.contactKey;
      const activeNoteKey = state.activeNoteByContact?.get(contactKey) || 'note_general';
      const notes = contact.notes || {};
      const activeText = trim(notes[activeNoteKey]);

      const textPre = tr.querySelector('.col-note .note-text');
      if (textPre) {
        textPre.textContent = activeText || '—';
      }
      tr.classList.remove('is-peeking-note');
      return;
    }
  });

  root.addEventListener('input', (event) => {
    if (event.target.matches('[data-campaign-edit-field]')) {
      updateCampaignEditSaveState(event.target.closest('.outbound-campaign-edit'));
    }
    if (event.target.matches('[data-search]')) {
      state.search = event.target.value;
      scheduleCenterRenderPreservingInput(event.target);
    }
    if (event.target.matches('[data-table-filter]')) {
      const column = event.target.dataset.tableFilter;
      if (!column) return;
      const value = event.target.value.trim();
      if (value) state.tableFilters[column] = value;
      else delete state.tableFilters[column];
      ensureSelectedCompanyInFilter();
      scheduleCenterRenderPreservingInput(event.target);
    }
    const aoAction = event.target.dataset?.action;
    if (aoAction && aoAction.startsWith('ao-edit-')) {
      handleActiveOutreachInput(aoAction, event.target);
    }
  });

  root.addEventListener('change', (event) => {
    if (event.target.id === 'status-filter') {
      state.statusFilter = event.target.value;
      render(true);
    } else if (event.target.id === 'tag-filter') {
      state.tagFilter = event.target.value;
      render(true);
    }
    const aoAction = event.target.dataset?.action;
    if (aoAction && aoAction.startsWith('ao-')) {
      handleActiveOutreachAction(aoAction, event.target).catch((error) =>
        console.warn('[outbound] active outreach change failed', error),
      );
    }
  });

  root.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    if (state.ctx?.host?.querySelector('.outbound-research-drawer')) {
      event.preventDefault();
      closeResearchSettingsDrawer();
      return;
    }
    if (state.editingCampaignId && event.target.closest('.outbound-campaign-edit')) {
      event.preventDefault();
      state.editingCampaignId = '';
      renderLeft();
    }
  });
}

function render(force = false) {
  if (!force) {
    const hasInlineEditor = !!state.ctx?.host?.querySelector('.inline-editor');
    if (state.isScrolling || hasInlineEditor || activeOverlay) {
      state.pendingRenderAfterScroll = true;
      return;
    }
  }
  renderLeft();
  renderCenter(force);
  renderRight();
}

function renderLeft() {
  const root = state.ctx.host.querySelector('.outbound-left');
  if (!root) return;
  const campaigns = visibleCampaigns();
  const outreachActive = !!state.outreachView;
  const outreachCounts = state.selectedCampaignId
    ? activeOutreachCounts(state.selectedCampaignId)
    : { leadQueue: 0, engagements: 0, approvalInbox: 0, readyToSend: 0, replies: 0, done: 0 };
  const outreachPending = outreachCounts.leadQueue + outreachCounts.approvalInbox + outreachCounts.readyToSend + outreachCounts.replies;
  root.innerHTML = `
    <header class="outbound-pane-header">
      <div><span>${escapeHtml(t('outbound', 'Outbound'))}</span><h2>${escapeHtml(t('campaigns', 'Campaigns'))}</h2></div>
      <div class="outbound-actions">
        <button class="outbound-icon-button outbound-outreach-toggle${outreachActive ? ' is-active' : ''}" type="button" data-action="toggle-outreach" title="${escapeHtml(t('outreachToggle', 'Active Outreach'))}" aria-pressed="${outreachActive}">${outreachActive ? '◀ Funnel' : 'Outreach ▶'}${outreachPending ? ` <em>${outreachPending}</em>` : ''}</button>
        <button class="outbound-icon-button" type="button" data-action="new-campaign" title="${escapeHtml(t('newCampaign', 'Neue Campaign'))}" aria-label="${escapeHtml(t('newCampaign', 'Neue Campaign'))}">+</button>
      </div>
    </header>
    <div class="outbound-scroll">
      <section class="outbound-section" aria-label="${escapeHtml(t('campaigns', 'Campaigns'))}">
        ${campaigns.map(renderCampaignItem).join('') || `<div class="outbound-empty">${escapeHtml(t('noCampaigns', 'Keine Campaigns vorhanden.'))}</div>`}
      </section>
    </div>
  `;
}

function renderCampaignItem(campaign) {
  if (state.editingCampaignId === campaign.id) return renderCampaignEditItem(campaign);
  const metrics = campaignFunnelMetrics(campaign.id);
  const subtitle = campaign.payload?.subtitle || `${campaign.market || 'DACH'} · ${campaign.status || 'active'}`;
  const scope = campaign.payload?.scope || campaign.objective || '';
  const knowledge = campaignKnowledgeRefs(campaign);
  return `
    <article class="outbound-campaign-item" aria-current="${campaign.id === state.selectedCampaignId}">
      <div class="outbound-campaign-top">
        <button class="outbound-campaign-select" type="button" data-action="select-campaign" data-id="${escapeHtml(campaign.id)}">
          <strong>${escapeHtml(campaign.name)}</strong>
          <span>${escapeHtml(subtitle)}</span>
          ${scope ? `<em>${escapeHtml(scope)}</em>` : ''}
        </button>
        <div class="outbound-shard-actions" aria-label="Campaign Aktionen">
          <button type="button" data-icon="runbook" data-action="open-campaign-runbook" data-id="${escapeHtml(campaign.id)}" title="${escapeHtml(t('openRunbook', 'Campaign Runbook öffnen'))}" aria-label="${escapeHtml(t('openRunbook', 'Campaign Runbook öffnen'))}" ${knowledge.runbookId ? '' : 'disabled'}></button>
          <button class="is-primary-action" type="button" data-icon="import" data-action="import-source" data-id="${escapeHtml(campaign.id)}" title="${escapeHtml(t('importJob', 'Importjob anlegen'))}" aria-label="${escapeHtml(t('importJob', 'Importjob anlegen'))}"><b>Import</b></button>
          <button type="button" data-icon="edit" data-action="edit-campaign" data-id="${escapeHtml(campaign.id)}" title="${escapeHtml(t('editCampaign', 'Campaign bearbeiten'))}" aria-label="${escapeHtml(t('editCampaign', 'Campaign bearbeiten'))}"></button>
          <button type="button" data-icon="delete" data-action="delete-campaign" data-id="${escapeHtml(campaign.id)}" title="${escapeHtml(t('deleteCampaign', 'Campaign löschen'))}" aria-label="${escapeHtml(t('deleteCampaign', 'Campaign löschen'))}"></button>
        </div>
      </div>
      <div class="outbound-funnel" aria-label="Campaign Funnel">
        ${renderInputFunnelStage(campaign, metrics)}
        ${renderConversion(metrics.input, metrics.companyResearchDone, campaign, 'company_research')}
        ${renderFunnelStage(campaign, 'research', 'Research', t('offen', 'offen'), `${metrics.companyResearchDone} / ${metrics.companyResearchTotal}`, metrics.companyResearchOpen)}
        ${renderConversion(metrics.companyResearchDone, metrics.companyQualified, campaign, 'pipeline')}
        ${renderFunnelStage(campaign, 'qualified', t('company', 'Firmen'), t('qualifiziert', 'qualifiziert'), `${metrics.companyQualified} / ${metrics.companyQualifiedTotal}`, metrics.pipelineOpen, 'good')}
        ${renderConversion(metrics.companyQualified, metrics.contactQualified, campaign, 'contact_research')}
        ${renderFunnelStage(campaign, 'contact_qualified', t('contact_people', 'Kontakte'), t('qualifiziert', 'qualifiziert'), `${metrics.contactQualified} / ${metrics.contactQualifiedTotal}`, metrics.contactOpen)}
        ${renderConversion(metrics.contactQualified, metrics.leadQualified, campaign, 'lead_qualification')}
        ${renderFunnelStage(campaign, 'lead_qualified', t('lead_status', 'Leads'), t('qualifiziert', 'qualifiziert'), `${metrics.leadQualified} / ${metrics.leadQualifiedTotal}`, metrics.leadOpen)}
      </div>
    </article>
  `;
}

function renderInputFunnelStage(campaign, metrics) {
  const active = campaign.id === state.selectedCampaignId && state.filter === 'all';
  const progress = metrics.sourceCount ? Math.round((metrics.parsedSourceCount / metrics.sourceCount) * 100) : 0;
  const status = inputImportStatusLabel(metrics);
  const runningClass = metrics.runningSourceCount ? ' is-running' : '';
  return `
    <button
      class="outbound-funnel-stage outbound-funnel-input${runningClass}"
      type="button"
      data-filter="all"
      data-campaign-id="${escapeHtml(campaign.id)}"
      aria-pressed="${active}"
      title="${escapeHtml(t('filter_input_title', 'Input Unternehmen filtern'))}"
    >
      <span><b>Input</b><small>${escapeHtml(t('company', 'Unternehmen'))}</small></span>
      <i>${formatCount(metrics.input)}</i>
      <em>${escapeHtml(status)}</em>
      <span class="outbound-funnel-progress" aria-hidden="true">
        <span style="width:${Math.max(0, Math.min(100, progress))}%"></span>
      </span>
    </button>
  `;
}

function inputImportStatusLabel(metrics) {
  if (!metrics.sourceCount) return t('noImport', 'Noch kein Import');
  const parts = [`${formatCount(metrics.parsedSourceCount)} / ${formatCount(metrics.sourceCount)} ${t('processed', 'verarbeitet')}`];
  if (metrics.runningSourceCount) parts.push(`${formatCount(metrics.runningSourceCount)} ${t('running', 'läuft')}`);
  if (metrics.failedSourceCount) parts.push(`${formatCount(metrics.failedSourceCount)} ${t('failed', 'Fehler')}`);
  return parts.join(' · ');
}

function renderCampaignEditItem(campaign) {
  const subtitle = campaign.payload?.subtitle || `${campaign.market || 'DACH'} · ${campaign.status || 'active'}`;
  const scope = campaign.payload?.scope || campaign.objective || '';
  return `
    <article
      class="outbound-campaign-item outbound-campaign-edit"
      aria-current="${campaign.id === state.selectedCampaignId}"
      data-id="${escapeHtml(campaign.id)}"
      data-original-name="${escapeHtml(campaign.name)}"
      data-original-subtitle="${escapeHtml(subtitle)}"
      data-original-scope="${escapeHtml(scope)}"
    >
      <div class="outbound-campaign-edit-grid">
        <label>
          <span>${escapeHtml(t('title', 'Titel'))}</span>
          <input data-campaign-edit-field="name" value="${escapeHtml(campaign.name)}" required aria-required="true" />
        </label>
        <label>
          <span>${escapeHtml(t('subtitle', 'Untertitel'))}</span>
          <input data-campaign-edit-field="subtitle" value="${escapeHtml(subtitle)}" />
        </label>
        <label>
          <span>${escapeHtml(t('scopeIcp', 'Scope / ICP'))}</span>
          <textarea data-campaign-edit-field="scope" rows="3" placeholder="z.B. DACH SaaS, 50-500 MA, hoher Energieverbrauch, kaufkräftige Operations-Teams">${escapeHtml(scope)}</textarea>
        </label>
      </div>
      <div class="outbound-campaign-edit-actions">
        <button class="outbound-button" type="button" data-action="cancel-campaign-edit" data-id="${escapeHtml(campaign.id)}">${escapeHtml(t('cancel', 'Abbrechen'))}</button>
        <button class="outbound-button primary" type="button" data-action="save-campaign-edit" data-id="${escapeHtml(campaign.id)}" data-campaign-edit-save disabled>${escapeHtml(t('save', 'Speichern'))}</button>
      </div>
    </article>
  `;
}

function updateCampaignEditSaveState(editor) {
  if (!editor) return;
  const saveButton = editor.querySelector('[data-campaign-edit-save]');
  if (!saveButton) return;
  const name = editor.querySelector('[data-campaign-edit-field="name"]')?.value?.trim() || '';
  const subtitle = editor.querySelector('[data-campaign-edit-field="subtitle"]')?.value?.trim() || '';
  const scope = editor.querySelector('[data-campaign-edit-field="scope"]')?.value?.trim() || '';
  const dirty = name !== (editor.dataset.originalName || '')
    || subtitle !== (editor.dataset.originalSubtitle || '')
    || scope !== (editor.dataset.originalScope || '');
  saveButton.disabled = !name || !dirty;
}

function renderFunnelStage(campaign, filter, label, sublabel, value, openCount = 0, tone = '') {
  const active = campaign.id === state.selectedCampaignId && state.filter === filter;
  return `
    <button
      class="outbound-funnel-stage ${tone}"
      type="button"
      data-filter="${escapeHtml(filter)}"
      data-campaign-id="${escapeHtml(campaign.id)}"
      aria-pressed="${active}"
      title="${escapeHtml(t('filter_stage_title', '{0} {1} filtern', label, sublabel || ''))}"
    >
      <span><b>${escapeHtml(label)}</b>${sublabel ? `<small>${escapeHtml(sublabel)}</small>` : ''}</span>
      <i>${typeof value === 'number' ? formatCount(value) : escapeHtml(value)}</i>
      ${openCount ? `<em>${formatCount(openCount)} ${escapeHtml(t('offen', 'offen'))}</em>` : ''}
    </button>
  `;
}

function renderConversion(from, to, campaign, stage) {
  const openCount = automationOpenRecords(campaign.id, stage).length;
  const stageDef = getAutomationStageDefinition(stage);
  const ctaText = stageDef?.cta || t('automationStart', 'Automatisierung starten');
  const titleText = `${ctaText} · ${formatCount(openCount)} ${t('offen', 'offen')}`;
  return `
    <div class="outbound-conversion">
      <span>${conversionRate(from, to)}</span>
      <button
        type="button"
        data-action="open-automation"
        data-stage="${escapeHtml(stage)}"
        data-campaign-id="${escapeHtml(campaign.id)}"
        title="${escapeHtml(titleText)}"
        aria-label="${escapeHtml(titleText)}"
        ${openCount ? '' : 'disabled'}
      >${openCount ? '<span aria-hidden="true">▶</span>' : ''}<b>${escapeHtml(automationShortLabel(stage, openCount))}</b></button>
    </div>
  `;
}

function currentFilterLabel() {
  return ({
    all: t('filter_all', 'Input Unternehmen'),
    research: t('filter_research', 'Research offen'),
    qualified: t('filter_qualified', 'Unternehmen qualifiziert'),
    contact_qualified: t('filter_contact_qualified', 'Kontakt qualifiziert'),
    lead_qualified: t('filter_lead_qualified', 'Lead qualifiziert'),
    pipeline: t('filter_pipeline', 'Pipeline'),
    rejected: t('filter_rejected', 'Nicht passend'),
  })[state.filter] || t('filter_default', 'Alle');
}

function automationShortLabel(stage, openCount) {
  if (!openCount) return t('done', 'fertig');
  return ({
    company_research: t('filter_research', 'Research'),
    pipeline: t('filter_pipeline', 'Pipeline'),
    contact_research: t('contact_people', 'Kontakte'),
    lead_qualification: t('lead_status', 'Leads'),
  })[stage] || 'Start';
}

function formatCount(value) {
  const number = Number(value || 0);
  if (number >= 1000000) return number.toLocaleString('de-DE', { notation: 'compact', maximumFractionDigits: 1 });
  return number.toLocaleString('de-DE');
}

function renderSourceItem(source) {
  return `
    <div class="outbound-source-item">
      <strong>${escapeHtml(source.title)}</strong>
      <span>${escapeHtml(source.source_type)} · ${escapeHtml(source.status)} · ${source.imported_count || 0}/${source.row_count || 0} ${escapeHtml(t('companiesCount', 'Firmen'))}</span>
    </div>
  `;
}

function renderCenter(force = false) {
  const root = state.ctx.host.querySelector('.outbound-center');
  if (!root) return;
  state.centerResizeCleanup?.();
  state.centerResizeCleanup = null;
  const campaign = selectedCampaign();
  if (!campaign) {
    root.innerHTML = `<div class="outbound-empty">${escapeHtml(t('noCampaignSelected', 'Keine Campaign ausgewählt.'))}</div>`;
    return;
  }
  if (state.outreachView) {
    root.innerHTML = renderActiveOutreachShell(campaign);
    return;
  }

  const settings = getCampaignResearchSettings(campaign);
  const activeCols = buildActiveTableColumns(settings, state.viewMode);
  const activeColsKey = activeCols.map(c => c.id).join(',');
  if (state.lastActiveColsKey !== activeColsKey) {
    force = true;
    state.lastActiveColsKey = activeColsKey;
  }

  const campaignStatuses = extractUniqueStatuses(currentPipeline());
  const campaignTags = extractUniqueTags(currentPipeline());
  const hiddenCount = loadHiddenCompanies().length;

  const scrollUnified = root.querySelector('.outbound-table-scroll-unified');
  const tbody = root.querySelector('.crm-table tbody');

  // SMART INLINE UPDATE DETECT
  if (!force && scrollUnified && tbody) {
    // 1. Update counts in the header
    const countsEl = root.querySelector('.outbound-header-counts');
    if (countsEl) {
      countsEl.textContent = `${currentSources().length} ${t('importJobsCount', 'Importjobs')} · ${currentCompanies().length} ${t('companiesCount', 'Firmen')} · ${buildActiveTableColumns(settings, state.viewMode).length} ${t('columnsCount', 'Spalten')}`;
    }

    // 2. Update active research summary
    const summaryContainer = root.querySelector('.outbound-active-research-container');
    if (summaryContainer) {
      summaryContainer.innerHTML = renderActiveResearchSummary(campaign);
    }

    // 3. Update research activity panel
    const activityContainer = root.querySelector('.outbound-research-activity-container');
    if (activityContainer) {
      activityContainer.innerHTML = renderResearchActivityPanel(campaign);
    }

    // 4. Update Status/Tag selects and "Versteckte Firmen" count
    const statusSelect = root.querySelector('#status-filter');
    if (statusSelect) {
      const activeVal = state.statusFilter;
      statusSelect.innerHTML = `<option value="">${escapeHtml(t('allStatus', 'Alle Status'))}</option>` + campaignStatuses.map(s => `<option value="${escapeHtml(s)}" ${activeVal === s ? 'selected' : ''}>${escapeHtml(s)}</option>`).join('');
    }

    const tagSelect = root.querySelector('#tag-filter');
    if (tagSelect) {
      const activeVal = state.tagFilter;
      tagSelect.innerHTML = `<option value="">${escapeHtml(t('allTags', 'Alle Tags'))}</option>` + campaignTags.map(t => `<option value="${escapeHtml(t)}" ${activeVal === t ? 'selected' : ''}>${escapeHtml(t)}</option>`).join('');
    }

    const hiddenCountBadge = root.querySelector('#toggle-hidden-companies span:last-child');
    if (hiddenCountBadge) {
      hiddenCountBadge.textContent = hiddenCount;
    }

    // 5. Update tbody with table rows
    const rows = filteredQualificationRows();
    const visibleRows = rows.slice(0, OUTBOUND_TABLE_RENDER_LIMIT);
    const emptyCompanyMessage = state.filter === 'all'
      ? t('noCompaniesInCampaign', 'Noch keine Unternehmen in dieser Campaign.')
      : `${t('emptyFilter', 'Keine Firmen für diesen Filter:')} ${currentFilterLabel()}.`;

    const newHtml = visibleRows.length
      ? visibleRows.map(row => renderCRMCompanyRows(row)).join('')
      : `<tr><td colspan="${activeCols.length}" class="outbound-table-empty" style="text-align:center; padding: 40px; color: var(--outbound-muted);">${escapeHtml(emptyCompanyMessage)}</td></tr>`;

    if (state.lastTbodyHtml !== newHtml) {
      state.lastTbodyHtml = newHtml;
      tbody.innerHTML = newHtml;
    }

    setupCenterSplitResizing(root);
    return;
  }

  // FALLBACK FULL REDRAW (force === true or initial campaign select)
  const scrollTop = scrollUnified ? scrollUnified.scrollTop : 0;
  const scrollLeft = scrollUnified ? scrollUnified.scrollLeft : 0;
  const focusState = saveFocusState(root);

  root.innerHTML = `
    <header class="outbound-pane-header">
      <div><span>${escapeHtml(campaign.market || 'DACH')}</span><h2>${escapeHtml(campaign.name)}</h2></div>
      <div class="outbound-header-actions">
        <div class="outbound-active-research-container">
          ${renderActiveResearchSummary(campaign)}
        </div>

        <label class="outbound-toggle-label" style="display: flex; align-items: center; gap: 6px; font-size: 11px; cursor: pointer; user-select: none; margin-right: 6px; color: var(--outbound-muted);">
          <input type="checkbox" id="toggle-compact" ${state.viewMode === 'compact' ? 'checked' : ''} style="cursor: pointer; accent-color: var(--outbound-accent);" />
          <span>${escapeHtml(t('compactView', 'Kompakte Ansicht'))}</span>
        </label>

        <select id="status-filter" style="background: var(--outbound-surface-2); border: 1px solid var(--outbound-line); border-radius: var(--outbound-radius); padding: 4px 8px; font-size: 11px; font-weight: 600; color: var(--outbound-text); cursor: pointer; max-width: 120px; outline: none; margin-right: 6px;">
          <option value="">${escapeHtml(t('allStatus', 'Alle Status'))}</option>
          ${campaignStatuses.map(s => `<option value="${escapeHtml(s)}" ${state.statusFilter === s ? 'selected' : ''}>${escapeHtml(s)}</option>`).join('')}
        </select>

        <select id="tag-filter" style="background: var(--outbound-surface-2); border: 1px solid var(--outbound-line); border-radius: var(--outbound-radius); padding: 4px 8px; font-size: 11px; font-weight: 600; color: var(--outbound-text); cursor: pointer; max-width: 120px; outline: none; margin-right: 6px;">
          <option value="">${escapeHtml(t('allTags', 'Alle Tags'))}</option>
          ${campaignTags.map(t => `<option value="${escapeHtml(t)}" ${state.tagFilter === t ? 'selected' : ''}>${escapeHtml(t)}</option>`).join('')}
        </select>

        <button type="button" id="toggle-hidden-companies" class="outbound-button" style="font-size: 11px; font-weight: 600; padding: 0 8px; min-height: 30px; display: inline-flex; align-items: center; gap: 6px; border-color: var(--outbound-line); color: var(--outbound-text); background: var(--outbound-surface-2); margin-right: 6px; cursor: pointer;">
          <span>${escapeHtml(t('hiddenCompanies', 'Versteckte Firmen'))}</span>
          <span style="background: var(--outbound-accent); color: #000; font-size: 10px; font-weight: 800; padding: 1px 6px; border-radius: 999px;">${hiddenCount}</span>
        </button>

        <button
          class="outbound-icon-button outbound-icon-mask"
          type="button"
          data-icon="export"
          data-action="export-table"
          title="${escapeHtml(t('exportExcel', 'Tabelle als Excel exportieren'))}"
          aria-label="${escapeHtml(t('exportExcel', 'Tabelle als Excel exportieren'))}"
          style="margin-right: 6px;"
        ></button>
        <button
          class="outbound-icon-button outbound-icon-mask"
          type="button"
          data-icon="settings"
          data-action="open-research-settings"
          title="${escapeHtml(t('settingsFields', 'Research-Felder einstellen'))}"
          aria-label="${escapeHtml(t('settingsFields', 'Research-Felder einstellen'))}"
          style="margin-right: 12px;"
        ></button>
        <div class="outbound-muted outbound-header-counts">${currentSources().length} ${t('importJobsCount', 'Importjobs')} · ${currentCompanies().length} ${t('companiesCount', 'Firmen')} · ${buildActiveTableColumns(settings, state.viewMode).length} ${t('columnsCount', 'Spalten')}</div>
      </div>
    </header>
    <div class="outbound-center-content">
      <div class="outbound-research-activity-container">
        ${renderResearchActivityPanel(campaign)}
      </div>
      ${renderQualificationSplit(campaign)}
    </div>
  `;

  // Restore scroll & focus states
  const newScrollUnified = root.querySelector('.outbound-table-scroll-unified');
  if (newScrollUnified) {
    newScrollUnified.scrollTop = scrollTop;
    newScrollUnified.scrollLeft = scrollLeft;
  }
  restoreFocusState(root, focusState);

  setupCenterSplitResizing(root);
}

const OUTREACH_TABS = [
  { key: 'message_mail_subject', label: 'Betreff' },
  { key: 'message_mail_body', label: 'E-Mail' },
  { key: 'message_followup_1', label: 'Follow-up 1' },
  { key: 'message_followup_2', label: 'Follow-up 2' }
];

const STANDARD_NOTE_TABS = [
  { key: 'note_general', label: 'Allgemein' },
  { key: 'note_next_step', label: 'Nächster Schritt' }
];

function getNoteTabsForContact(contact) {
  const tabs = [...STANDARD_NOTE_TABS];
  if (contact?.notes) {
    Object.keys(contact.notes).forEach(key => {
      if (key.startsWith('note_') && !tabs.some(t => t.key === key)) {
        const label = key.slice(5).split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
        tabs.push({ key, label });
      }
    });
  }
  return tabs;
}

function renderHeaderCell(columnId, label, className = '', action = false) {
  return renderTableHeaderCell({ id: columnId, label, action }, className);
}

function replaceRowDOMInline(tr) {
  if (!tr) return;
  const itemId = tr.dataset.id;
  const row = filteredQualificationRows().find(r => (r.item?.id === itemId || r.company.id === itemId));
  if (row) {
    const newHtml = renderCRMCompanyRows(row);
    const temp = document.createElement('tbody');
    temp.innerHTML = newHtml;
    const newTr = temp.firstElementChild;
    if (newTr) {
      const parent = tr.parentNode;
      parent.replaceChild(newTr, tr);
      state.lastTbodyHtml = parent.innerHTML;
    }
  }
}

const COLUMN_CLASS_MAPPING = {
  // Company fields
  'legal_form': 'col-dept',
  'country': 'col-loc',
  'city': 'col-loc',
  'phone': 'col-contact',
  'email': 'col-contact',
  'domain': 'col-contact',
  'employee_count': 'col-dept',
  'revenue_eur': 'col-dept',
  'profit_eur': 'col-dept',
  'share_capital_eur': 'col-dept',
  'balance_sheet_total_eur': 'col-dept',
  'equity_eur': 'col-dept',
  'representative_1': 'col-role',
  'representative_2': 'col-role',
  'representative_3': 'col-role',
  'business_purpose': 'col-note',
  'tickers': 'col-dept',
  'financials_date': 'col-loc',
  'industry_wz': 'col-role',
  'vat_id': 'col-dept',
  'street': 'col-contact',
  'postal_code': 'col-loc',
  'registry_court': 'col-role',
  'registry_id': 'col-dept',
  'status': 'col-status',
  // Contact fields
  'contact.people': 'col-name',
  'contact.role': 'col-role',
  'contact.email': 'col-contact',
  'contact.linkedin': 'col-contact',
  'contact.phone': 'col-contact',
  'contact.fit': 'col-note',
  'contact.status': 'col-status',
  'contact.tags': 'col-tags',
  'lead.reason': 'col-note',
  'lead.status': 'col-status',
};

function buildActiveTableColumns(settings, viewMode) {
  const isCompact = viewMode === 'compact';
  const activeCompanyFieldIds = isCompact ? settings.fieldsCompact : settings.fields;
  const activeContactFieldIds = isCompact ? settings.contactFieldsCompact : settings.contactFields;

  const cols = [];

  // 1. Always start with Company Name
  cols.push({
    id: 'company.name',
    label: 'Unternehmen',
    className: 'col-company',
    type: 'company_name'
  });

  // 2. Active Company Fields
  (activeCompanyFieldIds || []).forEach(fieldId => {
    let label = settings.columnLabels?.[fieldId];
    if (!label) {
      const def = RESEARCH_FIELD_DEFS.find(([id]) => id === fieldId);
      label = def ? def[1] : (settings.customFields?.find(f => f.id === fieldId)?.label || fieldId);
    }
    const mappedClass = COLUMN_CLASS_MAPPING[fieldId] || 'col-dept';
    cols.push({
      id: `field.${fieldId}`,
      fieldId: fieldId,
      label: label,
      className: mappedClass,
      type: 'company_field'
    });
  });

  // 3. Active Contact Fields
  (activeContactFieldIds || []).forEach(fieldId => {
    let label = settings.columnLabels?.[fieldId];
    if (!label) {
      const def = CONTACT_FIELD_DEFS.find(([id]) => id === fieldId);
      label = def ? def[1] : (settings.customContactFields?.find(f => f.id === fieldId)?.label || fieldId);
    }

    if (fieldId === 'contact.people') {
      cols.push({
        id: 'contact.people',
        fieldId: 'contact.people',
        label: label,
        className: 'col-name',
        type: 'contact_people'
      });
    } else {
      const mappedClass = COLUMN_CLASS_MAPPING[fieldId] || 'col-role';
      cols.push({
        id: `contact_field.${fieldId}`,
        fieldId: fieldId,
        label: label,
        className: mappedClass,
        type: 'contact_field'
      });
    }
  });

  // 4. Outreach and Notes columns (only in Expanded view)
  if (!isCompact) {
    cols.push({
      id: 'outreach.type',
      label: 'Anschreiben',
      className: 'col-msgtype',
      type: 'outreach_type'
    });
    cols.push({
      id: 'outreach.draft',
      label: 'Entwurf',
      className: 'col-message',
      type: 'outreach_draft'
    });
    cols.push({
      id: 'note.type',
      label: 'Notiz-Typ',
      className: 'col-notetype',
      type: 'note_type'
    });
    cols.push({
      id: 'note.content',
      label: 'Notizen',
      className: 'col-note',
      type: 'note_content'
    });
  }

  return cols;
}

function resolveContactFieldHTML(row, contact, fieldId, contactKey) {
  if (fieldId === 'contact.role') {
    return escapeHtml(contact.role || contact.title || contact.position || '—');
  }
  if (fieldId === 'contact.departments') {
    return escapeHtml(contact.departments || contact.department || '—');
  }
  if (fieldId === 'contact.email') {
    return contact.email ? `<a href="mailto:${escapeHtml(contact.email)}">${escapeHtml(contact.email)}</a>` : '<span style="color: var(--outbound-muted);">Keine E-Mail</span>';
  }
  if (fieldId === 'contact.linkedin') {
    const url = contact.linkedin_url || contact.linkedin;
    return url ? `<a href="${escapeHtml(url)}" target="_blank" rel="noopener">LinkedIn</a>` : '<span style="color: var(--outbound-muted);">Kein LinkedIn</span>';
  }
  if (fieldId === 'contact.phone') {
    return contact.phone ? `<span>${escapeHtml(contact.phone)}</span>` : '<span style="color: var(--outbound-muted);">Keine Tel.</span>';
  }
  if (fieldId === 'contact.fit') {
    return escapeHtml(contact.qualification_status || contact.fit_reason || contact.relevance || '—');
  }
  if (fieldId === 'contact.status') {
    const statusChips = chipsFromMultiSelect(contact.status_field || contact.status);
    const statusHtml = statusChips.length
      ? statusChips.map(c => `<span class="chip" data-kind="status">${escapeHtml(c)}</span>`).join('')
      : '<span class="chip" style="opacity: 0.5;">+ Status</span>';
    return `<div class="chip-row">${statusHtml}</div>`;
  }
  if (fieldId === 'contact.tags') {
    const tagChips = chipsFromMultiSelect(contact.tags);
    const tagHtml = tagChips.length
      ? tagChips.map(c => `<span class="chip">${escapeHtml(c)}</span>`).join('')
      : '<span class="chip" style="opacity: 0.5;">+ Tag</span>';
    return `<div class="chip-row">${tagHtml}</div>`;
  }
  if (fieldId === 'lead.reason') {
    return escapeHtml(row.item?.lead_reason || row.item?.qualification_reason || '—');
  }
  if (fieldId === 'lead.status') {
    const status = row.item ? pipelineResearchStatus(row.item, 'lead_qualification') : '';
    const val = isAutomationActive(status) || isFailedStatus(status) || isCancelledStatus(status)
      ? automationStatusLabel(status)
      : (isLeadQualified(row.item) ? 'qualifiziert' : 'offen');
    return escapeHtml(val);
  }

  // Fallback for custom contact fields
  const aliases = [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')];
  const value = firstObjectValue(contact, aliases)
    || (row.item?.payload ? firstObjectValue(row.item.payload, aliases) : '')
    || (row.item?.payload?.knowledge_rows ? firstKnowledgeRowValue(row.item.payload.knowledge_rows, fieldId) : '');
  if (Array.isArray(value)) return escapeHtml(value.filter(Boolean).join(', ') || '—');
  return escapeHtml(String(value || '').trim() || '—');
}

function renderCRMCompanyRows(row) {
  const contacts = Array.isArray(row.item?.contacts) ? row.item.contacts : [];

  let html = '';
  const contactStatus = row.item ? pipelineResearchStatus(row.item, 'contact_research') : '';
  const leadStatus = row.item ? pipelineResearchStatus(row.item, 'lead_qualification') : '';
  const rowClass = tableRowStatusClass(combinedAutomationStatus(contactStatus, leadStatus));

  // Helper to match contact against active filters
  const contactMatchesFilters = (c) => {
    let statusMatch = true;
    let tagMatch = true;
    if (state.statusFilter) {
      const chips = chipsFromMultiSelect(c.status_field || c.status).map(x => x.toLowerCase());
      statusMatch = chips.includes(state.statusFilter.toLowerCase());
    }
    if (state.tagFilter) {
      const chips = chipsFromMultiSelect(c.tags).map(x => x.toLowerCase());
      tagMatch = chips.includes(state.tagFilter.toLowerCase());
    }
    return statusMatch && tagMatch;
  };

  const campaign = selectedCampaign();
  const settings = getCampaignResearchSettings(campaign);
  const activeCols = buildActiveTableColumns(settings, state.viewMode);

  if (contacts.length === 0) {
    const contactKey = row.item ? `${row.item.id}_0` : `${row.company.id}_0`;

    html += `
      <tr${rowClass} data-action="select-company-row" data-company-id="${escapeHtml(row.company.id)}" data-contact-key="${escapeHtml(contactKey)}" data-id="${escapeHtml(row.item?.id || row.company.id)}" data-contact-index="0">
    `;

    const contactCols = activeCols.filter(col => col.type === 'contact_people' || col.type === 'contact_field');
    let contactCellRendered = false;

    activeCols.forEach(col => {
      if (col.type === 'company_name') {
        html += `
          <td class="col-company">
            <div class="company-cell">
              <a href="${escapeHtml(row.company.website || '#')}" target="_blank" rel="noopener" class="company-name-title">${escapeHtml(row.company.name)}</a>
              ${row.company.domain ? `<a href="https://${escapeHtml(row.company.domain)}" target="_blank" rel="noopener" class="company-domain-link">${escapeHtml(row.company.domain)}</a>` : ''}
              ${row.company.qualification_status === 'rejected' ? '<span class="company-exclude-badge">Ausgeschlossen</span>' : ''}
              <div style="margin-top: 4px; display: flex; flex-direction: column; gap: 4px; font-size: 10px; color: var(--outbound-muted);">
                <span>Recht: ${escapeHtml(row.company.company_data?.legal_form || '—')}</span>
                <span>Mitarbeiter: ${escapeHtml(row.company.company_data?.employee_count || '—')}</span>
                <span>Umsatz: ${escapeHtml(row.company.company_data?.revenue_eur ? row.company.company_data.revenue_eur.toLocaleString('de-DE') + ' €' : '—')}</span>
              </div>
              <div style="margin-top: 8px; display: flex; gap: 4px; flex-wrap: wrap;">
                ${row.company.qualification_status === 'pending' ? `
                  <button class="mini-btn" style="background: rgba(34, 197, 94, 0.2); color: #86efac; border-color: rgba(34, 197, 94, 0.4);" type="button" data-action="qualify-company" data-id="${escapeHtml(row.company.id)}">Qualifizieren</button>
                  <button class="mini-btn" style="background: rgba(239, 68, 68, 0.2); color: #fca5a5; border-color: rgba(239, 68, 68, 0.4);" type="button" data-action="reject-company" data-id="${escapeHtml(row.company.id)}">Ausschließen</button>
                ` : `
                  <span class="outbound-badge ${row.company.qualification_status === 'qualified' ? 'good' : 'warn'}">
                    ${row.company.qualification_status === 'qualified' ? 'Qualifiziert' : 'Ausgeschlossen'}
                  </span>
                  ${row.company.qualification_status === 'qualified' && row.company.pipeline_status !== 'pipeline' ? `
                    <button class="mini-btn" type="button" data-action="send-pipeline" data-id="${escapeHtml(row.company.id)}">In Pipeline</button>
                  ` : ''}
                `}
                <button class="mini-btn" style="background: rgba(255, 255, 255, 0.05); color: var(--outbound-muted); border-color: var(--outbound-line);" type="button" data-action="hide-company" data-id="${escapeHtml(row.company.id)}">Ausblenden</button>
                <button class="mini-btn" style="background: rgba(255, 255, 255, 0.05); color: var(--outbound-muted); border-color: var(--outbound-line);" type="button" data-action="research-company" data-id="${escapeHtml(row.company.id)}" title="Unternehmensdaten nachrecherchieren">Nachrecherche</button>
              </div>
            </div>
          </td>
        `;
      } else if (col.type === 'company_field') {
        html += `<td class="${col.className}">${escapeHtml(companyResearchValue(row.company, col.fieldId))}</td>`;
      } else if (col.type === 'contact_people' || col.type === 'contact_field') {
        if (!contactCellRendered) {
          html += `
            <td class="col-name" colspan="${contactCols.length}" style="vertical-align: middle;">
              ${row.item
                ? `<button class="outbound-button" type="button" style="width: 100%; border-color: var(--outbound-accent); color: var(--outbound-accent);" data-action="research-contacts" data-id="${escapeHtml(row.item.id)}">Ansprechpartner recherchieren</button>`
                : `<button class="outbound-button" type="button" style="width: 100%; background: var(--outbound-accent); color:#000; font-weight:800;" data-action="send-pipeline" data-id="${escapeHtml(row.company.id)}">In Pipeline übernehmen</button>`
              }
            </td>
          `;
          contactCellRendered = true;
        }
      } else {
        html += `<td class="${col.className}" style="color: var(--outbound-muted); opacity: 0.5;">—</td>`;
      }
    });

    html += `</tr>`;
  } else {
    let activeIndex = getSelectedContactIndexForCompany(row.company.id, contacts);

    // Filter-Aware Picker adjustments
    if (state.statusFilter || state.tagFilter) {
      const activeContact = contacts[activeIndex];
      if (!activeContact || !contactMatchesFilters(activeContact)) {
        const firstMatchingIdx = contacts.findIndex(c => contactMatchesFilters(c));
        if (firstMatchingIdx !== -1) {
          activeIndex = firstMatchingIdx;
          setSelectedContactIndexForCompany(row.company.id, activeIndex);
        }
      }
    }

    const contact = contacts[activeIndex] || contacts[0];
    const contactKey = row.item ? `${row.item.id}_${activeIndex}` : `${row.company.id}_${activeIndex}`;

    const activeMsgKey = state.activeMsgByContact.get(contactKey) || 'message_mail_subject';
    const activeNoteKey = state.activeNoteByContact.get(contactKey) || 'note_general';

    html += `
      <tr${rowClass} data-action="select-company-row" data-company-id="${escapeHtml(row.company.id)}" data-contact-key="${escapeHtml(contactKey)}" data-id="${escapeHtml(row.item?.id || row.company.id)}" data-contact-index="${activeIndex}">
    `;

    activeCols.forEach(col => {
      if (col.type === 'company_name') {
        html += `
          <td class="col-company">
            <div class="company-cell">
              <a href="${escapeHtml(row.company.website || '#')}" target="_blank" rel="noopener" class="company-name-title">${escapeHtml(row.company.name)}</a>
              ${row.company.domain ? `<a href="https://${escapeHtml(row.company.domain)}" target="_blank" rel="noopener" class="company-domain-link">${escapeHtml(row.company.domain)}</a>` : ''}
              ${row.company.qualification_status === 'rejected' ? '<span class="company-exclude-badge">Ausgeschlossen</span>' : ''}
              <div style="margin-top: 4px; display: flex; flex-direction: column; gap: 4px; font-size: 10px; color: var(--outbound-muted);">
                <span>Recht: ${escapeHtml(row.company.company_data?.legal_form || '—')}</span>
                <span>Mitarbeiter: ${escapeHtml(row.company.company_data?.employee_count || '—')}</span>
                <span>Umsatz: ${escapeHtml(row.company.company_data?.revenue_eur ? row.company.company_data.revenue_eur.toLocaleString('de-DE') + ' €' : '—')}</span>
              </div>
              <div style="margin-top: 8px; display: flex; gap: 4px; flex-wrap: wrap;">
                ${row.company.qualification_status === 'pending' ? `
                  <button class="mini-btn" style="background: rgba(34, 197, 94, 0.2); color: #86efac; border-color: rgba(34, 197, 94, 0.4);" type="button" data-action="qualify-company" data-id="${escapeHtml(row.company.id)}">Qualifizieren</button>
                  <button class="mini-btn" style="background: rgba(239, 68, 68, 0.2); color: #fca5a5; border-color: rgba(239, 68, 68, 0.4);" type="button" data-action="reject-company" data-id="${escapeHtml(row.company.id)}">Ausschließen</button>
                ` : `
                  <span class="outbound-badge ${row.company.qualification_status === 'qualified' ? 'good' : 'warn'}">
                    ${row.company.qualification_status === 'qualified' ? 'Qualifiziert' : 'Ausgeschlossen'}
                  </span>
                  ${row.company.qualification_status === 'qualified' && row.company.pipeline_status !== 'pipeline' ? `
                    <button class="mini-btn" type="button" data-action="send-pipeline" data-id="${escapeHtml(row.company.id)}">In Pipeline</button>
                  ` : ''}
                `}
                <button class="mini-btn" style="background: rgba(255, 255, 255, 0.05); color: var(--outbound-muted); border-color: var(--outbound-line);" type="button" data-action="hide-company" data-id="${escapeHtml(row.company.id)}">Ausblenden</button>
                <button class="mini-btn" style="background: rgba(255, 255, 255, 0.05); color: var(--outbound-muted); border-color: var(--outbound-line);" type="button" data-action="research-company" data-id="${escapeHtml(row.company.id)}" title="Unternehmensdaten nachrecherchieren">Nachrecherche</button>
              </div>
            </div>
          </td>
        `;
      } else if (col.type === 'company_field') {
        html += `<td class="${col.className}">${escapeHtml(companyResearchValue(row.company, col.fieldId))}</td>`;
      } else if (col.type === 'contact_people') {
        html += `
          <td class="col-name">
            <div class="name-picker" style="display: flex; flex-direction: column; gap: 6px; align-items: flex-start; min-width: 120px;">
              ${contacts.map((c, idx) => {
                if (state.statusFilter || state.tagFilter) {
                  if (!contactMatchesFilters(c)) return '';
                }
                const active = idx === activeIndex;
                const isCurrentAttr = active ? 'aria-current="true"' : '';
                const nameLabel = escapeHtml(c.name || c.firstname || c.lastname || '—');
                return `
                  <span class="person-picker-row" style="display: inline-flex; align-items: center; gap: 4px; width: 100%;">
                    <button
                      class="cell-select person-name-btn"
                      type="button"
                      data-action="select-company-contact"
                      data-company-id="${escapeHtml(row.company.id)}"
                      data-index="${idx}"
                      ${isCurrentAttr}
                      style="flex: 1; text-align: left; text-overflow: ellipsis; overflow: hidden; white-space: nowrap; font-size: 11px;"
                    >${nameLabel}</button>
                    <button
                      class="mini-btn delete-contact-btn"
                      type="button"
                      data-action="delete-company-contact"
                      data-item-id="${escapeHtml(row.item?.id || '')}"
                      data-index="${idx}"
                      title="Kontakt löschen"
                      style="padding: 2px 4px; font-size: 10px; opacity: 0.7;"
                    >🗑</button>
                  </span>
                `;
              }).join('')}
              ${row.item ? `
                <button class="mini-btn" type="button" data-action="research-contacts" data-id="${escapeHtml(row.item.id)}" style="margin-top: 6px; width: 100%; text-align: center;" title="Ansprechpartner nachrecherchieren">Nachrecherche</button>
              ` : ''}
            </div>
          </td>
        `;
      } else if (col.type === 'contact_field') {
        html += `<td class="${col.className}">${resolveContactFieldHTML(row, contact, col.fieldId, contactKey)}</td>`;
      } else if (col.type === 'outreach_type') {
        html += `
          <td class="col-msgtype">
            <div class="msg-tabs">
              ${OUTREACH_TABS.map(tab => {
                const active = tab.key === activeMsgKey ? 'aria-selected="true"' : '';
                return `<button type="button" class="msg-tab" data-msg="${tab.key}" ${active}>${escapeHtml(getTabLabel(tab.key, tab.label))}</button>`;
              }).join('')}
            </div>
            <div style="margin-top: 8px;">
              <button class="outbound-button" type="button" data-action="generate-outreach" style="width: 100%; font-size: 11px; padding: 4px 8px; border-color: var(--outbound-accent); color: var(--outbound-accent);" ${state.generatingOutreach.has(contactKey) ? 'disabled' : ''}>✨ ${t('generate', 'Generieren')}</button>
            </div>
          </td>
        `;
      } else if (col.type === 'outreach_draft') {
        html += `
          <td class="col-message">
            ${state.generatingOutreach.has(contactKey) ? `
              <div class="outbound-message-loading">
                <span class="outbound-spinner"></span>
                <div>
                  <strong>${escapeHtml(t('generatingOutreachEllipsis', 'Anschreiben wird generiert...'))}</strong>
                  <div style="font-size: 11px; color: var(--outbound-muted); margin-top: 2px;">
                    ${escapeHtml(state.generatingOutreach.get(contactKey)?.statusText || t('waitingForApi', 'Warte auf API...'))}
                  </div>
                </div>
              </div>
            ` : `
              <div class="msg-content" data-active-key="${escapeHtml(activeMsgKey)}">
                <pre class="msg-text">${escapeHtml(trim(contact.messages?.[activeMsgKey]) || '—')}</pre>
              </div>
            `}
          </td>
        `;
      } else if (col.type === 'note_type') {
        html += `
          <td class="col-notetype">
            <div class="note-tabs">
              ${getNoteTabsForContact(contact).map(tab => {
                const active = tab.key === activeNoteKey ? 'aria-selected="true"' : '';
                const isCustom = !STANDARD_NOTE_TABS.some(t => t.key === tab.key);
                const delBtn = isCustom ? `<button class="mini-btn" data-action="delete-note-type" data-note-key="${escapeHtml(tab.key)}" title="${escapeHtml(t('delete', 'Löschen'))}">🗑</button>` : '';
                return `
                  <div class="note-tab-wrap">
                    <button type="button" class="note-tab" data-note="${tab.key}" ${active}>${escapeHtml(getTabLabel(tab.key, tab.label))}</button>
                    ${delBtn}
                  </div>
                `;
              }).join('')}
              <button type="button" class="mini-btn mini-btn-add" data-action="add-note-type">+ ${t('noteType', 'Notiztyp')}</button>
            </div>
          </td>
        `;
      } else if (col.type === 'note_content') {
        html += `
          <td class="col-note">
            <div class="note-content" data-active-note-key="${escapeHtml(activeNoteKey)}">
              <pre class="note-text">${escapeHtml(trim(contact.notes?.[activeNoteKey]) || '—')}</pre>
            </div>
          </td>
        `;
      }
    });

    html += `</tr>`;
  }

  return html;
}

function renderQualificationSplit(campaign) {
  const rows = filteredQualificationRows();
  const visibleRows = rows.slice(0, OUTBOUND_TABLE_RENDER_LIMIT);
  const emptyCompanyMessage = state.filter === 'all'
    ? t('noCompaniesCampaign', 'Noch keine Unternehmen in dieser Campaign.')
    : t('noCompaniesFilter', 'Keine Unternehmen für Filter: {0}.', currentFilterLabel());

  const settings = getCampaignResearchSettings(campaign);
  const activeCols = buildActiveTableColumns(settings, state.viewMode);

  const rowsHtml = visibleRows.length
    ? `${visibleRows.map(row => renderCRMCompanyRows(row)).join('')}${renderTableLimitRow(rows.length, activeCols.length)}`
    : `
    <tr><td colspan="${activeCols.length}" class="outbound-table-empty" style="text-align:center; padding: 40px; color: var(--outbound-muted);">${escapeHtml(emptyCompanyMessage)}</td></tr>
  `;
  state.lastTbodyHtml = rowsHtml;

  const headersHtml = activeCols.map(col => {
    const isAction = col.type === 'outreach_type' || col.type === 'outreach_draft' || col.type === 'note_type' || col.type === 'note_content';
    if (isAction) {
      return `<th class="${col.className}">${escapeHtml(col.label)}</th>`;
    }
    return renderHeaderCell(col.id, col.label, col.className);
  }).join('');

  return `
    <div class="outbound-split-workbench" data-outbound-center-split>
      <div class="outbound-unified-workbench" data-view="${state.viewMode}">
        <div class="outbound-table-scroll-unified">
          <table class="crm-table">
            <thead>
              <tr>
                ${headersHtml}
              </tr>
            </thead>
            <tbody>
              ${rowsHtml}
            </tbody>
          </table>
        </div>
      </div>
      <button class="outbound-center-resizer" type="button" data-outbound-center-resizer aria-label="${escapeHtml(t('resizeDetailsPane', 'Tabellen- und Detailbereich anpassen'))}"></button>
      <aside class="outbound-pane outbound-right" aria-label="${escapeHtml(t('researchDetails', 'Research Details'))}">
        ${state.activeView === 'pipeline' ? renderPipelineDetail() : renderCompanyDetail()}
      </aside>
    </div>
  `;
}

function renderActiveResearchSummary(campaign) {
  const counts = researchActivityCounts(campaign.id);
  const active = [
    [t('running', 'läuft'), counts.running],
    [t('waiting', 'wartet'), counts.waiting],
    [t('failed', 'Fehler'), counts.failed],
    [t('cancelled', 'abgebrochen'), counts.cancelled],
  ].filter(([, count]) => count > 0);
  if (!active.length) return '';
  const tone = counts.failed || counts.cancelled ? 'warn' : counts.running ? 'running' : 'waiting';
  return `
    <div class="outbound-active-research ${tone}" title="CTOX Research Status">
      <span></span>
      <b>${active.map(([label, count]) => `${formatCount(count)} ${label}`).join(' · ')}</b>
    </div>
  `;
}

function renderResearchActivityPanel(campaign) {
  const activity = researchActivityRows(campaign.id);
  const counts = researchActivityCounts(campaign.id, activity);
  if (!counts.total) return '';
  const isProblem = counts.failed || counts.cancelled;
  const title = counts.running
    ? t('automationActiveResearch', 'CTOX arbeitet: {0} läuft{1}{2}',
        formatCount(counts.running),
        counts.waiting ? t('automationActiveResearchWaiting', ' · {0} wartet', formatCount(counts.waiting)) : '',
        isProblem ? t('automationActiveResearchFailed', ' · {0} Fehler', formatCount(counts.failed + counts.cancelled)) : ''
      )
    : t('automationIdleResearch', 'CTOX arbeitet gerade nicht: {0} wartet{1}',
        formatCount(counts.waiting),
        isProblem ? t('automationActiveResearchFailed', ' · {0} Fehler', formatCount(counts.failed + counts.cancelled)) : ''
      );
  const items = activity
    .filter((item) => item.kind !== 'done' && item.kind !== 'idle')
    .slice(0, 5);
  return `
    <section class="outbound-research-activity ${isProblem ? 'warn' : counts.running ? 'running' : 'waiting'}" aria-label="CTOX Research Aktivität">
      <div>
        <span>CTOX Research</span>
        <strong>${escapeHtml(title)}</strong>
      </div>
      <div class="outbound-research-activity-counts">
        ${renderActivityCount(t('running', 'läuft'), counts.running)}
        ${renderActivityCount(t('waiting', 'wartet'), counts.waiting)}
        ${renderActivityCount(t('failed', 'Fehler'), counts.failed, counts.failed ? 'warn' : '')}
        ${renderActivityCount(t('cancelled', 'abgebrochen'), counts.cancelled, counts.cancelled ? 'warn' : '')}
      </div>
      ${items.length ? `
        <ol>
          ${items.map((item) => `
            <li class="${escapeHtml(item.kind)}">
              <b>${escapeHtml(item.label)}</b>
              <span>${escapeHtml(item.statusLabel)}</span>
            </li>
          `).join('')}
        </ol>
      ` : ''}
    </section>
  `;
}

function renderActivityCount(label, count, tone = '') {
  return `<span class="${escapeHtml(tone)}"><b>${formatCount(count)}</b> ${escapeHtml(label)}</span>`;
}

function researchActivityCounts(campaignId, rows = researchActivityRows(campaignId)) {
  return rows.reduce((counts, item) => {
    if (item.kind === 'running') counts.running += 1;
    else if (item.kind === 'waiting') counts.waiting += 1;
    else if (item.kind === 'failed' || item.kind === 'blocked') counts.failed += 1;
    else if (item.kind === 'cancelled') counts.cancelled += 1;
    counts.total += item.kind === 'idle' || item.kind === 'done' ? 0 : 1;
    return counts;
  }, { running: 0, waiting: 0, failed: 0, cancelled: 0, total: 0 });
}

function researchActivityRows(campaignId) {
  const scoped = campaignScopedRows(state, campaignId);
  const companies = dedupeCompanies(scoped.companies);
  const pipeline = dedupePipelineItems(scoped.pipeline);
  const rows = [
    ...companies.map((company) => activityRowFromStatus(company.name, t('company', 'Company'), companyResearchStatus(company))),
    ...pipeline.map((item) => activityRowFromStatus(item.company_name, t('contact_status', 'Kontakt'), pipelineResearchStatus(item, 'contact_research'))),
    ...pipeline.map((item) => activityRowFromStatus(item.company_name, t('lead_status', 'Lead'), pipelineResearchStatus(item, 'lead_qualification'))),
  ];
  const priority = { running: 0, failed: 1, blocked: 1, cancelled: 2, waiting: 3, idle: 4, done: 5 };
  return rows.sort((a, b) => (priority[a.kind] ?? 9) - (priority[b.kind] ?? 9));
}

function activityRowFromStatus(name, stageLabel, status) {
  const kind = automationStatusKind(status);
  return {
    label: `${stageLabel}: ${name || t('dataset', 'Datensatz')}`,
    kind,
    statusLabel: automationStatusLabel(status),
  };
}

function renderTableLimitRow(total, columnCount) {
  if (total <= OUTBOUND_TABLE_RENDER_LIMIT) return '';
  return `
    <tr>
      <td colspan="${columnCount}" class="outbound-table-empty">
        ${escapeHtml(t('rowsVisibleHint', '{0} von {1} Zeilen sichtbar. Suche oder Filter eingrenzen.', OUTBOUND_TABLE_RENDER_LIMIT, formatCount(total)))}
      </td>
    </tr>
  `;
}

function companyTableColumns(fields) {
  return [
    { id: 'company.name', label: t('company', 'Unternehmen'), value: (row) => row.company.name, primary: true },
    { id: 'company.domain', label: t('domain', 'Domain'), value: (row) => row.company.domain || domainFromUrl(row.company.website) || '' },
    ...fields.map((field) => ({
      id: `field.${field.id}`,
      label: field.label,
      value: (row) => companyResearchValue(row.company, field.id),
    })),
    { id: 'company.qualification', label: t('qualification', 'Qualifizierung'), value: (row) => labelQualification(row.company.qualification_status), badge: true },
  ];
}

function contactTableColumns(settings = getCampaignResearchSettings(selectedCampaign())) {
  const fields = contactFieldsForPrompt(settings);
  return [
    ...fields.map((field) => ({
      id: field.id,
      label: field.label,
      value: (row) => contactColumnValue(row.item, field.id),
      primary: field.id === 'contact.people',
    })),
    { id: 'contact.action', label: t('action', 'Aktion'), value: () => '', action: true },
  ];
}

function renderTableHeaderCell(column, className = '') {
  const sorted = state.tableSort?.column === column.id ? state.tableSort.direction : '';
  const filterValue = state.tableFilters[column.id] || '';
  const sortLabel = sorted === 'asc' ? t('sortedAsc', 'Aufsteigend sortiert') : sorted === 'desc' ? t('sortedDesc', 'Absteigend sortiert') : t('sort', 'Sortieren');
  const classAttr = className ? ` class="${escapeHtml(className)}"` : '';
  return `
    <th${classAttr}>
      <div class="outbound-table-column-head">
        <button
          type="button"
          data-action="sort-table"
          data-column="${escapeHtml(column.id)}"
          aria-label="${escapeHtml(`${column.label} ${sortLabel}`)}"
          aria-sort="${sorted === 'asc' ? 'ascending' : sorted === 'desc' ? 'descending' : 'none'}"
          ${column.action ? 'disabled' : ''}
        >
          <span>${escapeHtml(column.label)}</span>
          ${column.action ? '' : `<i>${sorted === 'asc' ? '↑' : sorted === 'desc' ? '↓' : '↕'}</i>`}
        </button>
        ${column.action ? '<span class="outbound-table-filter-placeholder"></span>' : `
          <input
            data-table-filter="${escapeHtml(column.id)}"
            value="${escapeHtml(filterValue)}"
            placeholder="${escapeHtml(t('filter', 'Filtern'))}"
            aria-label="${escapeHtml(t('filterAria', '{0} filtern', column.label))}"
          />
        `}
      </div>
    </th>
  `;
}

function renderCompanyDataRow(row, columns) {
  const company = row.company;
  const status = companyResearchStatus(company);
  const rowClass = tableRowStatusClass(status);
  return `
    <tr${rowClass} data-action="select-company" data-id="${escapeHtml(company.id)}" aria-current="${company.id === state.selectedCompanyId}">
      ${columns.map((column) => {
        if (column.primary) {
          return `<td><strong>${escapeHtml(column.value(row) || '-')}</strong>${renderInlineResearchStatus(labelResearch(status), status)}</td>`;
        }
        if (column.badge) {
          return `<td><span class="outbound-badge ${company.qualification_status === 'qualified' ? 'good' : company.qualification_status === 'rejected' ? 'warn' : ''}">${escapeHtml(column.value(row) || '-')}</span></td>`;
        }
        return `<td>${escapeHtml(column.value(row) || '-')}</td>`;
      }).join('')}
    </tr>
  `;
}

function renderContactQualificationRow(row, columns) {
  const { company, item } = row;
  const contactStatus = pipelineResearchStatus(item, 'contact_research');
  const leadStatus = pipelineResearchStatus(item, 'lead_qualification');
  const rowClass = tableRowStatusClass(combinedAutomationStatus(contactStatus, leadStatus));
  const action = item
    ? `<button class="outbound-button" type="button" data-action="research-contacts" data-id="${escapeHtml(item.id)}">Ansprechpartner</button>`
    : `<button class="outbound-button" type="button" data-action="send-pipeline" data-id="${escapeHtml(company.id)}">Pipeline</button>`;
  return `
    <tr${rowClass} data-action="${item ? 'select-pipeline' : 'select-company'}" data-id="${escapeHtml(item?.id || company.id)}" aria-current="${company.id === state.selectedCompanyId || item?.id === state.selectedPipelineId}">
      ${columns.map((column) => {
        if (column.action) return `<td>${action}</td>`;
        if (column.primary) {
          return `<td><strong>${escapeHtml(column.value(row) || '-')}</strong>${renderInlineResearchStatus(item?.stage || labelPipeline(company.pipeline_status), combinedAutomationStatus(contactStatus, leadStatus))}</td>`;
        }
        if (column.id === 'contact.status') return `<td><span class="outbound-badge ${badgeClassForStatus(contactStatus, isContactQualified(item))}">${escapeHtml(column.value(row))}</span></td>`;
        if (column.id === 'lead.status') return `<td><span class="outbound-badge ${badgeClassForStatus(leadStatus, isLeadQualified(item))}">${escapeHtml(column.value(row))}</span></td>`;
        return `<td>${escapeHtml(column.value(row) || '-')}</td>`;
      }).join('')}
    </tr>
  `;
}

function tableRowStatusClass(status) {
  if (isAutomationActive(status)) return ' class="is-updating"';
  if (isFailedStatus(status) || isCancelledStatus(status)) return ' class="is-failed"';
  return '';
}

function combinedAutomationStatus(...statuses) {
  return statuses.find(isFailedStatus)
    || statuses.find(isCancelledStatus)
    || statuses.find((status) => automationStatusKind(status) === 'running')
    || statuses.find((status) => automationStatusKind(status) === 'waiting')
    || statuses.find(Boolean)
    || '';
}

function contactPeopleLabel(item) {
  const contactCount = Array.isArray(item?.contacts) ? item.contacts.length : 0;
  return contactCount ? `${contactCount} gefunden` : 'offen';
}

function contactStatusLabel(item) {
  const status = pipelineResearchStatus(item, 'contact_research');
  if (isAutomationActive(status) || isFailedStatus(status) || isCancelledStatus(status)) return automationStatusLabel(status);
  return isContactQualified(item) ? 'qualifiziert' : 'offen';
}

function leadStatusLabel(item) {
  const status = pipelineResearchStatus(item, 'lead_qualification');
  if (isAutomationActive(status) || isFailedStatus(status) || isCancelledStatus(status)) return automationStatusLabel(status);
  return isLeadQualified(item) ? 'qualifiziert' : 'offen';
}

function contactColumnValue(item, fieldId) {
  if (fieldId === 'contact.people') return contactPeopleLabel(item);
  if (fieldId === 'contact.status') return contactStatusLabel(item);
  if (fieldId === 'lead.status') return leadStatusLabel(item);
  const contacts = Array.isArray(item?.contacts) ? item.contacts : [];
  const first = contacts[0] || {};
  const payload = item?.payload || {};
  const knowledgeRows = Array.isArray(payload.knowledge_rows) ? payload.knowledge_rows : [];
  const direct = {
    'contact.role': first.role || first.title || first.position,
    'contact.email': first.email || first.e_mail,
    'contact.linkedin': first.linkedin_url || first.linkedin || first.profile_url,
    'contact.phone': first.phone || first.telephone,
    'contact.fit': first.qualification_status || first.fit_reason || first.relevance || payload.contact_fit,
    'lead.reason': item?.lead_reason || item?.qualification_reason || payload.lead_reason,
  };
  const value = direct[fieldId]
    || firstObjectValue(first, [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')])
    || firstObjectValue(payload, [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')])
    || firstKnowledgeRowValue(knowledgeRows, fieldId);
  if (Array.isArray(value)) return value.filter(Boolean).join(', ') || '-';
  return String(value || '').trim() || '-';
}

function firstKnowledgeRowValue(rows, fieldId) {
  const aliases = [fieldId, fieldId.replace(/^contact\./, ''), fieldId.replace(/^lead\./, '')];
  for (const row of rows || []) {
    const value = firstObjectValue(row, aliases);
    if (value) return value;
  }
  return '';
}

function automationOpenRecords(campaignId, stage) {
  const scoped = campaignScopedRows(state, campaignId);
  const companies = dedupeCompanies(scoped.companies);
  const pipeline = dedupePipelineItems(scoped.pipeline);
  if (stage === 'company_research') {
    return companies.filter((company) => !isCompanyResearchDone(company) && !isQueuedStatus(company.research_status));
  }
  if (stage === 'pipeline') {
    const existingCompanyIds = new Set(pipeline.map((item) => item.company_id));
    return companies.filter((company) => (
      !existingCompanyIds.has(company.id)
      && !isQueuedStatus(company.pipeline_status)
      && (company.qualification_status === 'qualified' || isCompanyResearchDone(company))
    ));
  }
  if (stage === 'contact_research') {
    return pipeline.filter((item) => !isContactQualified(item) && !isQueuedStatus(item.contact_research_status));
  }
  if (stage === 'lead_qualification') {
    return pipeline.filter((item) => isContactQualified(item) && !isLeadQualified(item) && !isQueuedStatus(item.outreach_status));
  }
  return [];
}

function openAutomationDrawer(stage, campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId) || selectedCampaign();
  const definition = getAutomationStageDefinition(stage);
  const root = state.ctx.host.querySelector('[data-outbound-root]');
  if (!campaign || !definition || !root) return;
  state.selectedCampaignId = campaign.id;
  closeAutomationDrawer();
  const records = automationOpenRecords(campaign.id, stage);
  const batchSize = Math.min(OUTBOUND_BATCH_DEFAULT, OUTBOUND_BATCH_LIMIT, records.length || OUTBOUND_BATCH_DEFAULT);
  const drawer = document.createElement('aside');
  drawer.className = 'outbound-automation-drawer';
  drawer.setAttribute('role', 'dialog');
  drawer.setAttribute('aria-modal', 'true');
  drawer.innerHTML = renderAutomationDrawer(campaign, stage, records.length, batchSize);
  root.append(drawer);
}

function closeAutomationDrawer() {
  state.ctx?.host?.querySelector('.outbound-automation-drawer')?.remove();
}

function renderAutomationDrawer(campaign, stage, openCount, batchSize) {
  const definition = getAutomationStageDefinition(stage);
  const nextCount = Math.min(openCount, batchSize, OUTBOUND_BATCH_LIMIT);
  const selectedSize = [25, 50, 100].find((size) => size >= nextCount) || OUTBOUND_BATCH_LIMIT;
  const allOption = openCount > OUTBOUND_BATCH_LIMIT
    ? `<option value="all">${escapeHtml(t('automationAllOpenRuns', '{0} offene in {1}er-Runs starten', formatCount(openCount), 100))}</option>`
    : '';
  return `
    <div class="outbound-automation-backdrop" data-action="close-automation"></div>
    <section class="outbound-automation-panel" data-stage="${escapeHtml(stage)}" data-campaign-id="${escapeHtml(campaign.id)}">
      <header class="outbound-automation-header">
        <div>
          <span>${escapeHtml(t('automationHeadline', 'CTOX Automation'))}</span>
          <h3>${escapeHtml(definition.label)}</h3>
          <p>${escapeHtml(definition.description)}</p>
        </div>
        <button class="outbound-icon-button" type="button" data-action="close-automation" aria-label="${escapeHtml(t('close', 'Schließen'))}">×</button>
      </header>
      <div class="outbound-automation-body">
        <dl>
          <div><dt>${escapeHtml(t('campaign', 'Campaign'))}</dt><dd>${escapeHtml(campaign.name)}</dd></div>
          <div><dt>${escapeHtml(t('open', 'Offen'))}</dt><dd>${formatCount(openCount)}</dd></div>
          <div><dt>${escapeHtml(t('automationNextRun', 'Nächster Run'))}</dt><dd>${escapeHtml(t('automationBatchSize', '{0} Datensätze', formatCount(nextCount)))}</dd></div>
          <div><dt>${escapeHtml(t('automationSafety', 'Sicherheit'))}</dt><dd>${escapeHtml(t('automationSafetyDesc', '{0} pro Run', OUTBOUND_BATCH_LIMIT))}</dd></div>
        </dl>
        <label>
          <span>${escapeHtml(t('automationScope', 'Umfang'))}</span>
          <select data-automation-batch-size ${openCount ? '' : 'disabled'}>
            ${[25, 50, 100].map((size) => `<option value="${size}" ${size === selectedSize ? 'selected' : ''}>${escapeHtml(t('automationBatchSize', '{0} Datensätze', size))}</option>`).join('')}
            ${allOption}
          </select>
        </label>
      </div>
      <footer class="outbound-automation-footer">
        <span class="outbound-muted">${openCount > OUTBOUND_BATCH_LIMIT ? escapeHtml(t('automationBatchWarning', 'Alle offenen Researches laufen als kontrollierte CTOX Runs mit je maximal {0} Datensätzen.', OUTBOUND_BATCH_LIMIT)) : escapeHtml(t('automationBatchInfo', 'Kontrollierter CTOX Batch-Run.'))}</span>
        <button class="outbound-button primary" type="button" data-action="start-automation" ${openCount ? '' : 'disabled'}>${escapeHtml(definition.cta)}</button>
      </footer>
    </section>
  `;
}

async function startAutomationBatch() {
  const drawer = state.ctx?.host?.querySelector('.outbound-automation-panel');
  if (!drawer) return;
  const stage = drawer.dataset.stage;
  const campaignId = drawer.dataset.campaignId;
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign || !AUTOMATION_STAGES[stage]) return;
  const requested = drawer.querySelector('[data-automation-batch-size]')?.value || String(OUTBOUND_BATCH_DEFAULT);
  const openRecords = automationOpenRecords(campaign.id, stage);
  const records = requested === 'all'
    ? openRecords
    : openRecords.slice(0, clampNumber(Number(requested) || OUTBOUND_BATCH_DEFAULT, 1, OUTBOUND_BATCH_LIMIT));
  if (!records.length) return;
  const button = drawer.querySelector('[data-action="start-automation"]');
  if (button) {
    button.setAttribute('disabled', 'disabled');
    button.textContent = 'Run wird gestartet...';
  }
  state.selectedCampaignId = campaign.id;
  closeAutomationDrawer();
  window.setTimeout(() => {
    markAutomationRecordsQueued(stage, records);
    render();
    window.setTimeout(() => {
      loadAll().then(render).catch((error) => console.warn('[outbound] refresh after automation start failed', error));
    }, 0);
    runAutomationBatchInBackground(stage, campaign, records);
  }, 0);
}

function markAutomationRecordsQueued(stage, records) {
  const now = Date.now();
  const ids = new Set(records.flatMap((record) => [record.id, ...(record.duplicate_company_ids || [])].filter(Boolean)));
  if (stage === 'company_research') {
    state.companies = state.companies.map((company) => (
      ids.has(company.id) ? { ...company, research_status: 'queued', updated_at_ms: now } : company
    ));
  }
  if (stage === 'pipeline') {
    state.companies = state.companies.map((company) => (
      ids.has(company.id) ? { ...company, pipeline_status: 'queued', qualification_status: 'qualified', updated_at_ms: now } : company
    ));
  }
  if (stage === 'contact_research' || stage === 'lead_qualification') {
    state.pipeline = state.pipeline.map((item) => {
      if (!ids.has(item.id)) return item;
      if (stage === 'contact_research') return { ...item, contact_research_status: 'queued', updated_at_ms: now };
      return { ...item, outreach_status: 'queued', updated_at_ms: now };
    });
  }
  persistAutomationQueueMarkers(stage, records, now);
}

function persistAutomationQueueMarkers(stage, records, now) {
  (async () => {
    for (const record of records) {
      if (stage === 'company_research') {
        await upsertDoc(state.ctx.db.raw.outbound_companies, record.id, {
          ...record,
          research_status: 'queued',
          updated_at_ms: now,
        });
      }
      if (stage === 'pipeline') {
        await upsertDoc(state.ctx.db.raw.outbound_companies, record.id, {
          ...record,
          pipeline_status: 'queued',
          qualification_status: 'qualified',
          updated_at_ms: now,
        });
      }
      if (stage === 'contact_research') {
        await upsertDoc(state.ctx.db.raw.outbound_pipeline_items, record.id, {
          ...record,
          contact_research_status: 'queued',
          updated_at_ms: now,
        });
      }
      if (stage === 'lead_qualification') {
        await upsertDoc(state.ctx.db.raw.outbound_pipeline_items, record.id, {
          ...record,
          outreach_status: 'queued',
          updated_at_ms: now,
        });
      }
    }
  })().catch((error) => console.warn('[outbound] local automation queue markers failed', error));
}

function runAutomationBatchInBackground(stage, campaign, records) {
  (async () => {
    const concurrency = stage === 'pipeline' ? 1 : 5;
    for (let batchIndex = 0; batchIndex < records.length; batchIndex += OUTBOUND_BATCH_LIMIT) {
      const batch = records.slice(batchIndex, batchIndex + OUTBOUND_BATCH_LIMIT);
      for (let index = 0; index < batch.length; index += concurrency) {
        const chunk = batch.slice(index, index + concurrency);
        await Promise.all(chunk.map((record) => runAutomationRecord(stage, campaign, record)));
      }
      await loadAll().catch(() => {});
      render();
    }
    await loadAll();
    render();
  })().catch(async (error) => {
    console.warn('[outbound] automation batch failed', error);
    await loadAll().catch(() => {});
    render();
  });
}

async function runAutomationRecord(stage, campaign, record) {
  if (stage === 'company_research') return queueCompanyResearch(record.id, { forceCampaign: campaign });
  if (stage === 'pipeline') return sendCompanyToPipeline(record.id, { forceCampaign: campaign, keepView: true });
  if (stage === 'contact_research') return queueContactResearch(record.id);
  if (stage === 'lead_qualification') return queueLeadQualification(record.id);
  return null;
}

function exportQualificationTable() {
  const campaign = selectedCampaign();
  if (!campaign) return;
  const settings = getCampaignResearchSettings(campaign);
  const visibleFields = researchFieldsForPrompt(settings)
    .filter((field) => field.id !== 'domain');
  const columns = [
    ...companyTableColumns(visibleFields),
    ...contactTableColumns(settings).filter((column) => !column.action),
  ];
  const rows = filteredQualificationRows();
  const worksheetRows = [
    columns.map((column) => column.label),
    ...rows.map((row) => columns.map((column) => tableColumnValue(row, column.id))),
  ];
  const xmlRows = worksheetRows.map((row) => `
    <Row>${row.map((cell) => `<Cell><Data ss:Type="String">${escapeXml(cell || '')}</Data></Cell>`).join('')}</Row>
  `).join('');
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<?mso-application progid="Excel.Sheet"?>
<Workbook xmlns="${OUTBOUND_EXPORT_NS}" xmlns:ss="${OUTBOUND_EXPORT_NS}">
  <Worksheet ss:Name="Outbound">
    <Table>${xmlRows}</Table>
  </Worksheet>
</Workbook>`;
  const blob = new Blob([xml], { type: 'application/vnd.ms-excel;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `${slugifyFileName(campaign.name || 'outbound')}-tabelle.xls`;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 500);
}

function companyResearchValue(company, fieldId) {
  const data = company.company_data || {};
  const payload = company.payload?.imported_row || {};
  const direct = {
    country: company.country,
    city: company.city,
    domain: company.domain || domainFromUrl(company.website),
    email: data.email || payload.email,
    phone: data.phone || payload.phone,
  };
  const aliases = {
    legal_form: ['legal_form', 'rechtsform'],
    postal_code: ['postal_code', 'plz', 'zip'],
    street: ['street', 'strasse', 'address'],
    registry_court: ['registry_court', 'firmenbuchgericht', 'register_court'],
    registry_id: ['registry_id', 'register_id', 'company_register_id'],
    status: ['status'],
    fax: ['fax'],
    vat_id: ['vat_id', 'ust_id', 'ustid'],
    industry_wz: ['industry_wz', 'branche_wz', 'industry'],
    representative_1: ['representative_1', 'ges_vertreter_1', 'management_public'],
    representative_2: ['representative_2', 'ges_vertreter_2'],
    representative_3: ['representative_3', 'ges_vertreter_3'],
    business_purpose: ['business_purpose', 'gegenstand', 'services'],
    tickers: ['tickers', 'ticker'],
    financials_date: ['financials_date', 'finanzkennzahlen_datum'],
    share_capital_eur: ['share_capital_eur', 'stammkapital_eur', 'grundkapital_eur'],
    balance_sheet_total_eur: ['balance_sheet_total_eur', 'bilanzsumme_eur'],
    profit_eur: ['profit_eur', 'gewinn_eur'],
    revenue_eur: ['revenue_eur', 'umsatz_eur'],
    equity_eur: ['equity_eur', 'eigenkapital_eur'],
    employee_count: ['employee_count', 'mitarbeiterzahl', 'company_size'],
  };
  const value = direct[fieldId] || firstObjectValue(data, aliases[fieldId] || [fieldId]) || firstObjectValue(payload, aliases[fieldId] || [fieldId]);
  if (Array.isArray(value)) return value.filter(Boolean).join(', ') || '-';
  return String(value || '').trim() || '-';
}

function firstObjectValue(object, keys) {
  if (!object || typeof object !== 'object') return '';
  for (const key of keys) {
    if (object[key] != null && String(object[key]).trim()) return object[key];
  }
  return '';
}

function pipelineItemForCompany(companyOrId) {
  const company = typeof companyOrId === 'object' ? companyOrId : state.companies.find((item) => item.id === companyOrId);
  const ids = new Set([typeof companyOrId === 'string' ? companyOrId : company?.id, ...(company?.duplicate_company_ids || [])].filter(Boolean));
  return currentPipeline().find((item) => ids.has(item.company_id)) || state.pipeline.find((item) => ids.has(item.company_id));
}

function openResearchSettingsDrawer() {
  const campaign = selectedCampaign();
  const root = state.ctx.host.querySelector('[data-outbound-root]');
  if (!campaign || !root) return;
  closeResearchSettingsDrawer();

  state.researchSettingsActiveTab = 'columns';
  state.tempResearchSettings = JSON.parse(JSON.stringify(getCampaignResearchSettings(campaign)));

  const drawer = document.createElement('aside');
  drawer.className = 'outbound-research-drawer';
  drawer.setAttribute('role', 'dialog');
  drawer.setAttribute('aria-modal', 'true');
  drawer.setAttribute('aria-label', t('settingsFields', 'Research-Felder einstellen'));
  drawer.innerHTML = renderResearchSettingsDrawer(campaign);
  root.append(drawer);

  const initialFocus = drawer.querySelector('[data-research-setting-custom]');
  if (initialFocus) initialFocus.focus();
}

function closeResearchSettingsDrawer() {
  state.tempResearchSettings = null;
  state.researchSettingsActiveTab = 'columns';
  state.ctx?.host?.querySelector('.outbound-research-drawer')?.remove();
}

function renderResearchSettingsDrawer(campaign) {
  const settings = state.tempResearchSettings || getCampaignResearchSettings(campaign);

  let tabBodyHtml = '';
  if (state.researchSettingsActiveTab === 'columns') {
    tabBodyHtml = `
      <div class="outbound-research-toolbar">
        <button class="outbound-button" type="button" data-action="research-settings-core">${escapeHtml(t('kernelFields', 'Kernfelder'))}</button>
        <button class="outbound-button" type="button" data-action="research-settings-all">${escapeHtml(t('allFields', 'Alle Felder'))}</button>
      </div>
      <div class="outbound-column-settings-grid">
        ${renderResearchColumnSection('company', t('columnSectionCompanyTitle', 'Unternehmenshälfte'), t('columnSectionCompanyDesc', 'Stammdaten und Firmenqualifizierung'), RESEARCH_FIELD_DEFS, settings.fields, settings.customFields, settings)}
        ${renderResearchColumnSection('contact', t('columnSectionContactTitle', 'Personenhälfte'), t('columnSectionContactDesc', 'Ansprechpartner und Lead-Qualifizierung'), CONTACT_FIELD_DEFS, settings.contactFields, settings.customContactFields, settings)}
      </div>
      <label class="outbound-research-notes">
        <span>${escapeHtml(t('additionalHints', 'Zusätzliche Hinweise'))}</span>
        <textarea data-research-setting-custom rows="3" placeholder="${escapeHtml(t('additionalHintsPlaceholder', 'z.B. Belege immer mit URL ausgeben, nur öffentlich belegbare Unternehmensdaten, keine Personen recherchieren'))}">${escapeHtml(settings.customInstruction)}</textarea>
      </label>
    `;
  } else if (state.researchSettingsActiveTab === 'prompts') {
    tabBodyHtml = `
      <div class="outbound-prompts-settings-form">
        <div class="outbound-settings-section">
          <h4>${escapeHtml(t('apiConnection', 'API Verbindung'))}</h4>
          <div class="outbound-prompt-fields-row">
            <label class="outbound-field-wrap">
              <span>${escapeHtml(t('gatewayUrl', 'Gateway URL'))}</span>
              <input data-prompt-setting="apiUrl" value="${escapeHtml(settings.apiUrl)}" placeholder="https://..." />
            </label>
            <label class="outbound-field-wrap">
              <span>${escapeHtml(t('tokenId', 'Token ID'))}</span>
              <input data-prompt-setting="apiToken" type="password" value="${escapeHtml(settings.apiToken)}" placeholder="Token..." />
            </label>
          </div>
        </div>

        <div class="outbound-settings-section">
          <h4>${escapeHtml(t('outreachVariables', 'Outreach Variables'))}</h4>
          <div class="outbound-prompt-variables-grid">
            <label class="outbound-field-wrap">
              <span>${escapeHtml(t('icpCoreLabel', 'ICP Core (Produkt- und Dienstleistungsbeschreibung)'))}</span>
              <textarea data-prompt-setting="icpCore" rows="4" placeholder="${escapeHtml(t('icpCorePlaceholder', 'Beschreibe dein Produkt...'))}">${escapeHtml(settings.icpCore)}</textarea>
            </label>
            <label class="outbound-field-wrap">
              <span>${escapeHtml(t('checklistLandingpage', 'Checkliste Landingpage'))}</span>
              <textarea data-prompt-setting="checklist" rows="4" placeholder="${escapeHtml(t('checklistPlaceholder', 'Ein Kriterium pro Zeile...'))}">${escapeHtml(settings.checklist)}</textarea>
            </label>
            <label class="outbound-field-wrap">
              <span>${escapeHtml(t('ctaLabel', 'CTA (Call to Action)'))}</span>
              <input data-prompt-setting="cta" value="${escapeHtml(settings.cta)}" placeholder="${escapeHtml(t('ctaPlaceholder', 'z.B. Sollen wir Ihnen nähere Details zusenden?'))}" />
            </label>
            <label class="outbound-field-wrap">
              <span>${escapeHtml(t('signatureLabel', 'Signatur'))}</span>
              <textarea data-prompt-setting="signature" rows="2" placeholder="${escapeHtml(t('signaturePlaceholder', 'Mit freundlichen Grüßen...'))}">${escapeHtml(settings.signature)}</textarea>
            </label>
          </div>
        </div>

        <div class="outbound-settings-section">
          <h4>${escapeHtml(t('extendedLlmPrompts', 'Erweiterte LLM-Prompts (Templates)'))}</h4>
          <details class="outbound-prompt-details">
            <summary>${escapeHtml(t('editSystemPrompts', 'System-Prompts für Pipeline-Generierung bearbeiten'))}</summary>
            <div class="outbound-details-content">
              <label class="outbound-field-wrap">
                <span>ICP Prompt Template</span>
                <textarea class="code-font" data-prompt-setting="icpPromptTemplate" rows="8">${escapeHtml(settings.icpPromptTemplate)}</textarea>
              </label>
              <label class="outbound-field-wrap">
                <span>Message Prompt Template</span>
                <textarea class="code-font" data-prompt-setting="messagePromptTemplate" rows="8">${escapeHtml(settings.messagePromptTemplate)}</textarea>
              </label>
              <label class="outbound-field-wrap">
                <span>Extract Email Prompt</span>
                <textarea class="code-font" data-prompt-setting="extractEmailPrompt" rows="8">${escapeHtml(settings.extractEmailPrompt)}</textarea>
              </label>
            </div>
          </details>
        </div>
      </div>
    `;
  } else if (state.researchSettingsActiveTab === 'mailserver') {
    const ms = state.mailserver || { domains: [], users: [], loading: false, error: null };
    
    let msContent = '';
    if (ms.loading) {
      msContent = `
        <div class="outbound-mailserver-loading" style="padding: 40px; text-align: center; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px;">
          <div class="outbound-loading-spinner" style="width: 32px; height: 32px; border: 3px solid rgba(255,255,255,0.1); border-radius: 50%; border-top-color: var(--outbound-accent, #3b82f6); animation: spin 1s linear infinite;"></div>
          <p style="color: var(--outbound-muted); font-size: 13px; margin: 0;">${escapeHtml(t('loadingMailserverConfig', 'Mailserver-Konfiguration wird geladen...'))}</p>
        </div>
      `;
    } else {
      if (ms.error) {
        msContent += `
          <div class="outbound-mailserver-error" style="background: rgba(220, 53, 69, 0.1); border: 1px solid #ef4444; padding: 12px; border-radius: 6px; color: #ef4444; margin-bottom: 16px; font-size: 13px;">
            <strong>${escapeHtml(t('errorLabel', 'Fehler'))}:</strong> ${escapeHtml(ms.error)}
          </div>
        `;
      }
      
      // Sektion 1: Custom Domains
      let domainsListHtml = '';
      if (!ms.domains || ms.domains.length === 0) {
        domainsListHtml = `<p class="outbound-muted-placeholder" style="color: var(--outbound-muted); font-style: italic; padding: 8px 0; font-size: 13px;">${escapeHtml(t('noDomainsConfigured', 'Keine Domains konfiguriert.'))}</p>`;
      } else {
        domainsListHtml = ms.domains.map(dom => {
          const publicDkimClean = (dom.dkim_public_key || '')
            .replace('-----BEGIN PUBLIC KEY-----', '')
            .replace('-----END PUBLIC KEY-----', '')
            .replace(/\s+/g, '')
            .trim();
          
          const dkimTxtRecord = `v=DKIM1; k=rsa; p=${publicDkimClean}`;
          
          return `
            <div class="outbound-mailserver-domain-card" style="border: 1px solid var(--outbound-border, #e2e8f0); border-radius: 8px; padding: 16px; margin-bottom: 16px; background: rgba(255,255,255,0.02);">
              <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 8px;">
                <h4 style="margin: 0; font-size: 14px; color: var(--outbound-text, #334155); font-weight: 600;">${escapeHtml(dom.domain_name)}</h4>
                <button class="outbound-button danger-link" type="button" data-action="mailserver-delete-domain" data-domain="${escapeHtml(dom.domain_name)}" style="background: none; border: none; color: #ef4444; cursor: pointer; font-size: 13px; font-weight: 500; padding: 0;">
                  ${escapeHtml(t('delete', 'Löschen'))}
                </button>
              </div>
              
              <details class="outbound-dns-guide" style="cursor: pointer;">
                <summary style="font-size: 13px; color: var(--outbound-accent, #3b82f6); font-weight: 500; outline: none; margin-bottom: 8px;">
                  ${escapeHtml(t('dnsSetupGuide', 'DNS-Einrichtungsanleitung (DKIM, SPF, DMARC)'))}
                </summary>
                <div class="outbound-dns-records" style="padding-top: 10px; display: grid; gap: 12px;">
                  <div style="background: rgba(0,0,0,0.15); padding: 10px; border-radius: 6px;">
                    <div style="display: flex; justify-content: space-between; margin-bottom: 4px; align-items: center;">
                      <strong style="font-size: 11px; color: #94a3b8;">1. DKIM (TXT Record)</strong>
                      <span style="font-size: 10px; color: #64748b; font-family: monospace;">Host: ${escapeHtml(dom.dkim_selector || 'default')}._domainkey</span>
                    </div>
                    <code class="code-font" style="display: block; word-break: break-all; font-size: 11px; background: #000; padding: 6px; border-radius: 4px; color: #10b981; max-height: 80px; overflow-y: auto;">${escapeHtml(dkimTxtRecord)}</code>
                    <button class="outbound-button mini-copy" type="button" data-copy-text="${escapeHtml(dkimTxtRecord)}" style="margin-top: 6px; font-size: 11px; padding: 2px 8px; height: auto; line-height: 1.2;">
                      ${escapeHtml(t('copy', 'Kopieren'))}
                    </button>
                  </div>

                  <div style="background: rgba(0,0,0,0.15); padding: 10px; border-radius: 6px;">
                    <div style="display: flex; justify-content: space-between; margin-bottom: 4px; align-items: center;">
                      <strong style="font-size: 11px; color: #94a3b8;">2. SPF (TXT Record)</strong>
                      <span style="font-size: 10px; color: #64748b; font-family: monospace;">Host: @</span>
                    </div>
                    <code class="code-font" style="display: block; word-break: break-all; font-size: 11px; background: #000; padding: 6px; border-radius: 4px; color: #10b981;">${escapeHtml(dom.spf_record || 'v=spf1 mx a ip4:51.210.246.120 ~all')}</code>
                    <button class="outbound-button mini-copy" type="button" data-copy-text="${escapeHtml(dom.spf_record || 'v=spf1 mx a ip4:51.210.246.120 ~all')}" style="margin-top: 6px; font-size: 11px; padding: 2px 8px; height: auto; line-height: 1.2;">
                      ${escapeHtml(t('copy', 'Kopieren'))}
                    </button>
                  </div>

                  <div style="background: rgba(0,0,0,0.15); padding: 10px; border-radius: 6px;">
                    <div style="display: flex; justify-content: space-between; margin-bottom: 4px; align-items: center;">
                      <strong style="font-size: 11px; color: #94a3b8;">3. DMARC (TXT Record)</strong>
                      <span style="font-size: 10px; color: #64748b; font-family: monospace;">Host: _dmarc</span>
                    </div>
                    <code class="code-font" style="display: block; word-break: break-all; font-size: 11px; background: #000; padding: 6px; border-radius: 4px; color: #10b981;">${escapeHtml(dom.dmarc_record || `v=DMARC1; p=none; rua=mailto:dmarc@${dom.domain_name}`)}</code>
                    <button class="outbound-button mini-copy" type="button" data-copy-text="${escapeHtml(dom.dmarc_record || `v=DMARC1; p=none; rua=mailto:dmarc@${dom.domain_name}`)}" style="margin-top: 6px; font-size: 11px; padding: 2px 8px; height: auto; line-height: 1.2;">
                      ${escapeHtml(t('copy', 'Kopieren'))}
                    </button>
                  </div>
                </div>
              </details>
            </div>
          `;
        }).join('');
      }

      // Sektion 2: E-Mail-Konten
      let usersListHtml = '';
      if (!ms.users || ms.users.length === 0) {
        usersListHtml = `<p class="outbound-muted-placeholder" style="color: var(--outbound-muted); font-style: italic; padding: 8px 0; font-size: 13px;">${escapeHtml(t('noUsersConfigured', 'Keine E-Mail-Konten konfiguriert.'))}</p>`;
      } else {
        usersListHtml = `
          <table class="outbound-mailserver-users-table" style="width: 100%; border-collapse: collapse; margin-top: 8px; font-size: 13px;">
            <thead>
              <tr style="border-bottom: 1px solid var(--outbound-border, #e2e8f0); color: var(--outbound-muted); text-align: left;">
                <th style="padding: 8px 4px; font-weight: 500;">${escapeHtml(t('emailAddress', 'E-Mail-Adresse'))}</th>
                <th style="padding: 8px 4px; font-weight: 500; text-align: right;">${escapeHtml(t('actions', 'Aktionen'))}</th>
              </tr>
            </thead>
            <tbody>
              ${ms.users.map(u => `
                <tr style="border-bottom: 1px solid rgba(255,255,255,0.03);">
                  <td style="padding: 10px 4px; font-weight: 500; color: var(--outbound-text, #334155);">${escapeHtml(u.username)}</td>
                  <td style="padding: 10px 4px; text-align: right;">
                    <button class="outbound-button danger-link" type="button" data-action="mailserver-delete-user" data-username="${escapeHtml(u.username)}" style="background: none; border: none; color: #ef4444; cursor: pointer; font-size: 12px; padding: 0;">
                      ${escapeHtml(t('delete', 'Löschen'))}
                    </button>
                  </td>
                </tr>
              `).join('')}
            </tbody>
          </table>
        `;
      }

      msContent = `
        <div class="outbound-mailserver-settings" style="display: flex; flex-direction: column; gap: 24px;">
          <div class="outbound-settings-section">
            <h4 style="margin-top: 0; margin-bottom: 6px; font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--outbound-muted);">${escapeHtml(t('customDomainsTitle', 'Custom Domains & DNS'))}</h4>
            <p style="font-size: 12px; color: var(--outbound-muted); margin-bottom: 16px; line-height: 1.4;">
              ${escapeHtml(t('customDomainsDesc', 'Füge deine Domains hinzu. CTOX generiert automatisch DKIM-Signaturen und liefert dir die SPF- und DMARC-Einträge für deinen Domain-Registrar.'))}
            </p>
            
            <div class="outbound-mailserver-add-domain-form" style="background: rgba(255,255,255,0.01); border: 1px dashed var(--outbound-border, #e2e8f0); border-radius: 8px; padding: 14px; margin-bottom: 20px;">
              <div style="display: flex; gap: 8px;">
                <input id="mailserver-new-domain-input" type="text" placeholder="z.B. meinefirma.de" style="flex: 1; padding: 6px 12px; border-radius: 6px; border: 1px solid var(--outbound-border, #e2e8f0); font-size: 13px; background: rgba(0,0,0,0.2); color: #fff;" />
                <button class="outbound-button" type="button" data-action="mailserver-save-domain" style="white-space: nowrap; height: 32px; padding: 0 16px; font-size: 13px;">
                  ${escapeHtml(t('addDomainButton', 'Domain hinzufügen'))}
                </button>
              </div>
            </div>
            
            <div class="outbound-mailserver-domains-list">
              ${domainsListHtml}
            </div>
          </div>

          <div class="outbound-settings-section" style="border-top: 1px solid var(--outbound-border, #e2e8f0); padding-top: 24px;">
            <h4 style="margin-top: 0; margin-bottom: 6px; font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--outbound-muted);">${escapeHtml(t('emailAccountsTitle', 'E-Mail-Konten'))}</h4>
            <p style="font-size: 12px; color: var(--outbound-muted); margin-bottom: 16px; line-height: 1.4;">
              ${escapeHtml(t('emailAccountsDesc', 'Erstelle vollwertige Mail-Accounts für den Empfang und Versand über IMAP und SMTP.'))}
            </p>
            
            <div class="outbound-mailserver-add-user-form" style="background: rgba(255,255,255,0.01); border: 1px dashed var(--outbound-border, #e2e8f0); border-radius: 8px; padding: 14px; margin-bottom: 20px; display: grid; gap: 10px;">
              <div style="display: grid; grid-template-columns: 1.2fr 1fr; gap: 8px;">
                <input id="mailserver-new-username-input" type="email" placeholder="name@meinefirma.de" style="padding: 6px 12px; border-radius: 6px; border: 1px solid var(--outbound-border, #e2e8f0); font-size: 13px; background: rgba(0,0,0,0.2); color: #fff;" />
                <input id="mailserver-new-password-input" type="password" placeholder="Passwort" style="padding: 6px 12px; border-radius: 6px; border: 1px solid var(--outbound-border, #e2e8f0); font-size: 13px; background: rgba(0,0,0,0.2); color: #fff;" />
              </div>
              <div style="display: flex; justify-content: flex-end;">
                <button class="outbound-button" type="button" data-action="mailserver-save-user" style="height: 32px; padding: 0 16px; font-size: 13px;">
                  ${escapeHtml(t('addUserButton', 'Konto erstellen'))}
                </button>
              </div>
            </div>
            
            <div class="outbound-mailserver-users-list">
              ${usersListHtml}
            </div>
          </div>
        </div>
      `;
    }
    tabBodyHtml = msContent;
  }

  let footerHtml = `
    <footer class="outbound-research-footer">
      <span class="outbound-muted">${escapeHtml(t('settingsFooterHint', 'Gilt für diese Campaign. Änderungen an Spalten und Prompts werden direkt übernommen.'))}</span>
      <button class="outbound-button" type="button" data-action="close-research-settings">${escapeHtml(t('cancel', 'Abbrechen'))}</button>
      <button class="outbound-button primary" type="button" data-action="save-research-settings">${escapeHtml(t('save', 'Speichern'))}</button>
    </footer>
  `;
  if (state.researchSettingsActiveTab === 'mailserver') {
    footerHtml = `
      <footer class="outbound-research-footer">
        <span class="outbound-muted">${escapeHtml(t('mailserverFooterHint', 'Mailserver-Einstellungen gelten global für das gesamte System.'))}</span>
        <button class="outbound-button" type="button" data-action="close-research-settings">${escapeHtml(t('close', 'Schließen'))}</button>
      </footer>
    `;
  }

  return `
    <div class="outbound-research-backdrop" data-action="close-research-settings"></div>
    <section class="outbound-research-panel">
      <header class="outbound-research-header">
        <div>
          <span>${escapeHtml(t('funnelSettings', 'Funnel Einstellungen'))}</span>
          <h3>${escapeHtml(t('configureCampaign', 'Campaign konfigurieren'))}</h3>
        </div>
        <button class="outbound-icon-button" type="button" data-action="close-research-settings" aria-label="${escapeHtml(t('close', 'Schließen'))}">×</button>
      </header>
      <div class="outbound-drawer-tabs">
        <button class="outbound-drawer-tab" type="button" data-action="research-settings-tab" data-tab="columns" ${state.researchSettingsActiveTab === 'columns' ? 'aria-selected="true"' : ''}>${escapeHtml(t('columnsAndFieldsTab', 'Spalten & Felder'))}</button>
        <button class="outbound-drawer-tab" type="button" data-action="research-settings-tab" data-tab="prompts" ${state.researchSettingsActiveTab === 'prompts' ? 'aria-selected="true"' : ''}>${escapeHtml(t('outreachAndApiTab', 'Outreach & API Prompts'))}</button>
        <button class="outbound-drawer-tab" type="button" data-action="research-settings-tab" data-tab="mailserver" ${state.researchSettingsActiveTab === 'mailserver' ? 'aria-selected="true"' : ''}>${escapeHtml(t('mailserverTab', 'Mailserver & Domains'))}</button>
      </div>
      <div class="outbound-research-body">
        ${tabBodyHtml}
      </div>
      ${footerHtml}
    </section>
  `;
}

function renderResearchColumnSection(side, title, subtitle, baseDefs, selectedIds, customFields, settings) {
  const selectedFull = new Set(selectedIds || []);
  const selectedCompactIds = side === 'contact' ? settings.contactFieldsCompact : settings.fieldsCompact;
  const selectedCompact = new Set(selectedCompactIds || []);
  const rows = [
    ...baseDefs.map(([id, fallbackLabel, fallbackPrompt]) => ({
      id,
      label: settings.columnLabels?.[id] || fallbackLabel,
      prompt: settings.fieldPrompts?.[id] || fallbackPrompt || '',
      custom: false,
    })),
    ...(customFields || []).map((field) => ({
      ...field,
      label: settings.columnLabels?.[field.id] || field.label,
      prompt: settings.fieldPrompts?.[field.id] || field.prompt || '',
      custom: true,
    })),
  ];
  const addAction = side === 'contact' ? 'research-settings-add-contact-field' : 'research-settings-add-field';
  return `
    <section class="outbound-column-settings" data-column-settings-side="${escapeHtml(side)}" aria-label="${escapeHtml(title)}">
      <div class="outbound-custom-research-head">
        <div>
          <span>${escapeHtml(title)}</span>
          <strong>${escapeHtml(subtitle)}</strong>
        </div>
      </div>
      <div class="outbound-column-settings-labels" aria-hidden="true" style="grid-template-columns: 56px minmax(126px, 0.72fr) minmax(160px, 1.28fr) 26px;">
        <div style="display: flex; gap: 8px; justify-content: center; font-size: 8px; font-weight: 800; color: var(--outbound-muted); letter-spacing: 0.5px;">
          <span>${escapeHtml(t('viewFull', 'VOLL'))}</span>
          <span>${escapeHtml(t('viewCompact', 'KOMP'))}</span>
        </div>
        <span>${escapeHtml(t('columnLabelAria', 'Spaltenname'))}</span>
        <span>${escapeHtml(t('columnPrompt', 'CTOX Research-Anweisung'))}</span>
        <span></span>
      </div>
      <div class="outbound-column-settings-list" data-custom-field-list="${escapeHtml(side)}">
        ${rows.map((field) => renderResearchColumnRow(side, field, selectedFull.has(field.id), selectedCompact.has(field.id))).join('')}
      </div>
      <div class="outbound-custom-field-form">
        <input data-custom-field-label="${escapeHtml(side)}" placeholder="${side === 'contact' ? escapeHtml(t('customContactPlaceholder', 'z.B. Buying Committee, Entscheider-Relevanz')) : escapeHtml(t('customCompanyPlaceholder', 'z.B. Zertifizierungen, Zielkunden, Technologien'))}" aria-label="${escapeHtml(t('newColumnAria', 'Neue Spalte'))}" />
        <button class="outbound-button" type="button" data-action="${addAction}">${escapeHtml(t('addColumn', 'Hinzufügen'))}</button>
      </div>
    </section>
  `;
}

function renderResearchColumnRow(side, field, checkedFull = true, checkedCompact = false) {
  const removeAction = side === 'contact' ? 'research-settings-remove-contact-field' : 'research-settings-remove-field';
  return `
    <div class="outbound-column-setting-row" data-column-setting-id="${escapeHtml(field.id)}" data-column-setting-side="${escapeHtml(side)}" ${field.custom ? `data-custom-field-id="${escapeHtml(field.id)}"` : ''} style="grid-template-columns: 56px minmax(126px, 0.72fr) minmax(160px, 1.28fr) 26px;">
      <div style="display: flex; gap: 8px; align-items: center; justify-content: center;">
        <label class="outbound-column-setting-toggle" title="${escapeHtml(t('viewFullTitle', 'Vollansicht'))}" style="margin: 0; display: inline-flex;">
          <input type="checkbox" data-column-setting-full="${escapeHtml(field.id)}" data-column-setting-kind="${escapeHtml(side)}" ${checkedFull ? 'checked' : ''} style="cursor: pointer; accent-color: var(--outbound-accent);" />
        </label>
        <label class="outbound-column-setting-toggle" title="${escapeHtml(t('viewCompactTitle', 'Kompaktansicht'))}" style="margin: 0; display: inline-flex;">
          <input type="checkbox" data-column-setting-compact="${escapeHtml(field.id)}" data-column-setting-kind="${escapeHtml(side)}" ${checkedCompact ? 'checked' : ''} style="cursor: pointer; accent-color: var(--outbound-accent);" />
        </label>
      </div>
      <input data-column-setting-label value="${escapeHtml(field.label)}" aria-label="${escapeHtml(t('columnLabelAria', 'Spaltenname'))}" />
      <input data-column-setting-prompt value="${escapeHtml(field.prompt || '')}" placeholder="${escapeHtml(t('columnPromptPlaceholder', 'Was soll CTOX dafür recherchieren?'))}" aria-label="${escapeHtml(t('columnPromptAria', 'Research-Anweisung'))}" />
      ${field.custom ? `<button type="button" data-action="${removeAction}" aria-label="${escapeHtml(t('deleteColumnAria', '{0} löschen', field.label))}">×</button>` : '<i></i>'}
    </div>
  `;
}

async function saveResearchSettings() {
  const campaign = selectedCampaign();
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  if (!campaign || !drawer) return;

  const beforeSettings = getCampaignResearchSettings(campaign);

  saveResearchSettingsTemporaryState(drawer);

  const researchSettings = normalizeResearchSettings(state.tempResearchSettings);

  const nextPayload = {
    ...(campaign.payload || {}),
    research_settings: researchSettings,
  };

  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaign.id, {
    payload: nextPayload,
    updated_at_ms: Date.now(),
  });

  state.campaigns = state.campaigns.map((item) => (
    item.id === campaign.id
      ? { ...item, payload: nextPayload, updated_at_ms: Date.now() }
      : item
  ));

  state.tempResearchSettings = null;
  state.researchSettingsActiveTab = 'columns';

  closeResearchSettingsDrawer();
  render();

  window.setTimeout(() => {
    loadAll()
      .then(() => queueResearchForNewFields(campaign.id, beforeSettings, researchSettings))
      .then(loadAll)
      .then(render)
      .catch((error) => console.warn('[outbound] research settings follow-up queue failed', error));
  }, 0);
}

function setResearchSettingsSelection(checked) {
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  drawer?.querySelectorAll('input[type="checkbox"][data-column-setting-kind]').forEach((input) => {
    input.checked = checked;
  });
}

function setResearchSettingsCoreSelection() {
  const core = new Set([
    'legal_form',
    'country',
    'postal_code',
    'city',
    'street',
    'registry_id',
    'phone',
    'email',
    'domain',
    'vat_id',
    'industry_wz',
    'representative_1',
    'business_purpose',
    'revenue_eur',
    'employee_count',
  ]);
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  drawer?.querySelectorAll('input[type="checkbox"][data-column-setting-kind]').forEach((input) => {
    const isFull = input.hasAttribute('data-column-setting-full');
    const fieldId = isFull ? input.dataset.columnSettingFull : input.dataset.columnSettingCompact;
    const kind = input.dataset.columnSettingKind;
    if (isFull) {
      if (kind === 'contact') {
        input.checked = DEFAULT_CONTACT_FIELD_IDS.includes(fieldId);
      } else {
        input.checked = core.has(fieldId);
      }
    } else {
      if (kind === 'contact') {
        input.checked = DEFAULT_CONTACT_FIELD_IDS_COMPACT.includes(fieldId);
      } else {
        input.checked = DEFAULT_RESEARCH_FIELD_IDS_COMPACT.includes(fieldId);
      }
    }
  });
}

function addResearchSettingsField(side = 'company') {
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  const input = drawer?.querySelector(`[data-custom-field-label="${cssEscape(side)}"]`);
  const list = drawer?.querySelector(`[data-custom-field-list="${cssEscape(side)}"]`);
  if (!drawer || !input || !list) return;
  const label = input.value.trim();
  if (!label) return;
  const id = side === 'contact' ? customContactFieldId(label) : customResearchFieldId(label);
  if (drawer.querySelector(`[data-column-setting-id="${cssEscape(id)}"]`)) {
    showBusinessAlert('Diese Spalte gibt es bereits.');
    return;
  }
  list.insertAdjacentHTML('beforeend', renderResearchColumnRow(side, { id, label, prompt: '', custom: true }, true));
  input.value = '';
  drawer.querySelector(`[data-column-setting-id="${cssEscape(id)}"] [data-column-setting-label]`)?.focus();
}

function removeResearchSettingsField(fieldId, side = 'company') {
  if (!fieldId) return;
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  const row = drawer?.querySelector(`[data-column-setting-side="${cssEscape(side)}"][data-custom-field-id="${cssEscape(fieldId)}"]`);
  row?.remove();
}

function getCampaignResearchSettings(campaign) {
  return normalizeResearchSettings(campaign?.payload?.research_settings || {});
}

function normalizeResearchSettings(raw = {}) {
  const customFields = normalizeCustomResearchFields(raw.customFields || raw.custom_fields || []);
  const customContactFields = normalizeCustomContactFields(raw.customContactFields || raw.custom_contact_fields || []);
  const columnLabels = normalizeStringMap(raw.columnLabels || raw.column_labels || {});
  const fieldPrompts = normalizeStringMap(raw.fieldPrompts || raw.field_prompts || {});
  const known = new Set([...RESEARCH_FIELD_DEFS.map(([id]) => id), ...customFields.map((field) => field.id)]);
  const fields = Array.isArray(raw.fields)
    ? raw.fields.filter((id) => known.has(id))
    : [...DEFAULT_RESEARCH_FIELD_IDS, ...customFields.map((field) => field.id)];
  
  const fieldsCompact = Array.isArray(raw.fieldsCompact || raw.fields_compact)
    ? (raw.fieldsCompact || raw.fields_compact).filter((id) => known.has(id))
    : [...DEFAULT_RESEARCH_FIELD_IDS_COMPACT, ...customFields.map((field) => field.id)];

  const knownContact = new Set([...CONTACT_FIELD_DEFS.map(([id]) => id), ...customContactFields.map((field) => field.id)]);
  const contactFields = Array.isArray(raw.contactFields || raw.contact_fields)
    ? (raw.contactFields || raw.contact_fields).filter((id) => knownContact.has(id))
    : [...DEFAULT_CONTACT_FIELD_IDS, ...customContactFields.map((field) => field.id)];

  const contactFieldsCompact = Array.isArray(raw.contactFieldsCompact || raw.contact_fields_compact)
    ? (raw.contactFieldsCompact || raw.contact_fields_compact).filter((id) => knownContact.has(id))
    : [...DEFAULT_CONTACT_FIELD_IDS_COMPACT, ...customContactFields.map((field) => field.id)];

  return {
    fields: fields.length ? fields : DEFAULT_RESEARCH_FIELD_IDS,
    fieldsCompact: fieldsCompact.length ? fieldsCompact : DEFAULT_RESEARCH_FIELD_IDS_COMPACT,
    contactFields: contactFields.length ? contactFields : DEFAULT_CONTACT_FIELD_IDS,
    contactFieldsCompact: contactFieldsCompact.length ? contactFieldsCompact : DEFAULT_CONTACT_FIELD_IDS_COMPACT,
    customFields,
    customContactFields,
    columnLabels,
    fieldPrompts,
    customInstruction: String(raw.customInstruction || raw.custom_instruction || '').trim(),
    apiUrl: String(raw.apiUrl || raw.api_url || DEFAULT_API_URL).trim(),
    apiToken: String(raw.apiToken || raw.api_token || DEFAULT_TOKEN_ID).trim(),
    icpCore: String(raw.icpCore || raw.icp_core || DEFAULT_ICP_CORE).trim(),
    checklist: String(raw.checklist || raw.checklist_landingpage || DEFAULT_CHECKLIST).trim(),
    cta: String(raw.cta || DEFAULT_CTA).trim(),
    signature: String(raw.signature || DEFAULT_SIGNATURE).trim(),
    icpPromptTemplate: String(raw.icpPromptTemplate || raw.icp_prompt_template || ICP_PROMPT_TEMPLATE).trim(),
    messagePromptTemplate: String(raw.messagePromptTemplate || raw.message_prompt_template || MESSAGE_PROMPT_TEMPLATE).trim(),
    extractEmailPrompt: String(raw.extractEmailPrompt || raw.extract_email_prompt || EXTRACT_EMAIL_PROMPT).trim(),
  };
}

function researchFieldsForPrompt(settings) {
  const selected = new Set(settings.fields);
  const customFields = settings.customFields || [];
  return [
    ...RESEARCH_FIELD_DEFS.map(([id, label]) => ({ id, label: settings.columnLabels?.[id] || label, prompt: settings.fieldPrompts?.[id] || '' })),
    ...customFields.map((field) => ({ id: field.id, label: settings.columnLabels?.[field.id] || field.label, prompt: settings.fieldPrompts?.[field.id] || field.prompt || '', custom: true })),
  ]
    .filter((field) => selected.has(field.id));
}

function contactFieldsForPrompt(settings) {
  const selected = new Set(settings.contactFields || DEFAULT_CONTACT_FIELD_IDS);
  const customFields = settings.customContactFields || [];
  return [
    ...CONTACT_FIELD_DEFS.map(([id, label, prompt]) => ({ id, label: settings.columnLabels?.[id] || label, prompt: settings.fieldPrompts?.[id] || prompt || '' })),
    ...customFields.map((field) => ({ id: field.id, label: settings.columnLabels?.[field.id] || field.label, prompt: settings.fieldPrompts?.[field.id] || field.prompt || '', custom: true })),
  ].filter((field) => selected.has(field.id));
}

function normalizeCustomResearchFields(fields) {
  if (!Array.isArray(fields)) return [];
  const seen = new Set(RESEARCH_FIELD_DEFS.map(([id]) => id));
  const normalized = [];
  for (const field of fields) {
    const label = String(field?.label || field?.name || '').replace(/\s+/g, ' ').trim();
    if (!label) continue;
    const id = String(field?.id || customResearchFieldId(label)).trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    normalized.push({ id, label, prompt: String(field?.prompt || field?.instruction || '').trim() });
  }
  return normalized;
}

function normalizeCustomContactFields(fields) {
  if (!Array.isArray(fields)) return [];
  const seen = new Set(CONTACT_FIELD_DEFS.map(([id]) => id));
  const normalized = [];
  for (const field of fields) {
    const label = String(field?.label || field?.name || '').replace(/\s+/g, ' ').trim();
    if (!label) continue;
    const id = String(field?.id || customContactFieldId(label)).trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    normalized.push({ id, label, prompt: String(field?.prompt || field?.instruction || '').trim() });
  }
  return normalized;
}

function normalizeStringMap(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  return Object.fromEntries(Object.entries(value)
    .map(([key, item]) => [String(key), String(item || '').trim()])
    .filter(([key, item]) => key && item));
}

function customResearchFieldId(label) {
  const slug = slugifyFileName(label).replaceAll('-', '_');
  return `custom_${slug || fingerprint(label).slice(0, 8)}`;
}

function customContactFieldId(label) {
  const slug = slugifyFileName(label).replaceAll('-', '_');
  return `contact_custom_${slug || fingerprint(label).slice(0, 8)}`;
}

async function queueResearchForNewFields(campaignId, beforeSettings, nextSettings) {
  const before = new Set(beforeSettings.fields || []);
  const beforeContact = new Set(beforeSettings.contactFields || []);
  const addedCustomFields = (nextSettings.customFields || [])
    .filter((field) => nextSettings.fields.includes(field.id) && !before.has(field.id));
  const addedCustomContactFields = (nextSettings.customContactFields || [])
    .filter((field) => nextSettings.contactFields.includes(field.id) && !beforeContact.has(field.id));
  if (!addedCustomFields.length && !addedCustomContactFields.length) return;
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign) return;
  const companies = state.companies
    .filter((item) => item.campaign_id === campaignId && !isQueuedStatus(item.research_status))
    .slice(0, OUTBOUND_BATCH_LIMIT);
  const pipelineItems = state.pipeline
    .filter((item) => item.campaign_id === campaignId && !isQueuedStatus(item.contact_research_status))
    .slice(0, OUTBOUND_BATCH_LIMIT);
  for (const company of addedCustomFields.length ? companies : []) {
    await queueCompanyResearch(company.id, {
      forceCampaign: campaign,
      forceSettings: nextSettings,
      fieldsOverride: addedCustomFields,
      reason: 'custom_fields_added',
    });
  }
  for (const item of addedCustomContactFields.length ? pipelineItems : []) {
    await queueContactResearch(item.id);
  }
}

function visibleCampaigns() {
  const defaultCandidates = state.campaigns.filter((campaign) => campaign.name === DEFAULT_CAMPAIGN_NAME);
  const emptyDefaultIds = new Set(defaultCandidates.filter((campaign) => !directCampaignHasData(campaign.id)).map((campaign) => campaign.id));
  const preferredDefault = defaultCandidates.find((campaign) => campaign.id === DEFAULT_CAMPAIGN_ID)
    || defaultCandidates.find((campaign) => !emptyDefaultIds.has(campaign.id))
    || defaultCandidates[0];
  return state.campaigns.filter((campaign) => {
    if (campaign.name !== DEFAULT_CAMPAIGN_NAME) return true;
    if (!emptyDefaultIds.has(campaign.id)) return true;
    return campaign.id === preferredDefault?.id;
  });
}

function directCampaignHasData(campaignId) {
  return state.sources.some((item) => item.campaign_id === campaignId)
    || state.companies.some((item) => item.campaign_id === campaignId)
    || state.pipeline.some((item) => item.campaign_id === campaignId);
}

function campaignHasData(campaignId) {
  const scoped = campaignScopedRows(state, campaignId);
  return scoped.sources.length > 0 || scoped.companies.length > 0 || scoped.pipeline.length > 0;
}

function campaignFunnelMetrics(campaignId) {
  const scoped = campaignScopedRows(state, campaignId);
  const sources = scoped.sources;
  const rawCompanies = scoped.companies;
  const companies = dedupeCompanies(rawCompanies);
  const pipeline = dedupePipelineItems(scoped.pipeline);
  const companySourceIds = new Set(rawCompanies.map((item) => item.source_id).filter(Boolean));
  const parsedSourceIds = new Set();
  const parsedSourceCount = sources.filter((source) => {
    const status = String(source.status || '').toLowerCase();
    const commandStatus = sourceImportCommandStatus(source);
    const parsed = ['imported', 'parsed', 'completed', 'done'].includes(status)
      || ['completed', 'done', 'imported', 'parsed'].includes(commandStatus)
      || companySourceIds.has(source.id);
    if (parsed) parsedSourceIds.add(source.id);
    return parsed;
  }).length;
  const runningSourceCount = sources.filter((source) => !parsedSourceIds.has(source.id) && isImportSourceRunning(source)).length;
  const failedSourceCount = sources.filter((source) => !parsedSourceIds.has(source.id) && isImportSourceFailed(source)).length;
  const companyResearchDone = companies.filter(isCompanyResearchDone).length;
  const companyQualified = companies.filter((item) => item.qualification_status === 'qualified').length;
  const contactQualified = pipeline.filter(isContactQualified).length;
  const leadQualified = pipeline.filter(isLeadQualified).length;
  return {
    input: companies.length,
    sourceCount: sources.length,
    parsedSourceCount,
    runningSourceCount,
    failedSourceCount,
    companyResearchDone,
    companyResearchTotal: companies.length,
    companyResearchOpen: automationOpenRecords(campaignId, 'company_research').length,
    companyQualified,
    companyQualifiedTotal: Math.max(companyResearchDone, companyQualified),
    pipelineOpen: automationOpenRecords(campaignId, 'pipeline').length,
    contactQualified,
    contactQualifiedTotal: Math.max(companyQualified, contactQualified),
    contactOpen: automationOpenRecords(campaignId, 'contact_research').length,
    leadQualified,
    leadQualifiedTotal: Math.max(contactQualified, leadQualified),
    leadOpen: automationOpenRecords(campaignId, 'lead_qualification').length,
  };
}

function isImportSourceRunning(source) {
  const status = String(source?.status || '').toLowerCase();
  if (['queued_parser', 'queued', 'accepted', 'running', 'processing', 'working'].includes(status)) return true;
  const commandStatus = sourceImportCommandStatus(source);
  return ['accepted', 'queued', 'running', 'working', 'leased'].includes(commandStatus);
}

function isImportSourceFailed(source) {
  const status = String(source?.status || '').toLowerCase();
  if (['failed', 'failed_parser', 'error'].includes(status)) return true;
  return sourceImportCommandStatus(source) === 'failed';
}

function sourceImportCommandStatus(source) {
  const commandId = source?.payload?.record_id || source?.payload?.command_id || source?.command_id || '';
  if (!commandId) return '';
  const command = state.commands.find((item) => item.command_id === commandId || item.id === commandId);
  return commandStatusForCommand(command);
}

function isCompanyResearchDone(company) {
  const status = String(company?.research_status || '').toLowerCase();
  return ['researched', 'completed', 'done'].includes(status)
    || ['qualified', 'rejected'].includes(String(company?.qualification_status || '').toLowerCase());
}

function isQueuedStatus(value) {
  return ['queued', 'accepted', 'pending', 'scheduled', 'running', 'in_progress', 'processing', 'working', 'leased'].includes(String(value || '').toLowerCase());
}

function isDoneLikeStatus(value) {
  return ['researched', 'qualified', 'completed', 'done', 'handled', 'lead_qualified'].includes(String(value || '').toLowerCase());
}

function isAutomationActive(value) {
  const kind = automationStatusKind(value);
  return kind === 'running' || kind === 'waiting';
}

function isFailedStatus(value) {
  const kind = automationStatusKind(value);
  return kind === 'failed' || kind === 'blocked';
}

function isCancelledStatus(value) {
  return automationStatusKind(value) === 'cancelled';
}

function automationStatusKind(value) {
  const status = String(value || '').toLowerCase();
  if (['running', 'in_progress', 'processing', 'working', 'leased'].includes(status)) return 'running';
  if (['queued', 'accepted', 'pending', 'scheduled'].includes(status)) return 'waiting';
  if (['failed', 'error', 'pending_sync_failed'].includes(status)) return 'failed';
  if (['blocked', 'stale_missing_native'].includes(status)) return 'blocked';
  if (['cancelled', 'canceled'].includes(status)) return 'cancelled';
  if (['completed', 'done', 'handled', 'researched', 'qualified'].includes(status)) return 'done';
  return 'idle';
}

function automationStatusLabel(value) {
  const kind = automationStatusKind(value);
  if (kind === 'running') return 'läuft gerade';
  if (kind === 'waiting') return 'wartet in CTOX';
  if (kind === 'failed') return 'Fehler';
  if (kind === 'blocked') return 'blockiert';
  if (kind === 'cancelled') return 'abgebrochen';
  if (kind === 'done') return 'fertig';
  return 'nicht gestartet';
}

function badgeClassForStatus(status, isDone) {
  if (isDone) return 'good';
  if (isAutomationActive(status)) return 'is-running';
  if (isFailedStatus(status) || isCancelledStatus(status)) return 'warn';
  return '';
}

function renderInlineResearchStatus(label, status) {
  const active = isAutomationActive(status);
  const failed = isFailedStatus(status) || isCancelledStatus(status);
  const className = ['outbound-inline-status', active ? 'is-running' : '', failed ? 'warn' : ''].filter(Boolean).join(' ');
  const statusLabel = active || failed ? automationStatusLabel(status) : label;
  return `<small class="${className}">${active ? '<i></i>' : ''}${escapeHtml(statusLabel || 'offen')}</small>`;
}

function companyResearchStatus(company) {
  const run = latestAutomationRun('company_research', companyRecordIds(company));
  return commandStatusForRun(run) || run?.status || company?.research_status || '';
}

function pipelineResearchStatus(item, stage) {
  if (!item) return '';
  const run = latestAutomationRun(stage, new Set([item.id, item.pipeline_id, item.company_id].filter(Boolean)));
  if (stage === 'contact_research') return commandStatusForRun(run) || run?.status || item.contact_research_status || '';
  if (stage === 'lead_qualification') return commandStatusForRun(run) || run?.status || item.outreach_status || '';
  return commandStatusForRun(run) || run?.status || '';
}

function latestAutomationRun(runType, recordIds) {
  const ids = recordIds instanceof Set ? recordIds : new Set(recordIds || []);
  if (!ids.size) return null;
  return state.runs
    .filter((run) => run.run_type === runType && [run.company_id, run.pipeline_id, run.record_id, run.id].some((id) => ids.has(id)))
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0] || null;
}

function commandStatusForRun(run) {
  if (!run?.command_id) return '';
  const command = state.commands.find((item) => item.command_id === run.command_id || item.id === run.command_id);
  return commandStatusForCommand(command || run);
}

function commandStatusForCommand(command) {
  const task = queueTaskForCommand(command);
  return String(task?.status || task?.route_status || command?.task_status || command?.status || '').toLowerCase();
}

function queueTaskForCommand(commandOrRun) {
  if (!commandOrRun) return null;
  const commandId = commandOrRun.command_id || commandOrRun.id || '';
  const taskId = commandOrRun.task_id || '';
  return state.queueTasks
    .filter((task) => (
      (commandId && (task.command_id === commandId || task.client_command_id === commandId))
      || (taskId && task.id === taskId)
    ))
    .sort((a, b) => Number(b.updated_at_ms || b.updated_at || 0) - Number(a.updated_at_ms || a.updated_at || 0))[0] || null;
}

function companyRecordIds(company) {
  return new Set([company?.id, ...(company?.duplicate_company_ids || []), ...(company?.payload?.duplicate_company_ids || [])].filter(Boolean));
}

function isContactQualified(item) {
  if (!item) return false;
  const status = String(item.contact_research_status || '').toLowerCase();
  const stage = String(item.stage || '').toLowerCase();
  return (Array.isArray(item.contacts) && item.contacts.length > 0)
    || ['qualified', 'researched', 'completed', 'done'].includes(status)
    || ['contact_qualified', 'lead_qualified', 'outreach', 'conversation'].includes(stage);
}

function isLeadQualified(item) {
  if (!item) return false;
  const status = String(item.outreach_status || '').toLowerCase();
  const stage = String(item.stage || '').toLowerCase();
  return ['lead_qualified', 'qualified', 'ready', 'completed'].includes(status)
    || ['lead_qualified', 'lead-ready', 'lead_ready', 'conversation'].includes(stage);
}

function conversionRate(from, to) {
  const denominator = Number(from || 0);
  if (denominator <= 0) return '0%';
  return `${Math.round((Number(to || 0) / denominator) * 100)}%`;
}

function renderCompanyWorkbench() {
  const companies = filteredCompanies();
  return `
    <div class="outbound-workbench">
      <div class="outbound-filter">
        ${[
          ['all', 'Alle'],
          ['research', 'Research offen'],
          ['qualified', 'Qualifiziert'],
          ['pipeline', 'Pipeline'],
          ['rejected', 'Nicht passend'],
        ].map(([id, label]) => `<button type="button" data-filter="${id}" aria-pressed="${state.filter === id}">${label}</button>`).join('')}
        <input class="outbound-search" data-search placeholder="Firma suchen" value="${escapeHtml(state.search)}" />
      </div>
      <div class="outbound-scroll">
        <div class="outbound-list">
          ${companies.map(renderCompanyRow).join('') || '<div class="outbound-empty">Keine Firmen für diesen Filter.</div>'}
        </div>
      </div>
    </div>
  `;
}

function renderCompanyRow(company) {
  return `
    <div class="outbound-company-row" data-action="select-company" data-id="${escapeHtml(company.id)}" aria-current="${company.id === state.selectedCompanyId}">
      <div>
        <div class="outbound-title">${escapeHtml(company.name)}</div>
        <div class="outbound-muted">${escapeHtml(company.domain || company.website || 'Website offen')}</div>
      </div>
      <div>
        <span class="outbound-badge ${company.research_status === 'researched' ? 'good' : 'warn'}">${labelResearch(company.research_status)}</span>
      </div>
      <div>
        <span class="outbound-badge ${company.qualification_status === 'qualified' ? 'good' : company.qualification_status === 'rejected' ? 'warn' : ''}">${labelQualification(company.qualification_status)}</span>
      </div>
      <div class="outbound-row-actions">
        <button class="outbound-button" type="button" data-action="research-company" data-id="${escapeHtml(company.id)}">Research</button>
        <button class="outbound-button" type="button" data-action="qualify-company" data-id="${escapeHtml(company.id)}">Qualifizieren</button>
        <button class="outbound-button" type="button" data-action="send-pipeline" data-id="${escapeHtml(company.id)}">Pipeline</button>
      </div>
    </div>
  `;
}

function renderPipelineWorkbench() {
  const items = currentPipeline();
  return `
    <div class="outbound-workbench">
      <div class="outbound-filter">
        <span class="outbound-muted">Nur qualifizierte Unternehmen gehen hier in die Ansprechpartner-Stufe.</span>
      </div>
      <div class="outbound-scroll">
        <div class="outbound-list">
          ${items.map(renderPipelineItem).join('') || '<div class="outbound-empty">Noch keine Firmen in der Pipeline.</div>'}
        </div>
      </div>
    </div>
  `;
}

function renderPipelineItem(item) {
  return `
    <div class="outbound-pipeline-item" data-action="select-pipeline" data-id="${escapeHtml(item.id)}" aria-current="${item.id === state.selectedPipelineId}">
      <div class="outbound-item-top">
        <div>
          <div class="outbound-title">${escapeHtml(item.company_name)}</div>
          <div class="outbound-muted">${escapeHtml(item.stage)} · ${escapeHtml(item.contact_research_status)}</div>
        </div>
        <button class="outbound-button" type="button" data-action="research-contacts" data-id="${escapeHtml(item.id)}">Ansprechpartner</button>
      </div>
      <div class="outbound-badges">
        <span class="outbound-badge">${escapeHtml(item.priority || 'normal')}</span>
        <span class="outbound-badge">${escapeHtml(item.outreach_status || 'not_started')}</span>
      </div>
    </div>
  `;
}

function renderRight() {
  const root = state.ctx.host.querySelector('.outbound-right');
  if (!root) return;
  if (state.activeView === 'pipeline') {
    root.innerHTML = renderPipelineDetail();
    return;
  }
  root.innerHTML = renderCompanyDetail();
}

function renderCompanyDetail() {
  const company = selectedCompany();
  if (!company) return '<div class="outbound-empty">Firma auswählen.</div>';
  const runs = state.runs.filter((run) => run.company_id === company.id);
  const source = state.sources.find((item) => item.id === company.source_id);
  const item = pipelineItemForCompany(company);
  const diagnostics = companyDiagnostics(company, source, item, runs);
  return `
    <header class="outbound-pane-header">
      <div><span>Company Research</span><h2>${escapeHtml(company.name)}</h2></div>
    </header>
    <div class="outbound-detail">
      <div class="outbound-detail-block">
        <div class="outbound-field-list">
          ${field('Website', company.website || company.company_data?.website || 'offen')}
          ${field('Ort', [company.city, company.country].filter(Boolean).join(', ') || 'offen')}
          ${field('Fit', `${labelQualification(company.qualification_status)} · ${company.fit_score || 0}/100`)}
          ${field('Pipeline', labelPipeline(company.pipeline_status))}
          ${field('Importjob', source ? `${source.title || source.id} · ${source.status || 'offen'}` : 'keine Quelle verknüpft')}
        </div>
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-row-actions">
          <button class="outbound-button primary" type="button" data-action="research-company" data-id="${escapeHtml(company.id)}">Unternehmen recherchieren</button>
          <button class="outbound-button" type="button" data-action="qualify-company" data-id="${escapeHtml(company.id)}">Qualifizieren</button>
          <button class="outbound-button" type="button" data-action="reject-company" data-id="${escapeHtml(company.id)}">Ablehnen</button>
          <button class="outbound-button" type="button" data-action="send-pipeline" data-id="${escapeHtml(company.id)}">In Pipeline</button>
        </div>
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Evidence</div>
        ${renderEvidence(company)}
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Research Runs</div>
        ${runs.map((run) => `<div class="outbound-muted">${escapeHtml(run.run_type)} · ${escapeHtml(run.status)} · ${new Date(run.updated_at_ms).toLocaleString()}</div>`).join('') || '<div class="outbound-muted">Noch keine Research Runs.</div>'}
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Diagnostik</div>
        ${renderDiagnosticsList(diagnostics)}
      </div>
    </div>
  `;
}

function renderPipelineDetail() {
  const item = selectedPipelineItem();
  if (!item) return '<div class="outbound-empty">Pipeline-Eintrag auswählen.</div>';
  const company = state.companies.find((entry) => entry.id === item.company_id);
  const runs = state.runs.filter((run) => run.pipeline_id === item.id || run.company_id === item.company_id);
  return `
    <header class="outbound-pane-header">
      <div><span>Pipeline</span><h2>${escapeHtml(item.company_name)}</h2></div>
    </header>
    <div class="outbound-detail">
      <div class="outbound-detail-block">
        ${field('Stage', item.stage)}
        ${field('Contact Research', item.contact_research_status)}
        ${field('Outreach', item.outreach_status)}
      </div>
      <div class="outbound-detail-block">
        <button class="outbound-button primary" type="button" data-action="research-contacts" data-id="${escapeHtml(item.id)}">Ansprechpartner recherchieren</button>
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Kontakte</div>
        ${(item.contacts || []).map((contact) => `<div class="outbound-muted">${escapeHtml(contact.name || 'Kontakt')} · ${escapeHtml(contact.role || '')}</div>`).join('') || '<div class="outbound-muted">Kontakte werden erst in dieser Pipeline-Stufe recherchiert.</div>'}
      </div>
      <div class="outbound-detail-block">
        <div class="outbound-kicker">Diagnostik</div>
        ${renderDiagnosticsList([
          ['Firma', company?.name || item.company_name || 'offen'],
          ['Pipeline-ID', item.id],
          ['Kontakte', String((item.contacts || []).length)],
          ['Letzter Run', runs[0] ? `${runs[0].run_type || 'Run'} · ${runs[0].status || 'offen'}` : 'kein Run'],
        ])}
      </div>
    </div>
  `;
}

function companyDiagnostics(company, source, item, runs) {
  const latestRun = runs
    .slice()
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0];
  const sourceCommand = sourceImportCommandStatus(source);
  return [
    ['Company-ID', company.id],
    ['Campaign-ID', company.campaign_id || 'nicht gesetzt'],
    ['Datenquelle', source ? `${source.source_type || 'Quelle'} · ${source.status || 'offen'}` : 'keine Quelle'],
    ['Import-Command', sourceCommand || source?.payload?.task_status || 'kein Status'],
    ['Research-Status', companyResearchStatus(company) || company.research_status || 'nicht gestartet'],
    ['Pipeline-Eintrag', item ? `${item.stage || 'Pipeline'} · ${item.contact_research_status || 'offen'}` : 'noch nicht übernommen'],
    ['Letzter Run', latestRun ? `${latestRun.run_type || 'Run'} · ${latestRun.status || 'offen'}` : 'kein Run'],
  ];
}

function renderDiagnosticsList(items) {
  return `
    <dl class="outbound-diagnostics">
      ${items.map(([label, value]) => `<div><dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value || 'offen')}</dd></div>`).join('')}
    </dl>
  `;
}

async function createCampaign() {
  const name = await showBusinessPrompt(t('campaignNamePrompt', 'Name der Outbound Campaign'), {
    title: t('createCampaignTitle', 'Campaign anlegen'),
    defaultValue: t('defaultCampaignName', 'Neue Outbound Campaign'),
  });
  if (!name) return;
  const now = Date.now();
  const id = `camp_${crypto.randomUUID()}`;
  const campaign = {
    id,
    name,
    objective: t('campaignObjectiveDefault', 'Outbound Firmenqualifizierung'),
    market: 'DACH',
    status: 'active',
    owner_id: state.ctx?.session?.user?.id || '',
    source_count: 0,
    company_count: 0,
    qualified_count: 0,
    pipeline_count: 0,
    payload: { outbound_only: true },
    created_at_ms: now,
    updated_at_ms: now,
  };
  await state.ctx.db.raw.outbound_campaigns.insert(campaign);
  await ensureCampaignKnowledge(campaign).catch((error) => {
    console.warn('[outbound] campaign knowledge setup failed', error);
  });
  state.selectedCampaignId = id;
  await loadAll();
  render();
}

async function saveCampaignInlineEdit(campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign) return;
  const editor = state.ctx?.host?.querySelector(`.outbound-campaign-edit[data-id="${cssEscape(campaign.id)}"]`);
  if (!editor) return;
  const name = editor.querySelector('[data-campaign-edit-field="name"]')?.value?.trim() || '';
  if (!name) {
    updateCampaignEditSaveState(editor);
    editor.querySelector('[data-campaign-edit-field="name"]')?.focus();
    return;
  }
  const subtitle = editor.querySelector('[data-campaign-edit-field="subtitle"]')?.value?.trim() || '';
  const scope = editor.querySelector('[data-campaign-edit-field="scope"]')?.value?.trim() || '';
  const payload = {
    ...(campaign.payload || {}),
    subtitle,
    scope,
  };
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaign.id, {
    name,
    objective: scope,
    payload,
    updated_at_ms: Date.now(),
  });
  await ensureCampaignKnowledge({ ...campaign, name, objective: scope, payload }).catch((error) => {
    console.warn('[outbound] campaign runbook update failed', error);
  });
  state.editingCampaignId = '';
  await loadAll();
  render();
}

async function deleteCampaign(campaignId) {
  const campaign = state.campaigns.find((item) => item.id === campaignId);
  if (!campaign) return;
  const ok = await showBusinessConfirm(t('deleteConfirm', 'Campaign "{0}" löschen? Importjobs, Firmen und Pipeline-Einträge dieser Campaign werden entfernt.', campaign.name), {
    title: t('deleteTitle', 'Campaign löschen'),
    confirmLabel: t('delete', 'Löschen'),
  });
  if (!ok) return;
  await removeWhere(state.ctx.db.raw.outbound_sources, (item) => item.campaign_id === campaign.id);
  await removeWhere(state.ctx.db.raw.outbound_companies, (item) => item.campaign_id === campaign.id);
  await removeWhere(state.ctx.db.raw.outbound_pipeline_items, (item) => item.campaign_id === campaign.id);
  await removeWhere(state.ctx.db.raw.outbound_research_runs, (item) => item.campaign_id === campaign.id);
  await removeDoc(state.ctx.db.raw.outbound_campaigns, campaign.id);
  state.selectedCampaignId = '';
  await ensureDefaultCampaign();
  await loadAll();
  render();
}

async function openCompanyImporter() {
  const campaign = selectedCampaign();
  if (!campaign) return;
  const drawer = await openUniversalImporter(state.ctx, {
    side: 'left',
    moduleId: 'outbound',
    entityType: 'company_source',
    commandType: 'outbound.source.import',
    title: t('importJob', 'Importjob anlegen'),
    kicker: 'Outbound Import',
    defaultTitle: `${campaign.name} Import`,
    helperText: t('importerHelperText', 'URL, PDF, Text oder Excel liefern Unternehmen für den Input-Funnel. Der Importer extrahiert daraus Unternehmen; Personen werden erst später in der Pipeline recherchiert.'),
    filterPromptLabel: t('importFilter', 'Importfilter'),
    filterPromptPlaceholder: t('filterPromptPlaceholder', 'z.B. nur Firmen mit Sitz in Deutschland'),
    defaultFilterPrompt: t('defaultFilterPrompt', 'Nur Firmen mit Sitz in Deutschland importieren.'),
    submitLabel: t('submitImport', 'Importjob starten'),
    submittingLabel: t('importSubmitting', 'Importjob wird angelegt...'),
    doneLabel: t('importDone', 'Importjob angelegt.'),
    closeOnSubmit: true,
    dispatch: false,
    definition: {
      target_collection: 'outbound_companies',
      pipeline_boundary: 'company_first_contact_later',
      import_filter: {
        company_country_codes: ['DE'],
        company_country_labels: [t('germany', 'Deutschland'), t('germanyEn', 'Germany')],
        rule: t('ruleCountryGermany', 'Nur Unternehmen mit Sitz in Deutschland importieren.'),
      },
    },
    clientContext: { campaign_id: campaign.id },
    onImport: async ({ payload }) => {
      const result = await importCompaniesFromPayload(campaign, payload);
      if (result?.status === 'queued_parser') {
        window.setTimeout(() => {
          loadAll().then(render).catch((error) => console.warn('[outbound] refresh after async import failed', error));
        }, 0);
        return result;
      }
      await loadAll();
      render();
      return result;
    },
  });
  attachOutboundImportValidation(drawer);
}

function attachOutboundImportValidation(drawer) {
  if (!drawer) return;
  const refresh = () => {
    const validation = validateOutboundImportPayload(readOutboundImportDrawerState(drawer));
    const button = drawer.querySelector('[data-action="submit-importer"]');
    const status = drawer.querySelector('[data-import-status]');
    if (button) button.disabled = !validation.valid;
    if (status) status.textContent = validation.valid ? '' : validation.message;
  };
  drawer.addEventListener('input', refresh);
  drawer.addEventListener('change', () => window.setTimeout(refresh, 0));
  const stagedList = drawer.querySelector('[data-staged-files-list]');
  const observer = stagedList ? new MutationObserver(refresh) : null;
  observer?.observe(stagedList, { childList: true, subtree: true });
  const cleanup = () => observer?.disconnect();
  drawer.querySelector('[data-action="close-importer"]')?.addEventListener('click', cleanup, { once: true });
  refresh();
}

function readOutboundImportDrawerState(drawer) {
  return {
    title: drawer?.querySelector('[data-import-title]')?.value || '',
    source_type: drawer?.querySelector('[data-import-source]')?.value || 'text',
    source: {
      text: drawer?.querySelector('[data-import-text]')?.value || '',
      url: drawer?.querySelector('[data-import-url]')?.value || '',
      files: drawer?.stagedFiles || [],
    },
  };
}

function validateOutboundImportPayload(payload) {
  const title = String(payload?.title || '').trim();
  if (!title) return { valid: false, message: t('importValidationTitle', 'Bitte einen Titel eingeben.') };
  const sourceType = String(payload?.source_type || 'text');
  if (sourceType === 'text') {
    return String(payload?.source?.text || '').trim()
      ? { valid: true, message: '' }
      : { valid: false, message: t('importValidationText', 'Bitte Text oder Zeilen einfügen.') };
  }
  if (sourceType === 'url') {
    const url = String(payload?.source?.url || '').trim();
    try {
      const parsed = new URL(url);
      return ['http:', 'https:'].includes(parsed.protocol)
        ? { valid: true, message: '' }
        : { valid: false, message: t('importValidationUrlHttp', 'Bitte eine HTTP(S)-URL angeben.') };
    } catch {
      return { valid: false, message: t('importValidationUrl', 'Bitte eine gültige URL angeben.') };
    }
  }
  if (sourceType === 'document' || sourceType === 'excel') {
    return (payload?.source?.files || []).length
      ? { valid: true, message: '' }
      : { valid: false, message: t('importValidationFile', 'Bitte mindestens eine Datei auswählen.') };
  }
  return { valid: true, message: '' };
}

async function importCompaniesFromPayload(campaign, payload) {
  const now = Date.now();
  const sourceId = `src_${crypto.randomUUID()}`;
  const rows = extractRowsFromPayload(payload);
  const sourceStatus = rows.length ? 'imported' : 'queued_parser';
  await state.ctx.db.raw.outbound_sources.insert({
    id: sourceId,
    campaign_id: campaign.id,
    title: payload.title,
    source_type: payload.source_type,
    status: sourceStatus,
    file_name: payload.source?.files?.[0]?.name || '',
    row_count: rows.length,
    imported_count: rows.length,
    payload,
    created_at_ms: now,
    updated_at_ms: now,
  });
  if (!rows.length) {
    runOutboundImportInBackground(campaign, payload, sourceId);
    return {
      status: sourceStatus,
      message: t('importStarted', 'Importjob gestartet.'),
      detail: t('importBackgroundDetail', 'CTOX extrahiert Unternehmen im Hintergrund.'),
      dispatch: false,
    };
  }
  const refs = await ensureCampaignKnowledge(campaign).catch((error) => {
    console.warn('[outbound] knowledge campaign setup failed', error);
    return campaignKnowledgeRefs(campaign);
  });
  const filteredRows = filterRowsForCampaignImport(campaign, payload, rows);
  for (const row of filteredRows) {
    const companyId = companyIdFromImportRow(campaign, row);
    const company = {
      id: companyId,
      campaign_id: campaign.id,
      source_id: sourceId,
      row_index: row.row_index || 0,
      name: row.name,
      website: row.website || '',
      domain: row.domain || '',
      city: row.city || '',
      country: row.country || '',
      qualification_status: 'new',
      research_status: 'pending',
      pipeline_status: 'not_started',
      fit_score: 0,
      fit_status: 'unqualified',
      company_data: {},
      evidence: [],
      payload: { imported_row: row.raw || row },
      created_at_ms: now,
      updated_at_ms: now,
    };
    await upsertDoc(state.ctx.db.raw.outbound_companies, company.id, company);
  }
  await appendKnowledgeRows(campaign, refs.companiesKey, filteredRows.map((row) => companyKnowledgeRow(campaign, sourceId, row, now))).catch((error) => {
    console.warn('[outbound] knowledge companies append failed', error);
  });
  await updateCampaignCounts(campaign.id);
  return {
    status: sourceStatus,
    message: t('companiesImported', '{0} Firmen importiert.', filteredRows.length),
    detail: '',
    dispatch: false,
  };
}

function runOutboundImportInBackground(campaign, payload, sourceId) {
  queueOutboundImportCommand(campaign, payload, sourceId)
    .then(async (result) => {
      const refs = campaignKnowledgeRefs(campaign);
      const importedCount = await countKnowledgeRowsForSource(refs, sourceId).catch(() => 0);
      await patchDoc(state.ctx.db.raw.outbound_sources, sourceId, {
        status: result?.status === 'completed' ? 'imported' : result?.status || 'queued_parser',
        row_count: importedCount,
        imported_count: importedCount,
        payload: {
          ...payload,
          command_id: result?.command_id || payload.record_id || '',
          task_id: result?.task_id || '',
          task_status: result?.task_status || result?.status || '',
        },
        updated_at_ms: Date.now(),
      });
      await updateCampaignCounts(campaign.id);
      await loadAll();
      render();
    })
    .catch(async (error) => {
      console.warn('[outbound] CTOX source import failed', error);
      await patchDoc(state.ctx.db.raw.outbound_sources, sourceId, {
        status: 'failed_parser',
        payload: {
          ...payload,
          error: error?.message || String(error),
        },
        updated_at_ms: Date.now(),
      }).catch(() => {});
      await loadAll().catch(() => {});
      render();
    });
}

async function countKnowledgeRowsForSource(refs, sourceId) {
  const rows = await readKnowledgeRows(refs, refs.companiesKey, 10000);
  return rows.filter((row) => row.source_id === sourceId).length;
}

function filterRowsForCampaignImport(campaign, payload, rows) {
  const allowed = importCountryFilter(campaign, payload);
  if (!allowed.length) return rows;
  return rows.filter((row) => {
    const country = normalizeCountryCode(row.country || row.raw?.country || row.raw?.land || '');
    return !country || allowed.includes(country);
  });
}

function importCountryFilter(campaign, payload) {
  const raw = payload?.definition?.import_filter?.company_country_codes
    || campaign?.payload?.import_filter?.company_country_codes
    || ['DE'];
  return Array.isArray(raw)
    ? raw.map(normalizeCountryCode).filter(Boolean)
    : [];
}

function normalizeCountryCode(value) {
  const text = String(value || '').trim().toLowerCase();
  if (!text) return '';
  if (['de', 'deu', 'ger', 'germany', 'deutschland', 'bundesrepublik deutschland'].includes(text)) return 'DE';
  if (['at', 'aut', 'austria', 'österreich', 'oesterreich'].includes(text)) return 'AT';
  if (['ch', 'che', 'switzerland', 'schweiz', 'suisse'].includes(text)) return 'CH';
  return text.toUpperCase();
}

function companyKnowledgeRow(campaign, sourceId, row, now) {
  return {
    company_id: companyIdFromImportRow(campaign, row),
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    source_id: sourceId,
    row_index: row.row_index || 0,
    company_name: row.name || '',
    website: row.website || '',
    domain: row.domain || '',
    city: row.city || '',
    country: row.country || '',
    qualification_status: 'new',
    research_status: 'pending',
    pipeline_status: 'not_started',
    fit_score: 0,
    fit_status: 'unqualified',
    imported_at_ms: now,
    updated_at_ms: now,
    raw_json: JSON.stringify(row.raw || row),
  };
}

function companyIdFromImportRow(campaign, row) {
  return `co_${fingerprint(companyIdentityKey({
    campaign_id: campaign.id,
    name: row.name || '',
    domain: row.domain || '',
    website: row.website || '',
    country: row.country || '',
  }))}`;
}

function companyStatusKnowledgeRow(campaign, company, now) {
  return {
    company_id: company.id,
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    source_id: company.source_id || '',
    row_index: company.row_index || 0,
    company_name: company.name || '',
    website: company.website || '',
    domain: company.domain || domainFromUrl(company.website),
    city: company.city || '',
    country: company.country || '',
    qualification_status: company.qualification_status || 'new',
    research_status: company.research_status || 'pending',
    pipeline_status: company.pipeline_status || 'not_started',
    fit_score: Number(company.fit_score || 0),
    fit_status: company.fit_status || 'unqualified',
    company_data_json: JSON.stringify(company.company_data || {}),
    evidence_json: JSON.stringify(company.evidence || []),
    updated_at_ms: now,
    raw_json: JSON.stringify(company.payload?.imported_row || {}),
  };
}

function pipelineSeedKnowledgeRow(campaign, company, pipelineItem, now) {
  return {
    pipeline_id: pipelineItem.id,
    company_id: company.id,
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    company_name: company.name || '',
    stage: pipelineItem.stage || 'contact_research',
    contact_research_status: pipelineItem.contact_research_status || 'pending',
    outreach_status: pipelineItem.outreach_status || 'not_started',
    priority: pipelineItem.priority || 'normal',
    updated_at_ms: now,
  };
}

function researchWritebackContract(campaign, refs, stage, record, fields = []) {
  const base = {
    system: 'ctox knowledge data',
    mode: 'append_latest_row',
    domain: refs.domain,
    runbook_id: refs.runbookId,
    campaign_id: campaign.id,
    campaign_name: campaign.name,
    rule: 'Append a new row with the same stable id and updated_at_ms. Do not create a separate datastore. Do not research persons before the pipeline contact stage.',
  };
  if (stage === 'company_research') {
    return {
      ...base,
      table_key: refs.companiesKey,
      stable_id_column: 'company_id',
      stable_id_value: record.id,
      required_columns: [
        'company_id',
        'campaign_id',
        'company_name',
        'website',
        'domain',
        'qualification_status',
        'research_status',
        'fit_score',
        'fit_status',
        'company_data_json',
        'evidence_json',
        'updated_at_ms',
      ],
      requested_fields: fields.map((field) => ({ id: field.id, label: field.label })),
      append_command: `ctox knowledge data append --domain ${refs.domain} --key ${refs.companiesKey} --rows '<json-array>'`,
    };
  }
  if (stage === 'contact_research' || stage === 'lead_qualification') {
    return {
      ...base,
      table_key: refs.contactsKey,
      stable_id_column: 'pipeline_id',
      stable_id_value: record.id,
      company_id: record.company_id,
      required_columns: [
        'pipeline_id',
        'company_id',
        'campaign_id',
        'company_name',
        'contact_id',
        'contact_name',
        'role',
        'email',
        'linkedin_url',
        'contact_research_status',
        'lead_status',
        'evidence_json',
        'updated_at_ms',
      ],
      append_command: `ctox knowledge data append --domain ${refs.domain} --key ${refs.contactsKey} --rows '<json-array>'`,
    };
  }
  return base;
}

async function queueOutboundImportCommand(campaign, payload, sourceId) {
  const refs = await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign));
  const commandId = payload.record_id || `import_${Date.now()}_${crypto.randomUUID()}`;
  const command = {
    id: commandId,
    module: 'outbound',
    type: 'outbound.source.import',
    record_id: commandId,
    inbound_channel: 'business_os.outbound',
    payload: {
      ...payload,
      source_id: sourceId,
      instruction: [
        t('importPromptLabel', 'Lies den Importjob als Firmenliste, folge bei Listen/Verzeichnissen den noetigen Unternehmensdetailseiten und schreibe alle gefundenen Unternehmen als record-shaped Knowledge in den Campaign Companies DataFrame. Keine Personen recherchieren.'),
        payload.filter_prompt ? t('importFilterStrict', 'Importfilter strikt anwenden: {0}', payload.filter_prompt) : '',
      ].filter(Boolean).join('\n'),
      writeback_contract: {
        system: 'ctox knowledge data',
        mode: 'append_rows',
        domain: refs.domain,
        table_key: refs.companiesKey,
        runbook_id: refs.runbookId,
        campaign_id: campaign.id,
        source_id: sourceId,
        filter_prompt: payload.filter_prompt || '',
        required_columns: [
          'company_id',
          'campaign_id',
          'source_id',
          'company_name',
          'website',
          'domain',
          'city',
          'country',
          'qualification_status',
          'research_status',
          'pipeline_status',
          'imported_at_ms',
          'updated_at_ms',
          'raw_json',
        ],
        append_command: `ctox knowledge data append --domain ${refs.domain} --key ${refs.companiesKey} --rows '<json-array>'`,
      },
      knowledge: {
        domain: refs.domain,
        companies_table_key: refs.companiesKey,
        contacts_table_key: refs.contactsKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      entity_type: 'company_source',
      campaign_id: campaign.id,
      source_id: sourceId,
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.companiesKey,
    },
  };
  return state.ctx.commandBus.dispatch(command);
}

function extractRowsFromPayload(payload) {
  if (payload.source_type === 'text') {
    return extractCompanyRowsFromText(payload.source?.text || '');
  }
  if (payload.source_type === 'url') {
    return [];
  }
  const rows = [];
  for (const file of payload.source?.files || []) {
    const text = file.text || decodeBase64Utf8(file.base64 || '');
    if (!text || !/\.(csv|tsv|txt)$/i.test(file.name)) continue;
    rows.push(...parseDelimitedText(text).map((row, index) => normalizeCompanyRow(row, index)));
  }
  return rows.filter((row) => row.name);
}

async function queueCompanyResearch(companyId, options = {}) {
  const company = state.companies.find((item) => item.id === companyId);
  const campaign = options.forceCampaign || selectedCampaign();
  if (!company || !campaign) return;
  const refs = await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign));
  const researchSettings = options.forceSettings || getCampaignResearchSettings(campaign);
  const researchFields = options.fieldsOverride || researchFieldsForPrompt(researchSettings);
  const runId = `run_${crypto.randomUUID()}`;
  const command = {
    id: `cmd_${runId}`,
    module: 'outbound',
    type: 'outbound.company.research',
    record_id: company.id,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: t('researchCompanyDataTitle', 'Unternehmensdaten recherchieren: {0}', company.name),
      instruction: options.reason === 'custom_fields_added'
        ? t('researchCompanyDataNew', 'Recherchiere die neu hinzugefügten Unternehmensdaten-Kategorien für die Outbound-Qualifizierung. Schreibe das Ergebnis ausschließlich in den angegebenen Knowledge DataFrame. Keine Personen adressieren und keine Outreach-Nachricht erstellen.')
        : t('researchCompanyDataStandard', 'Recherchiere nur Unternehmensdaten für die Outbound-Qualifizierung. Schreibe das Ergebnis ausschließlich in den angegebenen Knowledge DataFrame. Keine Personen adressieren und keine Outreach-Nachricht erstellen.'),
      research_request: {
        tool: 'ctox web research',
        mode: 'new_record',
        company: company.name,
        country: company.country || 'DE',
        fields: researchFields,
        custom_instruction: [
          researchSettings.customInstruction,
          options.reason === 'custom_fields_added' ? t('researchMissingOrNew', 'Nur fehlende oder neue Kategorien nachrecherchieren und bestehende Unternehmensdaten nicht entfernen.') : '',
        ].filter(Boolean).join('\n'),
        include_private: [],
      },
      company: serializeCompany(company),
      campaign: { id: campaign.id, name: campaign.name, objective: campaign.objective },
      writeback_contract: researchWritebackContract(campaign, refs, 'company_research', company, researchFields),
      knowledge: {
        domain: refs.domain,
        companies_table_key: refs.companiesKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      campaign_id: campaign.id,
      company_id: company.id,
      research_boundary: 'company_only_before_pipeline',
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.companiesKey,
      knowledge_runbook_id: refs.runbookId,
    },
  };
  const result = await state.ctx.commandBus.dispatch(command).catch((error) => ({ ok: false, status: 'pending_sync_failed', error: error?.message || String(error) }));
  const now = Date.now();
  await state.ctx.db.raw.outbound_research_runs.insert({
    id: runId,
    campaign_id: campaign.id,
    company_id: company.id,
    pipeline_id: '',
    run_type: 'company_research',
    status: result.status || 'queued',
    command_id: result.command_id || command.id,
    request: command.payload.research_request,
    result: {},
    error: result.error || '',
    created_at_ms: now,
    updated_at_ms: now,
  });
  await appendKnowledgeRows(campaign, refs.runsKey, [{
    run_id: runId,
    command_id: result.command_id || command.id,
    campaign_id: campaign.id,
    record_id: company.id,
    run_type: 'company_research',
    status: result.status || 'queued',
    ctox_status: result.status || 'pending_sync',
    created_at_ms: now,
    request_json: JSON.stringify(command.payload.research_request),
  }]).catch((error) => console.warn('[outbound] knowledge runs append failed', error));
  await patchDoc(state.ctx.db.raw.outbound_companies, company.id, {
    research_status: result.status || 'pending_sync',
    updated_at_ms: now,
  });
}

async function queueContactResearch(pipelineId) {
  const item = state.pipeline.find((entry) => entry.id === pipelineId);
  if (!item) return;
  const campaign = state.campaigns.find((entry) => entry.id === item.campaign_id);
  const researchSettings = getCampaignResearchSettings(campaign);
  const contactFields = contactFieldsForPrompt(researchSettings);
  const refs = campaign
    ? await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign))
    : campaignKnowledgeRefs({ id: item.campaign_id, name: item.company_name });
  const runId = `run_${crypto.randomUUID()}`;
  const command = {
    id: `cmd_${runId}`,
    module: 'outbound',
    type: 'outbound.pipeline.contact_research',
    record_id: item.id,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: `Ansprechpartner recherchieren: ${item.company_name}`,
      instruction: 'Jetzt beginnt die Pipeline-Stufe: Recherchiere relevante öffentlich belegbare Ansprechpartner und Rollen für eine spätere Ansprache. Schreibe Kontakte ausschließlich in den angegebenen Knowledge Contacts DataFrame.',
      company_id: item.company_id,
      pipeline_id: item.id,
      contact_fields: contactFields,
      custom_instruction: researchSettings.customInstruction || '',
      writeback_contract: researchWritebackContract(campaign || { id: item.campaign_id, name: item.company_name }, refs, 'contact_research', item),
      knowledge: {
        domain: refs.domain,
        contacts_table_key: refs.contactsKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      campaign_id: item.campaign_id,
      pipeline_id: item.id,
      research_boundary: 'pipeline_contact_stage',
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.contactsKey,
      knowledge_runbook_id: refs.runbookId,
    },
  };
  const result = await state.ctx.commandBus.dispatch(command).catch((error) => ({ ok: false, status: 'pending_sync_failed', error: error?.message || String(error) }));
  const now = Date.now();
  await state.ctx.db.raw.outbound_research_runs.insert({
    id: runId,
    campaign_id: item.campaign_id,
    company_id: item.company_id,
    pipeline_id: item.id,
    run_type: 'contact_research',
    status: result.status || 'queued',
    command_id: result.command_id || command.id,
    request: command.payload,
    result: {},
    error: result.error || '',
    created_at_ms: now,
    updated_at_ms: now,
  });
  await appendKnowledgeRows(campaign, refs.runsKey, [{
    run_id: runId,
    command_id: result.command_id || command.id,
    campaign_id: item.campaign_id,
    record_id: item.id,
    company_id: item.company_id,
    run_type: 'contact_research',
    status: result.status || 'queued',
    ctox_status: result.status || 'pending_sync',
    created_at_ms: now,
    request_json: JSON.stringify(command.payload),
  }]).catch((error) => console.warn('[outbound] knowledge runs append failed', error));
  await patchDoc(state.ctx.db.raw.outbound_pipeline_items, item.id, {
    contact_research_status: result.status || 'pending_sync',
    updated_at_ms: now,
  });
}

async function queueLeadQualification(pipelineId) {
  const item = state.pipeline.find((entry) => entry.id === pipelineId);
  const campaign = state.campaigns.find((entry) => entry.id === item?.campaign_id);
  if (!item || !campaign) return;
  const researchSettings = getCampaignResearchSettings(campaign);
  const contactFields = contactFieldsForPrompt(researchSettings);
  const refs = await ensureCampaignKnowledge(campaign).catch(() => campaignKnowledgeRefs(campaign));
  const runId = `run_${crypto.randomUUID()}`;
  const command = {
    id: `cmd_${runId}`,
    module: 'outbound',
    type: 'outbound.pipeline.lead_qualification',
    record_id: item.id,
    inbound_channel: 'business_os.outbound',
    payload: {
      title: `Lead qualifizieren: ${item.company_name}`,
      instruction: 'Qualifiziere die recherchierten Ansprechpartner gegen den Campaign Scope/ICP. Schreibe die Lead-Qualifikation ausschließlich in den Knowledge Contacts DataFrame. Keine Outreach-Nachricht senden.',
      company_id: item.company_id,
      pipeline_id: item.id,
      campaign: { id: campaign.id, name: campaign.name, objective: campaign.objective, scope: campaign.payload?.scope || '' },
      contacts: item.contacts || [],
      contact_fields: contactFields,
      custom_instruction: researchSettings.customInstruction || '',
      qualification_goal: 'lead_qualified_or_rejected',
      writeback_contract: researchWritebackContract(campaign, refs, 'lead_qualification', item),
      knowledge: {
        domain: refs.domain,
        contacts_table_key: refs.contactsKey,
        runs_table_key: refs.runsKey,
        runbook_id: refs.runbookId,
      },
    },
    client_context: {
      source_module: 'outbound',
      campaign_id: item.campaign_id,
      pipeline_id: item.id,
      research_boundary: 'lead_qualification_stage',
      writeback_required: true,
      knowledge_domain: refs.domain,
      knowledge_table_key: refs.contactsKey,
      knowledge_runbook_id: refs.runbookId,
    },
  };
  const result = await state.ctx.commandBus.dispatch(command).catch((error) => ({ ok: false, status: 'pending_sync_failed', error: error?.message || String(error) }));
  const now = Date.now();
  await state.ctx.db.raw.outbound_research_runs.insert({
    id: runId,
    campaign_id: item.campaign_id,
    company_id: item.company_id,
    pipeline_id: item.id,
    run_type: 'lead_qualification',
    status: result.status || 'queued',
    command_id: result.command_id || command.id,
    request: command.payload,
    result: {},
    error: result.error || '',
    created_at_ms: now,
    updated_at_ms: now,
  });
  await appendKnowledgeRows(campaign, refs.runsKey, [{
    run_id: runId,
    command_id: result.command_id || command.id,
    campaign_id: item.campaign_id,
    record_id: item.id,
    company_id: item.company_id,
    run_type: 'lead_qualification',
    status: result.status || 'queued',
    ctox_status: result.status || 'pending_sync',
    created_at_ms: now,
    request_json: JSON.stringify(command.payload),
  }]).catch((error) => console.warn('[outbound] knowledge runs append failed', error));
  await patchDoc(state.ctx.db.raw.outbound_pipeline_items, item.id, {
    outreach_status: result.status || 'pending_sync',
    stage: 'lead_qualification',
    updated_at_ms: now,
  });
}

async function setCompanyQualification(companyId, status) {
  const company = state.companies.find((item) => item.id === companyId);
  const campaign = state.campaigns.find((item) => item.id === company?.campaign_id) || selectedCampaign();
  const now = Date.now();
  await patchDoc(state.ctx.db.raw.outbound_companies, companyId, {
    qualification_status: status,
    fit_status: status === 'qualified' ? 'fit' : status === 'rejected' ? 'not_fit' : 'unqualified',
    fit_score: status === 'qualified' ? 75 : status === 'rejected' ? 10 : 0,
    updated_at_ms: now,
  });
  if (company && campaign) {
    const nextCompany = {
      ...company,
      qualification_status: status,
      fit_status: status === 'qualified' ? 'fit' : status === 'rejected' ? 'not_fit' : 'unqualified',
      fit_score: status === 'qualified' ? 75 : status === 'rejected' ? 10 : 0,
      updated_at_ms: now,
    };
    const refs = campaignKnowledgeRefs(campaign);
    await appendKnowledgeRows(campaign, refs.companiesKey, [companyStatusKnowledgeRow(campaign, nextCompany, now)]).catch((error) => {
      console.warn('[outbound] knowledge company status append failed', error);
    });
  }
  await updateCampaignCounts(campaign?.id || state.selectedCampaignId);
}

async function sendCompanyToPipeline(companyId, options = {}) {
  const company = state.companies.find((item) => item.id === companyId);
  const campaign = options.forceCampaign || selectedCampaign();
  if (!company || !campaign) return;
  if (company.qualification_status !== 'qualified') {
    await setCompanyQualification(company.id, 'qualified');
  }
  const existing = state.pipeline.find((item) => item.company_id === company.id);
  const now = Date.now();
  let pipelineItem = existing;
  if (!existing) {
    pipelineItem = {
      id: `pipe_${crypto.randomUUID()}`,
      campaign_id: campaign.id,
      company_id: company.id,
      company_name: company.name,
      stage: 'contact_research',
      contact_research_status: 'pending',
      outreach_status: 'not_started',
      priority: company.fit_score >= 80 ? 'high' : 'normal',
      contacts: [],
      payload: { company_snapshot: serializeCompany(company) },
      created_at_ms: now,
      updated_at_ms: now,
    };
    await state.ctx.db.raw.outbound_pipeline_items.insert(pipelineItem);
  }
  await patchDoc(state.ctx.db.raw.outbound_companies, company.id, {
    pipeline_status: 'pipeline',
    qualification_status: 'qualified',
    updated_at_ms: now,
  });
  const refs = campaignKnowledgeRefs(campaign);
  const nextCompany = { ...company, pipeline_status: 'pipeline', qualification_status: 'qualified', updated_at_ms: now };
  await Promise.all([
    appendKnowledgeRows(campaign, refs.companiesKey, [companyStatusKnowledgeRow(campaign, nextCompany, now)]),
    pipelineItem ? appendKnowledgeRows(campaign, refs.contactsKey, [pipelineSeedKnowledgeRow(campaign, nextCompany, pipelineItem, now)]) : Promise.resolve(null),
  ]).catch((error) => {
    console.warn('[outbound] knowledge pipeline append failed', error);
  });
  if (!options.keepView) state.activeView = 'pipeline';
  await updateCampaignCounts(campaign.id);
}

async function updateCampaignCounts(campaignId) {
  const raw = state.ctx?.db?.raw || {};
  const companies = dedupeCompanies((await findAll(raw.outbound_companies)).filter((item) => item.campaign_id === campaignId));
  const sources = (await findAll(raw.outbound_sources)).filter((item) => item.campaign_id === campaignId);
  const pipeline = (await findAll(raw.outbound_pipeline_items)).filter((item) => item.campaign_id === campaignId);
  await patchDoc(state.ctx.db.raw.outbound_campaigns, campaignId, {
    source_count: sources.length,
    company_count: companies.length,
    qualified_count: companies.filter((item) => item.qualification_status === 'qualified').length,
    pipeline_count: pipeline.length,
    updated_at_ms: Date.now(),
  });
}

function selectedCampaign() {
  return state.campaigns.find((item) => item.id === state.selectedCampaignId) || visibleCampaigns()[0] || state.campaigns[0] || null;
}

function currentSources() {
  return campaignScopedRows(state, state.selectedCampaignId).sources;
}

function currentCompanies() {
  return dedupeCompanies(campaignScopedRows(state, state.selectedCampaignId).companies);
}

function currentPipeline() {
  return dedupePipelineItems(campaignScopedRows(state, state.selectedCampaignId).pipeline);
}

function selectedCompany() {
  return currentCompanies().find((item) => item.id === state.selectedCompanyId || item.duplicate_company_ids?.includes(state.selectedCompanyId)) || currentCompanies()[0] || null;
}

function selectedPipelineItem() {
  return currentPipeline().find((item) => item.id === state.selectedPipelineId) || currentPipeline()[0] || null;
}

function campaignScopedRows(data, campaignId) {
  const campaigns = data.campaigns || [];
  const sources = data.sources || [];
  const companies = data.companies || [];
  const pipeline = data.pipeline || [];
  const directSources = sources.filter((item) => item.campaign_id === campaignId);
  const directCompanies = companies.filter((item) => item.campaign_id === campaignId);
  const directPipeline = pipeline.filter((item) => item.campaign_id === campaignId);
  if (directCompanies.length || directPipeline.length) {
    const companyIds = new Set(directCompanies.map((item) => item.id));
    return {
      sources: directSources,
      companies: directCompanies,
      pipeline: dedupePipelineItems([...directPipeline, ...pipeline.filter((item) => companyIds.has(item.company_id))]),
      recovered: false,
    };
  }
  const campaign = campaigns.find((item) => item.id === campaignId);
  const recoverForCampaign = campaigns.length <= 1 || campaign?.name === DEFAULT_CAMPAIGN_NAME || campaign?.id === DEFAULT_CAMPAIGN_ID;
  if (!campaignId || !recoverForCampaign) {
    return { sources: [], companies: [], pipeline: [], recovered: false };
  }
  const knownCampaignIds = new Set(campaigns.map((item) => item.id).filter(Boolean));
  const recoverable = (item) => !item.campaign_id || !knownCampaignIds.has(item.campaign_id) || campaigns.length <= 1;
  const recoveredCompanies = companies.filter(recoverable);
  const recoveredCompanyIds = new Set(recoveredCompanies.map((item) => item.id));
  return {
    sources: [...directSources, ...sources.filter((item) => item.campaign_id !== campaignId && recoverable(item))],
    companies: recoveredCompanies,
    pipeline: dedupePipelineItems(pipeline.filter((item) => recoverable(item) || recoveredCompanyIds.has(item.company_id))),
    recovered: recoveredCompanies.length > 0 || sources.some(recoverable) || pipeline.some(recoverable),
  };
}

function filteredCompanies() {
  return filteredQualificationRows().map((row) => row.company);
}

function loadHiddenCompanies() {
  const campaignId = state.selectedCampaignId;
  if (!campaignId) return [];
  try {
    const data = localStorage.getItem(`outbound_hidden_companies_${campaignId}`);
    return data ? JSON.parse(data) : [];
  } catch (e) {
    console.error('Error loading hidden companies:', e);
    return [];
  }
}

function saveHiddenCompanies(list) {
  const campaignId = state.selectedCampaignId;
  if (!campaignId) return;
  try {
    localStorage.setItem(`outbound_hidden_companies_${campaignId}`, JSON.stringify(list));
  } catch (e) {
    console.error('Error saving hidden companies:', e);
  }
}

function hideCompany(companyId) {
  const list = loadHiddenCompanies();
  if (!list.includes(companyId)) {
    list.push(companyId);
    saveHiddenCompanies(list);
  }
}

function unhideCompany(companyId) {
  const list = loadHiddenCompanies();
  const index = list.indexOf(companyId);
  if (index !== -1) {
    list.splice(index, 1);
    saveHiddenCompanies(list);
  }
}

function filteredQualificationRows() {
  const hiddenList = loadHiddenCompanies();
  const search = state.search.trim().toLowerCase();
  const rows = currentCompanies().map((company) => ({
    company,
    item: pipelineItemForCompany(company),
  })).filter((row) => {
    const { company, item } = row;
    if (hiddenList.includes(company.id)) return false;
    if (search && !`${company.name} ${company.domain} ${company.city}`.toLowerCase().includes(search)) return false;
    if (state.filter === 'research' && !(company.research_status === 'pending' || company.research_status === 'queued')) return false;
    if (state.filter === 'qualified' && company.qualification_status !== 'qualified') return false;
    if (state.filter === 'contact_qualified' && !isContactQualified(item)) return false;
    if (state.filter === 'lead_qualified' && !isLeadQualified(item)) return false;
    if (state.filter === 'pipeline' && company.pipeline_status !== 'pipeline') return false;
    if (state.filter === 'rejected' && company.qualification_status !== 'rejected') return false;

    // Status and tag filter matching on contacts
    if (state.statusFilter || state.tagFilter) {
      const contacts = Array.isArray(item?.contacts) ? item.contacts : [];
      const hasMatchingContact = contacts.some(c => {
        let statusMatch = true;
        let tagMatch = true;
        if (state.statusFilter) {
          const chips = chipsFromMultiSelect(c.status_field || c.status).map(x => x.toLowerCase());
          statusMatch = chips.includes(state.statusFilter.toLowerCase());
        }
        if (state.tagFilter) {
          const chips = chipsFromMultiSelect(c.tags).map(x => x.toLowerCase());
          tagMatch = chips.includes(state.tagFilter.toLowerCase());
        }
        return statusMatch && tagMatch;
      });
      if (!hasMatchingContact) return false;
    }

    return rowMatchesTableFilters(row);
  });
  return sortQualificationRows(rows);
}

function rowMatchesTableFilters(row) {
  const filters = Object.entries(state.tableFilters || {}).filter(([, value]) => String(value || '').trim());
  if (!filters.length) return true;
  return filters.every(([columnId, value]) => normalizeFilterText(tableColumnValue(row, columnId)).includes(normalizeFilterText(value)));
}

function sortQualificationRows(rows) {
  const sort = state.tableSort;
  if (!sort?.column || !sort?.direction) return rows;
  const direction = sort.direction === 'desc' ? -1 : 1;
  return [...rows].sort((a, b) => compareTableValues(tableColumnValue(a, sort.column), tableColumnValue(b, sort.column)) * direction);
}

function tableColumnValue(row, columnId) {
  if (!row) return '';
  if (columnId === 'company.name') return row.company.name;
  if (columnId === 'company.domain') return row.company.domain || domainFromUrl(row.company.website) || '';
  if (columnId?.startsWith('field.')) return companyResearchValue(row.company, columnId.slice(6));
  if (columnId === 'company.qualification') return labelQualification(row.company.qualification_status);
  if (columnId?.startsWith('contact.') || columnId?.startsWith('lead.')) return contactColumnValue(row.item, columnId);
  return '';
}

function setTableSort(column) {
  if (column === 'contact.action') return;
  if (state.tableSort?.column !== column) {
    state.tableSort = { column, direction: 'asc' };
    return;
  }
  if (state.tableSort.direction === 'asc') {
    state.tableSort = { column, direction: 'desc' };
    return;
  }
  state.tableSort = null;
}

function hasTableControls() {
  return Object.keys(state.tableFilters || {}).length > 0 || !!state.tableSort;
}

function renderCenterPreservingInput(input) {
  const filterKey = input.dataset.tableFilter || '';
  const isSearch = input.matches('[data-search]');
  const selectionStart = input.selectionStart;
  const selectionEnd = input.selectionEnd;
  renderCenter();
  const selector = isSearch ? '[data-search]' : `[data-table-filter="${cssEscape(filterKey)}"]`;
  const next = state.ctx?.host?.querySelector(selector);
  next?.focus?.();
  if (next && selectionStart != null && selectionEnd != null) next.setSelectionRange?.(selectionStart, selectionEnd);
}

function scheduleCenterRenderPreservingInput(input) {
  const filterKey = input.dataset.tableFilter || '';
  const isSearch = input.matches('[data-search]');
  const selectionStart = input.selectionStart;
  const selectionEnd = input.selectionEnd;
  const selector = isSearch ? '[data-search]' : `[data-table-filter="${cssEscape(filterKey)}"]`;
  if (state.centerRenderTimer) window.clearTimeout(state.centerRenderTimer);
  state.centerRenderTimer = window.setTimeout(() => {
    state.centerRenderTimer = null;
    renderCenter();
    const next = state.ctx?.host?.querySelector(selector);
    next?.focus?.();
    if (next && selectionStart != null && selectionEnd != null) next.setSelectionRange?.(selectionStart, selectionEnd);
  }, 120);
}

function compareTableValues(a, b) {
  const numericA = parseTableNumber(a);
  const numericB = parseTableNumber(b);
  if (Number.isFinite(numericA) && Number.isFinite(numericB)) return numericA - numericB;
  return normalizeFilterText(a).localeCompare(normalizeFilterText(b), 'de', { numeric: true, sensitivity: 'base' });
}

function parseTableNumber(value) {
  const text = String(value ?? '').trim().replace(/\s+/g, '');
  if (!text) return Number.NaN;
  const match = text.match(/^-?\d+(?:[.,]\d+)?/);
  if (!match) return Number.NaN;
  return Number(match[0].replace(',', '.'));
}

function normalizeFilterText(value) {
  return String(value ?? '').toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').trim();
}

function ensureSelectedCompanyInFilter() {
  const companies = filteredCompanies();
  if (!companies.some((company) => company.id === state.selectedCompanyId)) {
    state.selectedCompanyId = companies[0]?.id || '';
  }
}

async function findAll(collection) {
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc.toJSON ? doc.toJSON() : doc).sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
}

async function upsertDoc(collection, id, doc) {
  const existing = await collection.findOne(id).exec();
  if (existing) {
    await existing.incrementalPatch({ ...doc, created_at_ms: existing.created_at_ms || doc.created_at_ms });
    return;
  }
  await collection.insert(doc);
}

async function patchDoc(collection, id, patch) {
  const existing = await collection.findOne(id).exec();
  if (existing) await existing.incrementalPatch(patch);
}

async function removeDoc(collection, id) {
  const existing = await collection?.findOne(id).exec();
  if (existing) await existing.remove();
}

async function removeWhere(collection, predicate) {
  if (!collection) return;
  const docs = await collection.find().exec();
  for (const doc of docs) {
    const json = doc.toJSON ? doc.toJSON() : doc;
    if (predicate(json)) await doc.remove();
  }
}

function field(label, value) {
  return `<div><span class="outbound-field-label">${escapeHtml(label)}</span><div>${escapeHtml(value || '')}</div></div>`;
}

function renderEvidence(company) {
  const evidence = company.evidence || [];
  if (!evidence.length) return '<div class="outbound-muted">Noch keine belastbaren Quellen hinterlegt.</div>';
  return `<ul class="outbound-evidence">${evidence.map((item) => `<li>${escapeHtml(item.title || item.url || item.note || 'Quelle')}</li>`).join('')}</ul>`;
}

function labelResearch(value) {
  if (isAutomationActive(value) || isFailedStatus(value) || isCancelledStatus(value)) return automationStatusLabel(value);
  if (value === 'researched') return 'recherchiert';
  return 'offen';
}

function labelQualification(value) {
  if (value === 'qualified') return 'qualifiziert';
  if (value === 'rejected') return 'nicht passend';
  return 'neu';
}

function labelPipeline(value) {
  if (value === 'pipeline') return 'Pipeline';
  return 'nicht gestartet';
}

function serializeCompany(company) {
  return {
    id: company.id,
    name: company.name,
    website: company.website,
    domain: company.domain,
    city: company.city,
    country: company.country,
    company_data: company.company_data || {},
  };
}

function domainFromUrl(value) {
  try {
    const input = String(value || '').trim();
    if (!input) return '';
    return new URL(input.startsWith('http') ? input : `https://${input}`).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
}

function fingerprint(value) {
  let hash = 0;
  const text = String(value || '');
  for (let index = 0; index < text.length; index += 1) {
    hash = ((hash << 5) - hash + text.charCodeAt(index)) | 0;
  }
  return Math.abs(hash).toString(36);
}

function setupOutboundColumnResizing() {
  const root = state.ctx?.host?.querySelector?.('[data-outbound-root]');
  if (!root) return null;

  const handle = root.querySelector('[data-outbound-column-resizer]');
  if (!handle) return null;

  const applyCompact = (widthPx) => {
    root.dataset.leftCompact = widthPx <= 360 ? 'true' : 'false';
  };

  // Hydrate persisted width.
  let initialWidth = null;
  try {
    const raw = window.localStorage.getItem(OUTBOUND_LAYOUT_KEY);
    const stored = Number(raw);
    if (Number.isFinite(stored) && stored > 0) initialWidth = stored;
  } catch {
    /* storage unavailable */
  }
  if (!Number.isFinite(initialWidth)) initialWidth = 360;
  const clampedInitial = clampNumber(initialWidth, OUTBOUND_COL_MIN.left, OUTBOUND_COL_LEFT_MAX);
  root.style.setProperty('--outbound-left-width', `${clampedInitial}px`);
  applyCompact(clampedInitial);

  const resizer = new CtoxResizer({
    resizerEl: handle,
    containerEl: root,
    cssVar: '--outbound-left-width',
    side: 'left',
    minWidth: OUTBOUND_COL_MIN.left,
    maxWidth: OUTBOUND_COL_LEFT_MAX,
    onResize: (width) => {
      applyCompact(width);
      try {
        window.localStorage.setItem(OUTBOUND_LAYOUT_KEY, String(Math.round(width)));
      } catch {
        /* storage unavailable */
      }
    },
  });

  return () => {
    resizer.destroy();
    delete root.dataset.leftCompact;
  };
}

function setupCenterSplitResizing(root) {
  state.centerResizeCleanup?.();
  state.centerResizeCleanup = null;
  const split = root?.querySelector?.('[data-outbound-center-split]');
  const handle = root?.querySelector?.('[data-outbound-center-resizer]');
  if (!split || !handle) return;

  let activeWidths = null;
  let persistedRatio = readCenterSplitRatio();
  let dragState = null;
  let resizeRaf = 0;

  const applyWidths = (widths) => {
    if (!widths) return;
    split.style.gridTemplateColumns = `${widths.left}px 12px ${widths.right}px`;
  };

  const readCurrentWidths = () => {
    const tracks = String(getComputedStyle(split).gridTemplateColumns || '')
      .split(/\s+/)
      .map((part) => Number.parseFloat(part))
      .filter((number) => Number.isFinite(number) && number > 0);
    if (tracks.length < 3) return null;
    return { left: tracks[0], right: tracks[2] };
  };

  const clampWidths = (left, total) => {
    if (!Number.isFinite(total) || total < OUTBOUND_CENTER_MIN.left + OUTBOUND_CENTER_MIN.right) return null;
    const safeLeft = Math.round(clampNumber(left, OUTBOUND_CENTER_MIN.left, total - OUTBOUND_CENTER_MIN.right));
    return { left: safeLeft, right: Math.round(total - safeLeft) };
  };

  const totalWidth = () => {
    const cs = getComputedStyle(split);
    const gap = Number.parseFloat(cs.columnGap || cs.gap || '0') || 0;
    return Math.max(0, split.clientWidth - 12 - (gap * 2));
  };

  const syncLayout = () => {
    const total = totalWidth();
    if (total < OUTBOUND_CENTER_MIN.left + OUTBOUND_CENTER_MIN.right) {
      split.style.gridTemplateColumns = 'minmax(0, 1fr)';
      handle.hidden = true;
      return;
    }
    const current = readCurrentWidths();
    const left = persistedRatio ? total * persistedRatio : current?.left || total * 0.56;
    activeWidths = clampWidths(left, total);
    applyWidths(activeWidths);
    handle.hidden = false;
  };

  const stopDrag = () => {
    if (!dragState) return;
    dragState = null;
    handle.classList.remove('is-active');
    document.body.classList.remove('is-outbound-center-resizing');
    if (activeWidths) {
      const total = activeWidths.left + activeWidths.right;
      persistedRatio = total > 0 ? activeWidths.left / total : persistedRatio;
      writeCenterSplitRatio(persistedRatio);
    }
  };

  const startDrag = (event) => {
    const total = totalWidth();
    const initial = activeWidths || clampWidths(readCurrentWidths()?.left || total * 0.56, total);
    if (!initial) return;
    activeWidths = initial;
    dragState = {
      rect: split.getBoundingClientRect(),
      total,
    };
    handle.classList.add('is-active');
    document.body.classList.add('is-outbound-center-resizing');
    event.preventDefault();
  };

  const handleMove = (event) => {
    if (!dragState) return;
    const left = event.clientX - dragState.rect.left - 6;
    activeWidths = clampWidths(left, dragState.total);
    applyWidths(activeWidths);
  };

  const handleResize = () => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      syncLayout();
    });
  };

  handle.addEventListener('pointerdown', startDrag);
  window.addEventListener('pointermove', handleMove);
  window.addEventListener('pointerup', stopDrag);
  window.addEventListener('pointercancel', stopDrag);
  window.addEventListener('blur', stopDrag);
  window.addEventListener('resize', handleResize);
  syncLayout();

  state.centerResizeCleanup = () => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    window.removeEventListener('pointermove', handleMove);
    window.removeEventListener('pointerup', stopDrag);
    window.removeEventListener('pointercancel', stopDrag);
    window.removeEventListener('blur', stopDrag);
    window.removeEventListener('resize', handleResize);
    document.body.classList.remove('is-outbound-center-resizing');
  };
}

function readCenterSplitRatio() {
  try {
    const value = Number(window.localStorage.getItem(OUTBOUND_CENTER_SPLIT_KEY));
    return Number.isFinite(value) && value > 0.25 && value < 0.78 ? value : null;
  } catch {
    return null;
  }
}

function writeCenterSplitRatio(value) {
  try {
    if (Number.isFinite(value)) window.localStorage.setItem(OUTBOUND_CENTER_SPLIT_KEY, String(value));
  } catch {
    // Ignore unavailable storage.
  }
}

function clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function cssEscape(value) {
  if (window.CSS?.escape) return window.CSS.escape(value);
  return String(value ?? '').replaceAll('"', '\\"').replaceAll('\\', '\\\\');
}

function slugifyFileName(value) {
  return String(value || 'outbound')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    || 'outbound';
}

function escapeXml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

async function updateContactInPipelineItem(pipelineItemId, contactIndex, mutator) {
  const collection = state.ctx?.db?.raw?.outbound_pipeline_items;
  if (!collection) return;
  const existing = await collection.findOne(pipelineItemId).exec();
  if (!existing) return;

  const contacts = Array.isArray(existing.contacts) ? JSON.parse(JSON.stringify(existing.contacts)) : [];
  if (contacts[contactIndex]) {
    mutator(contacts[contactIndex]);
    await existing.incrementalPatch({ contacts });
    await loadAll({ hydrateKnowledge: false });
    render();
  }
}

function getSelectedContactIndexForCompany(companyId, contacts) {
  if (!contacts || !contacts.length) return 0;
  if (!state.selectedContactByCompany) {
    state.selectedContactByCompany = new Map();
  }
  const saved = state.selectedContactByCompany.get(companyId);
  if (typeof saved === 'number' && saved >= 0 && saved < contacts.length) {
    return saved;
  }
  return 0;
}

function setSelectedContactIndexForCompany(companyId, index) {
  if (!state.selectedContactByCompany) {
    state.selectedContactByCompany = new Map();
  }
  state.selectedContactByCompany.set(companyId, index);
}

async function deleteContactFromPipelineItem(pipelineItemId, contactIndex) {
  const collection = state.ctx?.db?.raw?.outbound_pipeline_items;
  if (!collection) return;
  const existing = await collection.findOne(pipelineItemId).exec();
  if (!existing) return;

  const contacts = Array.isArray(existing.contacts) ? JSON.parse(JSON.stringify(existing.contacts)) : [];
  if (contactIndex >= 0 && contactIndex < contacts.length) {
    contacts.splice(contactIndex, 1);
    await existing.incrementalPatch({ contacts });

    if (state.selectedContactByCompany) {
      const saved = state.selectedContactByCompany.get(existing.company_id);
      if (typeof saved === 'number') {
        if (contacts.length === 0) {
          state.selectedContactByCompany.delete(existing.company_id);
        } else if (saved >= contacts.length) {
          state.selectedContactByCompany.set(existing.company_id, contacts.length - 1);
        }
      }
    }

    await loadAll({ hydrateKnowledge: false });
    render();
  }
}

function saveFocusState(parentEl) {
  const activeEl = document.activeElement;
  if (!activeEl || !parentEl.contains(activeEl)) return null;

  let selector = '';
  if (activeEl.hasAttribute('data-table-filter')) {
    selector = `[data-table-filter="${activeEl.getAttribute('data-table-filter')}"]`;
  } else if (activeEl.hasAttribute('data-campaign-edit-field')) {
    selector = `[data-campaign-edit-field="${activeEl.getAttribute('data-campaign-edit-field')}"]`;
  } else if (activeEl.classList.contains('inline-editor')) {
    const tr = activeEl.closest('tr');
    if (tr && tr.hasAttribute('data-contact-key')) {
      selector = `tr[data-contact-key="${tr.getAttribute('data-contact-key')}"] .inline-editor`;
    }
  }

  if (!selector) return null;

  return {
    selector,
    selectionStart: activeEl.selectionStart,
    selectionEnd: activeEl.selectionEnd,
  };
}

function restoreFocusState(parentEl, focusState) {
  if (!focusState) return;
  const newEl = parentEl.querySelector(focusState.selector);
  if (newEl) {
    newEl.focus();
    if (typeof focusState.selectionStart === 'number') {
      try {
        newEl.selectionStart = focusState.selectionStart;
        newEl.selectionEnd = focusState.selectionEnd;
      } catch (e) {
        // Safe-guard
      }
    }
  }
}

function chipsFromMultiSelect(val) {
  if (!val) return [];
  if (Array.isArray(val)) return val;
  if (typeof val === 'object' && Array.isArray(val.keys)) return val.keys;
  if (typeof val === 'string') return val.split(',').map(s => s.trim()).filter(Boolean);
  return [];
}

function extractUniqueStatuses(pipeline) {
  const set = new Set(['Offen', 'Recherchiert', 'Kontaktiert', 'Antwort erhalten', 'Nicht passend']);
  for (const item of pipeline) {
    if (Array.isArray(item.contacts)) {
      for (const contact of item.contacts) {
        const val = contact.status_field || contact.status;
        if (val) {
          chipsFromMultiSelect(val).forEach(k => set.add(k));
        }
      }
    }
  }
  return Array.from(set);
}

function extractUniqueTags(pipeline) {
  const set = new Set(['Entscheider', 'Influencer', 'Technisch', 'Marketing', 'Vertrieb']);
  for (const item of pipeline) {
    if (Array.isArray(item.contacts)) {
      for (const contact of item.contacts) {
        if (contact.tags) {
          chipsFromMultiSelect(contact.tags).forEach(k => set.add(k));
        }
      }
    }
  }
  return Array.from(set);
}

function startInlineEdit(hostEl, { initialText, multiline, onSave }) {
  if (hostEl.querySelector('.inline-editor')) return;
  const originalHtml = hostEl.innerHTML;

  const editor = document.createElement(multiline ? 'textarea' : 'input');
  editor.className = 'inline-editor';
  if (multiline) {
    editor.style.height = '100px';
    editor.style.resize = 'vertical';
  }
  editor.value = initialText || '';

  hostEl.innerHTML = '';
  hostEl.appendChild(editor);
  editor.focus();
  if (!multiline && editor.select) {
    editor.select();
  }

  let finished = false;
  async function finish(save) {
    if (finished) return;
    finished = true;
    const value = editor.value.trim();
    if (save) {
      hostEl.innerHTML = `<span style="opacity:0.5;">${escapeHtml(t('savingEllipsis', 'Speichern...'))}</span>`;
      try {
        await onSave(value);
      } catch (e) {
        console.error('Error saving inline edit:', e);
        hostEl.innerHTML = originalHtml;
      }
    } else {
      hostEl.innerHTML = originalHtml;
    }
  }

  editor.addEventListener('blur', () => {
    finish(true);
  });

  editor.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      if (!multiline || e.ctrlKey || e.metaKey) {
        e.preventDefault();
        finish(true);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      finish(false);
    }
  });
}

let activeOverlay = null;

function closeMultiSelectOverlay() {
  if (activeOverlay) {
    activeOverlay.remove();
    activeOverlay = null;
  }
}

function showMultiSelectOverlay(hostCell, { currentValues, allOptions, labelSingular, onSave }) {
  closeMultiSelectOverlay();

  const rect = hostCell.getBoundingClientRect();
  const overlay = document.createElement('div');
  overlay.className = 'multi-editor-overlay';

  overlay.style.top = `${window.scrollY + rect.bottom + 4}px`;
  overlay.style.left = `${window.scrollX + rect.left}px`;

  const selectedSet = new Set(currentValues);

  function renderList() {
    return allOptions.map(opt => {
      const checked = selectedSet.has(opt) ? 'checked' : '';
      return `
        <label style="display:flex; align-items:center; gap:6px; cursor:pointer;">
          <input type="checkbox" data-option="${escapeHtml(opt)}" ${checked} style="cursor:pointer;" />
          <span>${escapeHtml(opt)}</span>
        </label>
      `;
    }).join('');
  }

  overlay.innerHTML = `
    <div class="multi-editor-title" style="margin-bottom:8px; font-weight:800; font-size:11px; text-transform:uppercase;">${escapeHtml(t('manageEntities', '{0}s verwalten', labelSingular))}</div>
    <div class="multi-editor-list" style="display:flex; flex-direction:column; gap:4px; max-height:150px; overflow-y:auto; border:1px solid var(--outbound-line); padding:6px; background:var(--outbound-surface); border-radius:4px; margin-bottom:8px;">
      ${renderList()}
    </div>
    <div class="multi-editor-addrow" style="display:flex; gap:4px; align-items:center; margin-bottom:8px;">
      <input type="text" class="multi-editor-input" placeholder="${escapeHtml(t('newOptionPlaceholder', 'Neu...'))}" style="flex:1; font-size:11px; padding:3px 6px; border:1px solid var(--outbound-line); background:var(--outbound-surface); color:var(--outbound-text); border-radius:4px;" />
      <button type="button" class="mini-btn" data-action="add-option" style="padding:3px 8px;">+</button>
    </div>
    <div class="multi-editor-actions" style="display:flex; justify-content:flex-end; gap:6px;">
      <button type="button" class="btn-small" data-action="cancel-overlay" style="padding:4px 8px; font-size:10px; cursor:pointer;">${escapeHtml(t('cancel', 'Abbrechen'))}</button>
      <button type="button" class="btn-small btn-save" style="padding:4px 10px; font-size:10px; background:var(--outbound-accent); border-color:var(--outbound-accent); color:#000; font-weight:800; cursor:pointer;" data-action="save-overlay">${escapeHtml(t('save', 'Speichern'))}</button>
    </div>
  `;

  document.body.appendChild(overlay);
  activeOverlay = overlay;

  const addInput = overlay.querySelector('.multi-editor-input');
  const addBtn = overlay.querySelector('[data-action="add-option"]');
  const listContainer = overlay.querySelector('.multi-editor-list');

  function addOption() {
    const val = addInput.value.trim();
    if (val && !allOptions.includes(val)) {
      allOptions.push(val);
      selectedSet.add(val);
      listContainer.innerHTML = renderList();
      addInput.value = '';
    }
  }

  addBtn.addEventListener('click', addOption);
  addInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      addOption();
    }
  });

  listContainer.addEventListener('change', (e) => {
    const checkbox = e.target.closest('input[type="checkbox"]');
    if (checkbox) {
      const opt = checkbox.dataset.option;
      if (checkbox.checked) {
        selectedSet.add(opt);
      } else {
        selectedSet.delete(opt);
      }
    }
  });

  overlay.querySelector('[data-action="cancel-overlay"]').addEventListener('click', closeMultiSelectOverlay);

  overlay.querySelector('[data-action="save-overlay"]').addEventListener('click', async () => {
    const finalVals = Array.from(selectedSet);
    closeMultiSelectOverlay();
    await onSave(finalVals);
  });

  const clickOutsideHandler = (e) => {
    if (!overlay.contains(e.target) && !hostCell.contains(e.target)) {
      closeMultiSelectOverlay();
      document.removeEventListener('click', clickOutsideHandler);
    }
  };
  setTimeout(() => {
    document.addEventListener('click', clickOutsideHandler);
  }, 50);
}

function trim(str) {
  return typeof str === 'string' ? str.trim() : '';
}

function toggleResearchSettingsTab(tabName) {
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  if (!drawer) return;
  saveResearchSettingsTemporaryState(drawer);
  state.researchSettingsActiveTab = tabName;
  drawer.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
  if (tabName === 'mailserver') {
    loadMailserverConfig();
  }
}

async function loadMailserverConfig() {
  if (!state.ctx?.commandBus?.dispatch) return;
  state.mailserver = state.mailserver || { domains: [], users: [], loading: false, error: null };
  state.mailserver.loading = true;
  state.mailserver.error = null;
  
  // Render loading state
  const drawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
  if (drawer && state.researchSettingsActiveTab === 'mailserver') {
    drawer.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
  }

  try {
    const commandId = `cmd_mailserver_get_config_${crypto.randomUUID()}`;
    const startedAtMs = Date.now();
    
    const dispatched = await state.ctx.commandBus.dispatch({
      id: commandId,
      module: 'ctox',
      type: 'ctox.mailserver.get_config',
      record_id: commandId,
      inbound_channel: 'business_os.outbound',
      payload: {},
      client_context: {
        source_module: 'outbound',
      },
    });

    let result = dispatched;
    if (!dispatched?.status || dispatched.status === 'pending_sync') {
      result = await waitForBusinessCommandProjection(commandId, startedAtMs);
    }

    if (result && (result.status === 'completed' || result.result)) {
      const outcome = result.result || result.payload?.outcome || result;
      state.mailserver.domains = outcome.domains || [];
      state.mailserver.users = outcome.users || [];
    } else {
      throw new Error('Fehler beim Laden der Mailserver-Konfiguration.');
    }
  } catch (err) {
    console.error('[outbound] Failed to load mailserver config', err);
    state.mailserver.error = err.message || String(err);
  } finally {
    state.mailserver.loading = false;
    // Re-render if tab is still active
    const currentDrawer = state.ctx?.host?.querySelector('.outbound-research-drawer');
    if (currentDrawer && state.researchSettingsActiveTab === 'mailserver') {
      currentDrawer.innerHTML = renderResearchSettingsDrawer(selectedCampaign());
    }
  }
}

function saveResearchSettingsTemporaryState(drawer) {
  if (!drawer || !state.tempResearchSettings) return;

  const customInstructionTextarea = drawer.querySelector('[data-research-setting-custom]');
  if (customInstructionTextarea) {
    state.tempResearchSettings.customInstruction = customInstructionTextarea.value.trim();
  }

  const fieldsChecked = Array.from(drawer.querySelectorAll('[data-column-setting-kind="company"][data-column-setting-full]:checked'))
    .map((input) => input.dataset.columnSettingFull)
    .filter(Boolean);
  if (fieldsChecked.length || drawer.querySelector('[data-column-setting-kind="company"]')) {
    state.tempResearchSettings.fields = fieldsChecked;
  }

  const fieldsCompactChecked = Array.from(drawer.querySelectorAll('[data-column-setting-kind="company"][data-column-setting-compact]:checked'))
    .map((input) => input.dataset.columnSettingCompact)
    .filter(Boolean);
  if (fieldsCompactChecked.length || drawer.querySelector('[data-column-setting-kind="company"]')) {
    state.tempResearchSettings.fieldsCompact = fieldsCompactChecked;
  }

  const contactFieldsChecked = Array.from(drawer.querySelectorAll('[data-column-setting-kind="contact"][data-column-setting-full]:checked'))
    .map((input) => input.dataset.columnSettingFull)
    .filter(Boolean);
  if (contactFieldsChecked.length || drawer.querySelector('[data-column-setting-kind="contact"]')) {
    state.tempResearchSettings.contactFields = contactFieldsChecked;
  }

  const contactFieldsCompactChecked = Array.from(drawer.querySelectorAll('[data-column-setting-kind="contact"][data-column-setting-compact]:checked'))
    .map((input) => input.dataset.columnSettingCompact)
    .filter(Boolean);
  if (contactFieldsCompactChecked.length || drawer.querySelector('[data-column-setting-kind="contact"]')) {
    state.tempResearchSettings.contactFieldsCompact = contactFieldsCompactChecked;
  }

  const companyRows = Array.from(drawer.querySelectorAll('[data-column-setting-side="company"][data-column-setting-id]'));
  if (companyRows.length) {
    state.tempResearchSettings.customFields = companyRows
      .filter((node) => node.dataset.customFieldId)
      .map((node) => ({
        id: node.dataset.columnSettingId,
        label: node.querySelector('[data-column-setting-label]')?.value?.trim() || '',
        prompt: node.querySelector('[data-column-setting-prompt]')?.value?.trim() || '',
      }))
      .filter((field) => field.id && field.label);
  }

  const contactRows = Array.from(drawer.querySelectorAll('[data-column-setting-side="contact"][data-column-setting-id]'));
  if (contactRows.length) {
    state.tempResearchSettings.customContactFields = contactRows
      .filter((node) => node.dataset.customFieldId)
      .map((node) => ({
        id: node.dataset.columnSettingId,
        label: node.querySelector('[data-column-setting-label]')?.value?.trim() || '',
        prompt: node.querySelector('[data-column-setting-prompt]')?.value?.trim() || '',
      }))
      .filter((field) => field.id && field.label);
  }

  if (companyRows.length || contactRows.length) {
    if (!state.tempResearchSettings.columnLabels) state.tempResearchSettings.columnLabels = {};
    if (!state.tempResearchSettings.fieldPrompts) state.tempResearchSettings.fieldPrompts = {};
    [...companyRows, ...contactRows].forEach((node) => {
      const id = node.dataset.columnSettingId;
      const label = node.querySelector('[data-column-setting-label]')?.value?.trim() || '';
      const prompt = node.querySelector('[data-column-setting-prompt]')?.value?.trim() || '';
      if (id && label) state.tempResearchSettings.columnLabels[id] = label;
      if (id && prompt) state.tempResearchSettings.fieldPrompts[id] = prompt;
    });
  }

  const apiUrlInput = drawer.querySelector('[data-prompt-setting="apiUrl"]');
  if (apiUrlInput) state.tempResearchSettings.apiUrl = apiUrlInput.value.trim();

  const apiTokenInput = drawer.querySelector('[data-prompt-setting="apiToken"]');
  if (apiTokenInput) state.tempResearchSettings.apiToken = apiTokenInput.value.trim();

  const icpCoreTextarea = drawer.querySelector('[data-prompt-setting="icpCore"]');
  if (icpCoreTextarea) state.tempResearchSettings.icpCore = icpCoreTextarea.value.trim();

  const checklistTextarea = drawer.querySelector('[data-prompt-setting="checklist"]');
  if (checklistTextarea) state.tempResearchSettings.checklist = checklistTextarea.value.trim();

  const ctaInput = drawer.querySelector('[data-prompt-setting="cta"]');
  if (ctaInput) state.tempResearchSettings.cta = ctaInput.value.trim();

  const signatureTextarea = drawer.querySelector('[data-prompt-setting="signature"]');
  if (signatureTextarea) state.tempResearchSettings.signature = signatureTextarea.value.trim();

  const icpPromptTemplateTextarea = drawer.querySelector('[data-prompt-setting="icpPromptTemplate"]');
  if (icpPromptTemplateTextarea) state.tempResearchSettings.icpPromptTemplate = icpPromptTemplateTextarea.value.trim();

  const messagePromptTemplateTextarea = drawer.querySelector('[data-prompt-setting="messagePromptTemplate"]');
  if (messagePromptTemplateTextarea) state.tempResearchSettings.messagePromptTemplate = messagePromptTemplateTextarea.value.trim();

  const extractEmailPromptTextarea = drawer.querySelector('[data-prompt-setting="extractEmailPrompt"]');
  if (extractEmailPromptTextarea) state.tempResearchSettings.extractEmailPrompt = extractEmailPromptTextarea.value.trim();
}

function normalizeDraftResult(messageObj) {
  if (!messageObj) return {};
  const result = {};
  const findVal = (keys) => {
    for (const key of keys) {
      const found = Object.keys(messageObj).find(k => k.toLowerCase() === key.toLowerCase());
      if (found) return messageObj[found];
    }
    return '';
  };

  result.message_mail_subject = findVal(['Betreff', 'subject']);
  result.message_mail_body = findVal(['Text', 'body', 'mail_body', 'email_body', 'email']);
  result.message_followup_1 = findVal(['FollowUp1', 'followup_1', 'follow_up_1', 'followup1']);
  result.message_followup_2 = findVal(['FollowUp2', 'followup_2', 'follow_up_2', 'followup2']);

  return result;
}

async function generateOutreachForContact(itemId, contactIndex) {
  const contactKey = `${itemId}_${contactIndex}`;
  if (state.generatingOutreach.has(contactKey)) return;

  const item = state.pipeline.find(entry => entry.id === itemId);
  if (!item) return;
  const company = state.companies.find(c => c.id === item.company_id);
  if (!company) return;
  const contact = item.contacts?.[contactIndex];
  if (!contact) return;

  const campaign = selectedCampaign();
  const settings = getCampaignResearchSettings(campaign);
  const gatewayUrl = settings.apiUrl || DEFAULT_API_URL;
  const gatewayToken = settings.apiToken || DEFAULT_TOKEN_ID;

  state.generatingOutreach.set(contactKey, { statusText: 'Verbindung wird aufgebaut... (0%)' });
  renderCenter();

  const researchResults = company.company_data || company.payload?.research_results || {};
  const summaryParts = [];
  summaryParts.push(`Unternehmen: ${company.name}`);
  if (company.website) summaryParts.push(`Website: ${company.website}`);
  if (company.domain) summaryParts.push(`Domain: ${company.domain}`);

  Object.entries(researchResults).forEach(([key, val]) => {
    if (val && typeof val !== 'object') {
      summaryParts.push(`${key}: ${val}`);
    }
  });
  const homepage_summary = summaryParts.join('\n');

  let first_name = contact.firstname || '';
  let last_name = contact.lastname || '';
  if (!first_name && !last_name && contact.name) {
    const parts = contact.name.trim().split(/\s+/);
    if (parts.length > 1) {
      first_name = parts[0];
      last_name = parts.slice(1).join(' ');
    } else {
      first_name = parts[0] || '';
    }
  }

  const person = {
    first_name,
    last_name,
    title: contact.role || contact.title || contact.position || '',
    email: contact.email || '',
    phone: contact.phone || '',
    linkedin_url: contact.linkedin_url || contact.linkedin || ''
  };

  const itemPayload = {
    company: {
      website_url: company.website || (company.domain ? `https://${company.domain}` : ''),
      homepage_summary
    },
    person
  };

  const payload = {
    Produkt_und_Dienstleistungsbeschreibung: settings.icpCore,
    CTA: settings.cta,
    Signatur: settings.signature,
    Checkliste_Landingpage: settings.checklist,
    message_prompt_template: settings.messagePromptTemplate,
    extract_prompt_template: settings.extractEmailPrompt,
    items: [itemPayload]
  };

  const headers = {
    'Content-Type': 'application/json',
    'X-Token-Id': gatewayToken,
    'X-Auth-Token': gatewayToken
  };

  try {
    const url = `${gatewayUrl}/email/generate?async=1`;
    const response = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(payload)
    });

    if (!response.ok) {
      throw new Error(`HTTP-Fehler ${response.status}: ${response.statusText}`);
    }

    const resData = await response.json();

    if (response.status === 200 && resData.results && resData.results[0]) {
      const messageObj = resData.results[0].message;
      const normalized = normalizeDraftResult(messageObj);
      await updateContactInPipelineItem(itemId, contactIndex, (c) => {
        if (!c.messages) c.messages = {};
        c.messages.message_mail_subject = normalized.message_mail_subject || '';
        c.messages.message_mail_body = normalized.message_mail_body || '';
        c.messages.message_followup_1 = normalized.message_followup_1 || '';
        c.messages.message_followup_2 = normalized.message_followup_2 || '';
      });
      state.generatingOutreach.delete(contactKey);
      renderCenter();
      return;
    }

    const jobId = resData.job_id;
    if (!jobId) {
      throw new Error('Keine Job-ID vom Server zurückgegeben.');
    }

    const pollInterval = 2000;
    const poll = async () => {
      if (!state.generatingOutreach.has(contactKey)) return;

      try {
        const pollResponse = await fetch(`${gatewayUrl}/email/generate/status/${jobId}`, {
          method: 'GET',
          headers
        });

        if (!pollResponse.ok) {
          throw new Error(`Polling HTTP-Fehler ${pollResponse.status}`);
        }

        const pollData = await pollResponse.json();

        if (!pollData.ok) {
          throw new Error(pollData.error || 'Fehler beim Polling.');
        }

        const status = pollData.status;
        const progress = pollData.progress || 0;
        const note = pollData.note || 'Warte auf Verarbeitung...';

        if (status === 'queued' || status === 'running') {
          state.generatingOutreach.set(contactKey, {
            statusText: `${note} (${progress}%)`
          });
          renderCenter();
          setTimeout(poll, pollInterval);
        } else if (status === 'done') {
          const result = pollData.result || {};
          if (result.results && result.results[0]) {
            const messageObj = result.results[0].message;
            const normalized = normalizeDraftResult(messageObj);

            await updateContactInPipelineItem(itemId, contactIndex, (c) => {
              if (!c.messages) c.messages = {};
              c.messages.message_mail_subject = normalized.message_mail_subject || '';
              c.messages.message_mail_body = normalized.message_mail_body || '';
              c.messages.message_followup_1 = normalized.message_followup_1 || '';
              c.messages.message_followup_2 = normalized.message_followup_2 || '';
            });

            state.generatingOutreach.delete(contactKey);
            renderCenter();
          } else {
            throw new Error('Keine Ergebnisse in der Serverantwort vorhanden.');
          }
        } else {
          throw new Error(pollData.error || 'Serverfehler bei der Generierung.');
        }
      } catch (err) {
        console.error('[outbound] error during outreach generation polling:', err);
        state.generatingOutreach.delete(contactKey);
        renderCenter();
        showBusinessAlert(`Fehler bei der Outreach-Generierung: ${err.message}`);
      }
    };

    setTimeout(poll, pollInterval);

  } catch (err) {
    console.error('[outbound] error starting outreach generation:', err);
    state.generatingOutreach.delete(contactKey);
    renderCenter();
    showBusinessAlert(`Verbindungsfehler: ${err.message}`);
  }
}

let hiddenCompaniesPanelOpen = false;

export function toggleHiddenCompaniesPanel() {
  if (hiddenCompaniesPanelOpen) {
    closeHiddenCompaniesPanel();
  } else {
    buildHiddenCompaniesPanel();
  }
}

export function closeHiddenCompaniesPanel() {
  const panel = document.querySelector('.hidden-company-panel');
  if (panel) {
    panel.remove();
  }
  hiddenCompaniesPanelOpen = false;
}

export function buildHiddenCompaniesPanel() {
  closeHiddenCompaniesPanel();

  const panel = document.createElement('div');
  panel.className = 'hidden-company-panel';

  const header = document.createElement('div');
  header.className = 'hidden-company-header';

  const title = document.createElement('div');
  title.style.fontWeight = '800';
  title.textContent = t('hiddenCompanies', 'Versteckte Firmen');

  const btnClose = document.createElement('button');
  btnClose.type = 'button';
  btnClose.className = 'mini-btn';
  btnClose.style.padding = '4px 8px';
  btnClose.textContent = t('close', 'Schließen');
  btnClose.addEventListener('click', closeHiddenCompaniesPanel);

  header.appendChild(title);
  header.appendChild(btnClose);
  panel.appendChild(header);

  const listWrap = document.createElement('div');
  listWrap.className = 'hidden-company-list';
  listWrap.style.marginTop = '8px';
  panel.appendChild(listWrap);

  const hiddenIds = loadHiddenCompanies();

  if (hiddenIds.length === 0) {
    const emptyRow = document.createElement('div');
    emptyRow.textContent = t('noCompaniesHidden', 'Keine Firma ausgeblendet.');
    emptyRow.style.fontStyle = 'italic';
    emptyRow.style.color = 'var(--outbound-muted)';
    listWrap.appendChild(emptyRow);
  } else {
    hiddenIds.forEach(companyId => {
      const company = state.companies.find(c => c.id === companyId);
      const companyName = company ? company.name : companyId;

      const row = document.createElement('div');
      row.className = 'hidden-company-row';
      row.style.display = 'flex';
      row.style.justifyContent = 'space-between';
      row.style.alignItems = 'center';
      row.style.gap = '8px';
      row.style.marginBottom = '6px';

      const nameDiv = document.createElement('div');
      nameDiv.className = 'hidden-company-name';
      nameDiv.style.fontSize = '11px';
      nameDiv.textContent = companyName;

      const restoreBtn = document.createElement('button');
      restoreBtn.type = 'button';
      restoreBtn.className = 'mini-btn';
      restoreBtn.style.padding = '2px 6px';
      restoreBtn.textContent = t('restoreCompany', 'Wieder anzeigen');
      restoreBtn.addEventListener('click', () => {
        unhideCompany(companyId);
        render(true);
        buildHiddenCompaniesPanel(); // re-render panel list
      });

      row.appendChild(nameDiv);
      row.appendChild(restoreBtn);
      listWrap.appendChild(row);
    });

    const bulkRow = document.createElement('div');
    bulkRow.style.marginTop = '12px';
    bulkRow.style.display = 'flex';
    bulkRow.style.justifyContent = 'flex-end';

    const bulkBtn = document.createElement('button');
    bulkBtn.type = 'button';
    bulkBtn.className = 'outbound-button';
    bulkBtn.style.fontSize = '10px';
    bulkBtn.style.padding = '4px 8px';
    bulkBtn.textContent = t('restoreAllCompanies', 'Alle wieder anzeigen');
    bulkBtn.addEventListener('click', () => {
      saveHiddenCompanies([]);
      render(true);
      closeHiddenCompaniesPanel();
    });
    bulkRow.appendChild(bulkBtn);
    panel.appendChild(bulkRow);
  }

  document.body.appendChild(panel);
  hiddenCompaniesPanelOpen = true;
}

export const __outboundTestHooks = {
  campaignScopedRows,
  validateOutboundImportPayload,
};
