import {
  extractCompanyRowsFromWorkbookFile,
  extractCompanyRowsFromText,
  normalizeCompanyRow,
  openUniversalImporter,
  parseDelimitedText,
} from '../../shared/universal-importer.js?v=20261006-import-preview-groups-v1';
import { extractImportRows, finalizeImportAnalysis, buildImportPreviewExtras } from './import-preview-groups.js';
import {
  showBusinessAlert as shellAlert,
  showBusinessConfirm as shellConfirm,
  showBusinessPrompt as shellPrompt,
} from '../../shared/dialogs.js';

// Rueckfragen und Hinweise IMMER im eigenen Fenster. Die Shell merkt sich das
// Dialogziel global; nach einem Wechsel zu einer anderen App (z. B.
// "Zugaenge") und zurueck landete jeder Dialog der Outbound-App unsichtbar im
// Fenster darunter und wartete dort auf eine Antwort — die App wirkte
// blockiert (Klicktest P0e, Owner-Befund 11.09.2026 abends).
function eigenesDialogZiel() {
  const host = globalThis.__olgDialogZiel;
  return host && host.isConnected ? host : undefined;
}
function showBusinessAlert(message, options = {}) { return shellAlert(message, { host: eigenesDialogZiel(), ...options }); }
function showBusinessConfirm(message, options = {}) { return shellConfirm(message, { host: eigenesDialogZiel(), ...options }); }
function showBusinessPrompt(message, options = {}) { return shellPrompt(message, { host: eigenesDialogZiel(), ...options }); }
import { loadModuleMessages } from '../../shared/i18n.js';
import { createCollectionReloader } from './collection-reloader.mjs';
import { loadLeadList, loadFullLeadRows, leadListRow, withLeadQueryAuthority } from './lead-list-loader.mjs';
import { captureResearchExport, openResearchSnapshot } from './current-state-export.mjs';
import { optionalKeysForRequiredCheckbox } from './required-field-selection.mjs';

// Owner-Rechercheanweisung (Schritt 1-3) und Belegregel 5: Felder, die zwei
// unabhaengige Quellen brauchen, waren nur EINER Quelle zugeordnet (wz_code nur
// D&B, firma_land fuer DE gar keiner) und blieben deshalb immer offen. Die
// Zuordnung folgt jetzt woertlich den Anweisungen: Register DE/AT/CH je Land,
// Adresse Impressum > FirmenABC/Zefix/D&B > Northdata, WZ D&B/Leadfeeder (DE
// zusaetzlich Bundesanzeiger), Fax ueber Google, Steckbrief aus der Homepage
// (25.09.2026). Umsatz/Mitarbeiter unveraendert.
const SOURCE_DEFS = Object.freeze([
  source('handelsregister.de', 'Handelsregister', 'https://www.handelsregister.de/', ['DE'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftstaetigkeit', 'firma_geschaeftsfuehrung', 'firma_prokura', 'person_vorname', 'person_nachname']),
  source('bundesanzeiger.de', 'Bundesanzeiger', 'https://www.bundesanzeiger.de/', ['DE'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_ort', 'firma_geschaeftstaetigkeit', 'firma_geschaeftsfuehrung', 'firma_prokura', 'wz_code', 'umsatz', 'mitarbeiter']),
  source('northdata.de', 'Northdata', 'https://www.northdata.de/', ['DE', 'AT', 'CH'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftsfuehrung', 'firma_prokura', 'umsatz', 'mitarbeiter', 'person_vorname', 'person_nachname', 'person_position']),
  // Owner 23.09.2026: CompanyHouse nur ueber die offizielle GraphQL-API
  // (Zugangstoken aus der Premium-Mitgliedschaft), kein Scraping. Bis ein
  // Token hinterlegt ist, zeigt die Zeile "Zugang fehlt" statt "defekt".
  source('companyhouse.de', 'CompanyHouse', 'https://www.companyhouse.de/', ['DE'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_geschaeftsfuehrung', 'firma_prokura', 'person_titel'], 'COMPANYHOUSE_API_TOKEN'),
  source('dnbhoovers.com', 'D&B Hoovers', 'https://app.dnbhoovers.com/', ['DE', 'AT', 'CH'], ['firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftstaetigkeit', 'wz_code', 'umsatz', 'mitarbeiter'], 'DNB_HOOVERS_BROWSER_LOGIN'),
  source('leadfeeder.com', 'Leadfeeder', 'https://app.leadfeeder.com/f/200538/dashboard?organization_id=wmNE2MdyDk&full_view=false&tab=overview', ['DE', 'AT', 'CH'], ['firma_name', 'firma_domain', 'firma_homepage_fact_sheet', 'firma_geschaeftstaetigkeit', 'wz_code'], 'LEADFEEDER_BROWSER_LOGIN'),
  // Owner 18.09.2026: LinkedIn wird ueber den Bright Data "LinkedIn People
  // Scraper" gelesen (Scrape-Ziel linkedin-com laeuft im API-Modus). Der
  // Zugang ist deshalb der Bright-Data-API-Schluessel, kein LinkedIn-Login.
  // Kann liefern, nicht muss: den akademischen Titel nur, wenn das Profil ihn
  // ausdruecklich nennt (Dr./Prof.), ein Geschlecht nie. Der Adaptertest
  // verlangt seit 22.09.2026 nur mindestens ein erklaertes Feld.
  source('linkedin.com', 'LinkedIn (Bright Data People Scraper)', 'https://www.linkedin.com/', ['DE', 'AT', 'CH'], ['person_geschlecht', 'person_titel', 'person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_linkedin'], 'BRIGHTDATA_API_KEY'),
  source('xing.com', 'XING', 'https://www.xing.com/', ['DE', 'AT', 'CH'], ['person_geschlecht', 'person_titel', 'person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_xing'], 'XING_BROWSER_LOGIN'),
  // Owner 18.09.2026: Die Google-Suche laeuft ueber ein eigenes Google-Konto.
  // Damit bekommt die Quelle einen Zugangsplatz; der Wert liegt im Secret Store.
  source('google.de', 'Google', 'https://www.google.de/', ['DE', 'AT', 'CH'], ['firma_name', 'firma_plz', 'firma_ort', 'firma_email', 'firma_domain', 'firma_telefon', 'firma_fax', 'firma_geschaeftstaetigkeit', 'firma_homepage_fact_sheet'], 'GOOGLE_BROWSER_LOGIN'),
  source('maps.google.com', 'Google Maps', 'https://www.google.de/maps', ['DE', 'AT', 'CH'], ['firma_name', 'firma_anschrift', 'firma_besucheranschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_domain', 'firma_telefon']),
  source('impressum', 'Unternehmenswebsite / Impressum', '', ['DE', 'AT', 'CH'], ['firma_anschrift', 'firma_besucheranschrift', 'firma_postanschrift', 'firma_postfach', 'firma_plz', 'firma_ort', 'firma_land', 'firma_email', 'firma_domain', 'firma_telefon', 'firma_fax', 'firma_homepage_fact_sheet', 'person_titel', 'person_vorname', 'person_nachname', 'person_funktion'], '', {
    inputDriven: true,
    startUrlSource: 'Unternehmensdomain des jeweils recherchierten Leads',
  }),
  // RocketReach zeigt Kontaktdaten nur angemeldet; der Adapter meldete ohne
  // Sitzung session_expired_login_landing (Messung 22.09.2026). Ohne Zugangs-
  // verweis gab es in der App keinen Ort, den Login zu hinterlegen.
  source('rocketreach.com', 'RocketReach', 'https://rocketreach.co/', ['DE', 'AT', 'CH'], ['person_titel', 'person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_email', 'person_telefon'], 'ROCKETREACH_BROWSER_LOGIN'),
  source('firmenabc.at', 'FirmenABC', 'https://www.firmenabc.at/', ['AT'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_email', 'firma_domain', 'firma_telefon', 'firma_geschaeftstaetigkeit', 'firma_geschaeftsfuehrung', 'firma_prokura']),
  source('moneyhouse.ch', 'Moneyhouse', 'https://www.moneyhouse.ch/', ['CH'], ['firma_name', 'firma_fruehere_namen', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_aktivitaetsstatus', 'firma_geschaeftsfuehrung', 'firma_prokura', 'person_vorname', 'person_nachname', 'person_position']),
  source('zefix.ch', 'Zefix', 'https://www.zefix.ch/', ['CH'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftstaetigkeit', 'firma_geschaeftsfuehrung', 'firma_prokura']),
  source('experte.de', 'E-Mail-Prüfung', 'https://www.experte.de/email-pruefen', ['DE', 'AT', 'CH'], ['person_email_validation']),
  source('mailtester.com', 'MailTester', 'https://mailtester.com/', ['DE', 'AT', 'CH'], ['person_email_validation']),
  source('evi.gv.at', 'EVI – Amtsblatt Österreich', 'https://www.evi.gv.at/', ['AT'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftstaetigkeit']),
  source('justizonline.gv.at', 'Firmenbuch / JustizOnline', 'https://justizonline.gv.at/jop/web/firmenbuchabfrage', ['AT'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftstaetigkeit', 'firma_geschaeftsfuehrung', 'firma_prokura']),
  source('shab.ch', 'SHAB – Schweizerisches Handelsamtsblatt', 'https://www.shab.ch/#!/gazette', ['CH'], ['firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_plz', 'firma_ort', 'firma_land', 'firma_geschaeftstaetigkeit', 'firma_geschaeftsfuehrung', 'firma_prokura']),
]);
// Sellify ist keine Adapter-Quelle: keine Anmeldung, kein Scrape-Target.
// Sie erscheint in der Quellenliste als interne Quelle und als Beleg an
// jedem Feld, das aus dem eigenen CRM uebernommen wurde.
const SELLIFY_SOURCE_ID = 'sellify';
const SELLIFY_SOURCE_LABEL = 'Sellify (eigenes CRM)';
// Owner-Vorgabe 31.08.: mindestens eine Person je Kategorie, in dieser
// Reihenfolge (Prompt-Punkt 4).
const PERSON_RESEARCH_PRIORITIES = Object.freeze([
  'Geschäftsführung',
  'Prokura',
  'Finanzen',
  'Einkauf',
  'Supply Chain',
  'Operations',
  'Technik',
  'Entwicklung',
]);

const RESEARCH_FIELDS = Object.freeze([
  'firma_name',
  'firma_fruehere_namen',
  'firma_aktivitaetsstatus',
  'firma_anschrift',
  'firma_besucheranschrift',
  'firma_postanschrift',
  'firma_postfach',
  'firma_plz',
  'firma_ort',
  'firma_land',
  'firma_email',
  'firma_domain',
  'firma_telefon',
  'firma_fax',
  'firma_geschaeftstaetigkeit',
  'firma_homepage_fact_sheet',
  'firma_geschaeftsfuehrung',
  'firma_prokura',
  'wz_code',
  'umsatz',
  'mitarbeiter',
  'person_geschlecht',
  'person_titel',
  'person_vorname',
  'person_nachname',
  'person_funktion',
  'person_position',
  'person_email',
  'person_email_validation',
  'person_telefon',
  'person_linkedin',
  'person_xing',
]);
const RESEARCH_FIELD_SET = new Set(RESEARCH_FIELDS);

// Fachliche Felder aus der verbindlichen Nachbesprechung. Sie werden im
// Lead-Dokument unter `data` dauerhaft gespeichert und in derselben
// Review-Oberflaeche wie die maschinell recherchierten Felder angezeigt.
// `RESEARCH_FIELDS` enthält die extern recherchierbaren Fakten. Die übrigen
// Werte sind menschliche Entscheidungen, Datenpflege oder aus belegten Fakten
// abgeleitete Prüfmerkmale und werden nicht als künstliche Quellenwerte an den
// Web-Stack geschickt.
const GOVERNANCE_FIELDS = Object.freeze([
  'firma_fruehere_namen',
  'firma_aktivitaetsstatus',
  'firma_aufnahmeeignung',
  'firma_ausschlussgrund',
  'firma_land',
  'firma_besucheranschrift',
  'firma_postanschrift',
  'firma_postfach',
  'firma_fax',
  'firma_geschaeftstaetigkeit',
  'firma_homepage_fact_sheet',
  'firma_umsatz_schaetzung',
  'firma_mitarbeiter_schaetzung',
  'firma_geschaeftsfuehrung',
  'firma_prokura',
  'firma_email_domain_konflikt',
  'herkunft_import',
  'adressquelle',
  'verantwortlicher',
  'listenstatus',
  'aenderungsart',
  'bearbeiter_initialen',
  'sellify_nummer',
  'statistische_kampagne',
  'fachliche_aufnahmeentscheidung',
  'fachliche_entscheidung_begruendung',
]);
// Felder, die die Feldansicht mit "ändern"/"eintragen" anbietet, die der
// Editor aber nicht hatte: der Klick oeffnete einen Dialog ohne das Feld.
const EXTRA_COMPANY_EDIT_FIELDS = Object.freeze(['wz_code', 'umsatz', 'mitarbeiter']);
const PERSON_EDIT_FIELDS = Object.freeze([
  'person_geschlecht', 'person_titel', 'person_vorname', 'person_nachname', 'person_funktion',
  'person_position', 'person_email', 'person_telefon', 'person_linkedin', 'person_xing',
]);
const EDITOR_KEY_FOR_FIELD = Object.freeze({
  firma_name: 'name', firma_domain: 'website', firma_anschrift: 'address_line', firma_plz: 'postal_code',
  firma_ort: 'city', firma_land: 'country', firma_email: 'email', firma_telefon: 'phone',
});
const NON_EVIDENCE_REVIEW_FIELDS = new Set([
  'firma_umsatz_schaetzung',
  'firma_mitarbeiter_schaetzung',
  'firma_aufnahmeeignung',
  'firma_ausschlussgrund',
  'firma_email_domain_konflikt',
]);

// Selbstauskuenfte: Angaben, fuer die das Unternehmen selbst die Urkunde ist.
// Eine Telefonzentrale, eine Info-Adresse oder das XING-Profil eines
// Mitarbeiters stehen nirgends ein zweites Mal unabhaengig im Netz. Die
// Zwei-Quellen-Regel macht sie deshalb nicht sicherer, sondern nur
// unerreichbar: am 09.09.2026 landeten bei Zschimmer & Schwarz 20 von 32
// Feldern auf no_match mit der Begruendung "nur auf eigener Website
// dokumentiert". Ein Beleg von der Unternehmensseite bzw. dem Profil genuegt
// hier; er braucht weiterhin URL und woertliches Zitat, und die Belegampel
// zeigt eine einzelne Quelle orange.
const SELF_REPORTED_FIELDS = new Set([
  'firma_domain',
  'firma_email',
  'firma_telefon',
  'firma_fax',
  'firma_postfach',
  'firma_besucheranschrift',
  'firma_postanschrift',
  'firma_homepage_fact_sheet',
  'person_geschlecht',
  'person_titel',
  'person_vorname',
  'person_nachname',
  'person_funktion',
  'person_position',
  'person_email',
  'person_email_validation',
  'person_telefon',
  'person_linkedin',
  'person_xing',
]);
// Owner 23.09.2026: fuer ALLE Felder genuegt eine passende belegte Quelle.
// Die Zahl unabhaengiger Quellen zeigt nur noch, wie stark ein Wert getragen
// ist (Belegampel), und sperrt keine Freigabe mehr.
function requiredIndependentSources(key) {
  return String(key || '').trim() ? 1 : 1;
}

const RESEARCH_FIELD_GROUPS = Object.freeze([
  {
    id: 'company',
    label: 'Unternehmen',
    fields: Object.freeze([
      ['firma_name', 'Firmenname'],
      ['firma_anschrift', 'Anschrift'],
      ['firma_plz', 'PLZ'],
      ['firma_ort', 'Ort'],
      ['firma_email', 'E-Mail'],
      ['firma_domain', 'Domain'],
      ['firma_telefon', 'Telefon'],
      ['firma_fax', 'Fax'],
      ['firma_fruehere_namen', 'Frühere Namen / Namensvarianten'],
      ['firma_aktivitaetsstatus', 'Aktivitätsstatus'],
      ['firma_land', 'Länderkennzeichen'],
      ['firma_besucheranschrift', 'Besucheradresse'],
      ['firma_postanschrift', 'Postadresse'],
      ['firma_postfach', 'Postfach'],
      ['firma_geschaeftstaetigkeit', 'Geschäftstätigkeit'],
      ['firma_homepage_fact_sheet', 'Homepage-Fact-Sheet / URL'],
      ['firma_geschaeftsfuehrung', 'Geschäftsführung'],
      ['firma_prokura', 'Prokura'],
      ['firma_email_domain_konflikt', 'E-Mail-/Info-/Homepage-Domain'],
    ]),
  },
  {
    id: 'classification',
    label: 'Klassifikation',
    fields: Object.freeze([
      ['wz_code', 'WZ-Code'],
      ['umsatz', 'Umsatz'],
      ['mitarbeiter', 'Mitarbeiter'],
      ['firma_umsatz_schaetzung', 'Umsatz als Schätzung markiert'],
      ['firma_mitarbeiter_schaetzung', 'Mitarbeiter als Schätzung markiert'],
      ['firma_aufnahmeeignung', 'Fachliche Eignung'],
      ['firma_ausschlussgrund', 'Ausschlussliste / Ausschlussgrund'],
    ]),
  },
  {
    id: 'contact',
    label: 'Ansprechpartner',
    fields: Object.freeze([
      ['person_geschlecht', 'Geschlecht'],
      ['person_titel', 'Titel'],
      ['person_vorname', 'Vorname'],
      ['person_nachname', 'Nachname'],
      ['person_funktion', 'Funktion'],
      ['person_position', 'Position'],
      ['person_email', 'E-Mail'],
      ['person_email_validation', 'E-Mail-Prüfung'],
      ['person_telefon', 'Telefon'],
      ['person_linkedin', 'LinkedIn'],
      ['person_xing', 'XING'],
    ]),
  },
  {
    id: 'governance',
    label: 'Datenpflege & Entscheidung',
    fields: Object.freeze([
      ['herkunft_import', 'Importherkunft'],
      ['adressquelle', 'Adressquelle'],
      ['verantwortlicher', 'Verantwortlicher'],
      ['listenstatus', 'Listenstatus'],
      ['aenderungsart', 'Änderungsart'],
      ['bearbeiter_initialen', 'Bearbeiterinitialen'],
      ['sellify_nummer', 'Sellify-Nummer'],
      ['statistische_kampagne', 'Statistische Kampagne'],
      ['fachliche_aufnahmeentscheidung', 'Menschliche Aufnahmeentscheidung'],
      ['fachliche_entscheidung_begruendung', 'Entscheidungsbegründung'],
    ]),
    evidenceRequired: false,
  },
]);
const REVIEW_FIELD_LABELS = Object.freeze(Object.fromEntries(
  RESEARCH_FIELD_GROUPS.flatMap((group) => group.fields.map(([key, label]) => [key, label])),
));

function researchFieldLabel(key) {
  return REVIEW_FIELD_LABELS[String(key || '').trim()] || String(key || '').trim();
}

// Owner 23.09.2026: Nicht jedes Feld muss fuer die Sellify-Uebergabe
// vorliegen. Optionale Felder werden weiter recherchiert und, wenn belegt,
// uebertragen, blockieren die Freigabe aber nicht und zaehlen nicht als offen.
// Vorbelegt sind Felder, die bei Firmen praktisch nie oeffentlich sind
// (Postfach 40/40, persoenliches Telefon 38/40, XING 35/40 "nicht gefunden").
const DEFAULT_OPTIONAL_FIELDS = Object.freeze([
  'firma_postfach', 'firma_fax', 'firma_fruehere_namen',
  'person_telefon', 'person_xing', 'person_linkedin', 'person_titel',
]);
function optionalResearchFields() {
  const record = state.researchPolicyRecord;
  // Ein Nachladen mit noch nicht synchronisiertem Stand darf die gerade
  // gespeicherte Auswahl nicht wieder ueberdecken.
  const lokal = state.optionalFieldsSaved;
  if (lokal && !(Number(record?.updated_at_ms || 0) >= lokal.at && Array.isArray(record?.optional_field_keys))) {
    return new Set(lokal.keys);
  }
  const gespeichert = record?.optional_field_keys;
  return new Set(Array.isArray(gespeichert) ? gespeichert : DEFAULT_OPTIONAL_FIELDS);
}
function optionalFieldsDraft() {
  return state.optionalFieldsDraft instanceof Set ? state.optionalFieldsDraft : optionalResearchFields();
}
// Ein begruendetes "nicht gefunden" der Recherche ist eine Antwort, kein
// offener Punkt. Bisher zaehlte jedes leere Feld als offen, auch Postfach
// oder XING, die die Recherche belegt nicht gefunden hatte - daher die
// "ueber 10 offenen Felder" bei fast jedem Lead.
// Owner 27.09.2026: Felder ohne gefundene Information sollen sich je Lead
// freigeben lassen, solange die Kampagne trotzdem starten kann. Nicht leer
// freigebbar ist, was die Kampagne oder eine Owner-Regel zwingend braucht.
const NICHT_LEER_FREIGEBBAR = new Set([
  'firma_name',
  'umsatz',
  'mitarbeiter',
  'firma_aufnahmeeignung',
  'firma_ausschlussgrund',
  'firma_email_domain_konflikt',
  'fachliche_aufnahmeentscheidung',
]);
function leerFreigegeben(lead, key) {
  return (lead?.payload?.operator_released_empty_field_keys || []).includes(String(key || ''));
}
function leerFreigebbar(key) {
  return Boolean(key) && !NICHT_LEER_FREIGEBBAR.has(String(key));
}

function researchAnsweredNotFound(lead, key) {
  const status = lead?.field_status?.[key];
  return ['no_match', 'unsupported'].includes(String(status?.status || '').trim())
    && Boolean(String(status?.reason || '').trim());
}

const RESEARCH_FIELD_VALUE_KEYS = Object.freeze({
  firma_name: ['firma_name', 'company_name', 'firma'],
  firma_anschrift: ['firma_anschrift', 'address_line', 'address', 'street', 'strasse'],
  firma_plz: ['firma_plz', 'postal_code', 'postcode', 'plz'],
  firma_ort: ['firma_ort', 'city', 'ort'],
  firma_email: ['firma_email', 'email', 'company_email', 'e_mail'],
  firma_domain: ['firma_domain', 'domain', 'website', 'website_url', 'internet'],
  firma_telefon: ['firma_telefon', 'phone', 'company_phone', 'telefon'],
  firma_fax: ['firma_fax', 'fax'],
  firma_fruehere_namen: ['firma_fruehere_namen', 'former_names', 'aliases', 'namensvarianten'],
  firma_aktivitaetsstatus: ['firma_aktivitaetsstatus', 'activity_status', 'company_status'],
  firma_aufnahmeeignung: ['firma_aufnahmeeignung', 'eligibility', 'fit'],
  firma_ausschlussgrund: ['firma_ausschlussgrund', 'exclusion_reason'],
  firma_land: ['firma_land', 'country_code', 'country'],
  firma_besucheranschrift: ['firma_besucheranschrift', 'visitor_address'],
  firma_postanschrift: ['firma_postanschrift', 'postal_address'],
  firma_postfach: ['firma_postfach', 'post_box'],
  firma_geschaeftstaetigkeit: ['firma_geschaeftstaetigkeit', 'business_activity', 'activity'],
  firma_homepage_fact_sheet: ['firma_homepage_fact_sheet', 'homepage_fact_sheet', 'fact_sheet_url'],
  firma_umsatz_schaetzung: ['firma_umsatz_schaetzung', 'revenue_estimate'],
  firma_mitarbeiter_schaetzung: ['firma_mitarbeiter_schaetzung', 'employee_estimate'],
  firma_geschaeftsfuehrung: ['firma_geschaeftsfuehrung', 'managing_directors', 'geschaeftsfuehrer'],
  firma_prokura: ['firma_prokura', 'prokura', 'procuration'],
  firma_email_domain_konflikt: ['firma_email_domain_konflikt', 'email_domain_conflict'],
  wz_code: ['wz_code', 'wzcode'],
  umsatz: ['umsatz', 'revenue_mio', 'umsatz_mio'],
  mitarbeiter: ['mitarbeiter', 'employees'],
  person_geschlecht: ['person_geschlecht', 'geschlecht', 'gender'],
  person_titel: ['person_titel', 'titel'],
  person_vorname: ['person_vorname', 'vorname', 'first_name'],
  person_nachname: ['person_nachname', 'nachname', 'last_name'],
  person_funktion: ['person_funktion', 'funktion', 'role'],
  person_position: ['person_position', 'position'],
  person_email: ['person_email', 'email'],
  person_email_validation: ['person_email_validation', 'email_validation'],
  person_telefon: ['person_telefon', 'telefon', 'phone'],
  person_linkedin: ['person_linkedin', 'linkedin'],
  person_xing: ['person_xing', 'xing'],
  herkunft_import: ['herkunft_import', 'import_origin'],
  adressquelle: ['adressquelle', 'address_source'],
  verantwortlicher: ['verantwortlicher', 'owner', 'responsible'],
  listenstatus: ['listenstatus', 'list_status'],
  aenderungsart: ['aenderungsart', 'change_type'],
  bearbeiter_initialen: ['bearbeiter_initialen', 'editor_initials'],
  sellify_nummer: ['sellify_nummer', 'sellify_number', 'crm_record_number'],
  statistische_kampagne: ['statistische_kampagne', 'statistical_campaign'],
  fachliche_aufnahmeentscheidung: ['fachliche_aufnahmeentscheidung', 'human_admission_decision'],
  fachliche_entscheidung_begruendung: ['fachliche_entscheidung_begruendung', 'human_decision_reason'],
});

// ACHTUNG: jede Aenderung an index.css braucht hier eine NEUE Nummer. Das
// Stylesheet wird unter genau dieser URL geholt, und Cloudflare speichert
// jede URL ein Jahr lang unveraenderlich. Blieb die Nummer stehen, kam keine
// einzige CSS-Aenderung beim Nutzer an — neun Tage lang unbemerkt.
// Der Cache-Buster des Stylesheets darf NICHT eingefroren sein. Bis zum
// 03.09.2026 stand hier eine feste Zeichenkette von Ende August: die Shell lud
// index.css korrekt mit ihrem eigenen Buster, die App haengte danach ein
// ZWEITES <link> mit dem alten Buster an, der Browser lieferte dafuer die alte
// Datei aus dem Cache - und weil sie zuletzt kam, gewann sie. Jede
// CSS-Aenderung war damit unsichtbar, obwohl sie ausgeliefert war.
// Der Buster kommt jetzt aus der eigenen Modul-URL und wechselt mit jedem
// Deploy mit.
const STYLE_BUILD = (() => {
  try {
    const eigener = new URL(import.meta.url).searchParams.get('v');
    if (eigener) return eigener;
  } catch { /* import.meta.url ohne Query - Rueckfall unten */ }
  return '20260831-outbound-lead-generation-v2-conform-v36';
})();
const COMMAND_REFRESH_MS = 5000;
const REPLICATION_WRITE_TIMEOUT_MS = 60_000;
const CAMPAIGN_TERMINAL_REPAIR_GRACE_MS = 30_000;
const CAMPAIGN_PARENT_PLACEHOLDER_STALE_MS = 30 * 60_000;
const SOURCE_REQUEST_TIMEOUT_MS = 15 * 60 * 1000;
const RESEARCH_POLICY_ID = 'leadgen_research_policy_v1';
// Update-Verteiler: Konfiguration (vom Browser geschrieben) und Status (nur vom
// CTOX-Dienst geschrieben) liegen als eigene Dokumente in derselben Sammlung.
const UPDATE_DIGEST_ID = 'outbound_update_digest_v1';
const UPDATE_DIGEST_STATUS_ID = 'outbound_update_digest_status_v1';
const UPDATE_DIGEST_WEEKDAYS = Object.freeze([[1, 'Mo'], [2, 'Di'], [3, 'Mi'], [4, 'Do'], [5, 'Fr'], [6, 'Sa'], [7, 'So']]);
const UPDATE_DIGEST_TIMEZONES = Object.freeze([
  ['Europe/Berlin', 'Deutschland (Berlin)'],
  ['Europe/Vienna', 'Österreich (Wien)'],
  ['Europe/Zurich', 'Schweiz (Zürich)'],
]);
// A research run reports progress continuously. Silence past this window means
// the task is gone even though the durable status still reads `running`.
const RESEARCH_HEARTBEAT_STALE_MS = 10 * 60 * 1000;
// Obergrenze fuer einen Lead im Zustand "laeuft", wenn zu ihm ueberhaupt kein
// Vorgang mehr auffindbar ist (ANGUS Chemie hing am 11.08.2026 ueber fuenf
// Stunden). Die 30 Minuten von damals galten fuer 90-Sekunden-Laeufe; eine
// Chat-Recherche braucht heute 30-60 Minuten (Sasol 11.09.2026: 53 min, Frist
// je Turn 60 min) und ihr Befehl ist nicht in jedem Browser repliziert - Sasol
// wurde nach 31 Minuten mitten im Lauf als "nicht zurueckgemeldet" beendet.
// Einen wirklich haengenden Lauf beendet seit 1.0.151 der Abbruchknopf.
// 26.09.2026: bei 130 Auftraegen und vier Workern wartet ein Auftrag viele
// Stunden in der Queue, bevor er ueberhaupt laeuft. Nach 3 h wurden wartende
// Leads als "nicht zurueckgemeldet" gescheitert gemeldet, obwohl ihr Auftrag
// noch in der Queue stand; Nutzer starteten sie dann doppelt.
const RESEARCH_RUNNING_MAX_MS = 24 * 60 * 60 * 1000;
const NICHT_ZURUECKGEMELDET = 'Der Vorgang hat sich nicht zurueckgemeldet. Recherche bitte erneut starten.';
// Frist fuer die Sperrvermerkspruefung gegen die CRM-Projektion.
// Am 12.08.2026 auf CHEMOFAST gemessen, serielle Einzelabfragen:
//   "Sperrvermerkspruefung hat die Frist ueberschritten"   (bei 12 s)
//   "Sperrvermerkspruefung lead_b7nl4a: 50360 ms, 1 Firmen, 3 Personen"
// Die Pruefung LIEF durch und fand die Daten — nur 38 Sekunden nach ihrer
// damaligen Frist. Ursache: viele serielle await-Abfragen gegen
// den serverseitigen Sellify-Lesezugriff (je Firma contact_id, je Kontakt person_id,
// email, display_name, plus bis zu zwoelf Namensvarianten).
// Heilung: unabhaengige Abfragen laufen nebenlaeufig (Promise.all), gleiche
// Selektoren nur einmal, Mehrfachwerte per $in. Ziel: unter 3 s je Lead.
// Gemessen: vorher 50360 ms fuer 1 Firma / 3 Personen; mit Parallelisierung
// und $in liegt die Frist grosszuegig ueber dem 3-s-Ziel, aber weit unter
// der alten 50-s-Welt — ein haengender Vorgang blockiert die Kette nicht mehr.
// Gemessen am 03.09.2026 auf Kundeninstanz: eine Sperrvermerkspruefung braucht 7 bis
// 75 Sekunden (CRM-Projektion mit 17.520 Firmen / 60.639 Personen, Bedarfs-
// abfrage). Mit 30 Sekunden riss jede zweite Pruefung die Frist und der Nutzer
// sah dauerhaft "hat nicht geantwortet", obwohl der Befehl serverseitig sauber
// abschloss. Die Frist deckt jetzt die gemessene Oberkante ab.
const RECIPIENT_ELIGIBILITY_TIMEOUT_MS = 90_000;
const LEGACY_RESEARCH_POLICY_IMPORT_ID = 'settings_research_policy';
const REPLICATED_COLLECTIONS = Object.freeze([
  'outbound_lead_generation_sources',
  'outbound_lead_generation_adapters',
  'outbound_lead_generation_imports',
  'outbound_lead_generation_research_policies',
  'outbound_lead_generation_leads',
]);
const DEFAULT_RESEARCH_POLICY = [
  "0. Zuerst prüfen, ob Unternehmen und Ansprechpartner bereits in Sellify vorhanden sind. Der Sellify-Bestand ist der Ausgangswert jedes Feldes und zugleich eine Quelle. Er wird nur geändert, wenn eine externe Quelle etwas anderes belegt.",
  "1. Identität und Registerdaten zuerst klären: Firmierung (firma_name) einschließlich früherer Namen und Umfirmierungen (firma_fruehere_namen), Rechtsform, Aktivitätsstatus (firma_aktivitaetsstatus) sowie Geschäftsführung (firma_geschaeftsfuehrung) und Prokura (firma_prokura) aus dem Register. Deutschland: Handelsregister, Northdata, Bundesanzeiger, CompanyHouse. Österreich: Firmenbuch/JustizOnline, FirmenABC, Northdata. Schweiz: Zefix/Handelsregister, SHAB, Moneyhouse, Northdata.",
  "2. Danach Website, Anschrift und Kommunikation ergänzen: Domain (firma_domain) zuerst, weil Impressum und Unternehmensseite ohne sie nicht auffindbar sind. Dann Anschrift (firma_anschrift), Besucheranschrift (firma_besucheranschrift) und Postanschrift (firma_postanschrift) getrennt führen, Postfach (firma_postfach), PLZ (firma_plz), Ort (firma_ort), Land (firma_land), Firmen-E-Mail (firma_email), Telefon (firma_telefon) und Fax (firma_fax, am besten über Google). Priorität der Adresssuche in allen drei Ländern: 1. Impressum der offiziellen Website, 2. FirmenABC (Österreich) bzw. Zefix (Schweiz) bzw. D&B Hoovers (Deutschland, Schweiz), 3. Northdata als letzte Quelle.",
  "3. Danach die Kennzahlen prüfen: Branche bzw. WZ-Code (wz_code), Umsatz (umsatz), Mitarbeiterzahl (mitarbeiter) und die Geschäftstätigkeit (firma_geschaeftstaetigkeit). WZ-Code in allen drei Ländern ausschließlich aus D&B Hoovers und/oder Leadfeeder; Deutschland zusätzlich Bundesanzeiger. Die Geschäftstätigkeit über D&B Hoovers oder eine Google-Suche nach Unternehmensname und Tätigkeit klären; aus der Homepage einen Firmensteckbrief erstellen (firma_homepage_fact_sheet).",
  "4. Zuletzt die Ansprechpartner recherchieren: Anrede/Geschlecht (person_geschlecht), Titel (person_titel), Vorname (person_vorname), Nachname (person_nachname), Funktion (person_funktion), Position (person_position), E-Mail (person_email), Telefon (person_telefon), LinkedIn-Profil (person_linkedin) und XING-Profil (person_xing). E-Mail-Adressen aus dem Unternehmensmuster ableiten und über MailTester und Experte validieren (person_email_validation). Gesucht wird mindestens eine Person aus jeder dieser Kategorien, in dieser Reihenfolge: Geschäftsführung/Gesamtverantwortung, Prokura, Leitung Finanzen, Einkauf, Supply Chain Management, Operations, Technik, Entwicklung. Personen über die Namenssuche in LinkedIn und XING ansteuern, nicht über angeklickte Suchtreffer.",
  "4a. Die E-Mail-Pruefung (person_email_validation) uebernimmt CTOX selbst: nach jedem Rueckschreiben prueft der Daemon jede gelieferte Kontaktadresse ueber experte.de und haengt das Ergebnis dem Kontakt an. Liefere deshalb jede gefundene persoenliche Adresse als person_email mit Beleg und person_key. Versuche die Pruefung NICHT ueber `ctox web read` und setze person_email_validation NICHT auf no_match, nur weil du sie nicht selbst ausfuehren kannst.",
  "5. Belegregel (Owner 23.09.2026): Für jedes Feld genügt EINE passende belegte Quelle mit URL und wörtlichem Zitat, das den konkreten Wert tatsächlich nennt. Weitere unabhängige Quellen stärken den Wert und werden angezeigt, sind aber keine Pflicht. Zwei Seiten derselben Quelle sind eine Quelle, und Sellify allein belegt nichts. Eine Spanne (z. B. 11–100) belegt keinen Einzelwert.",
  "5a. Selbstauskünfte wie firma_domain, firma_email, firma_telefon, firma_fax, firma_postfach, firma_besucheranschrift, firma_postanschrift, firma_homepage_fact_sheet sowie alle person_-Felder belegt die Unternehmensseite (Impressum, Kontakt, Team) bzw. das Profil selbst, mit URL und wörtlichem Zitat. Einen so belegten Wert eintragen, niemals als no_match verwerfen mit der Begründung, er stehe nur auf der eigenen Website. Eine persönliche E-Mail-Adresse, die genau so auf der offiziellen Unternehmensseite steht, ist belegt; eine SMTP-Prüfung ist dafür nicht nötig.",
  "5c. Belege als reine JSON-Liste senden: \"sources\": [ { … }, { … } ]. Kein Trägerobjekt wie {\"item\": [ … ]} — so verpackte Belege gehen beim Speichern verloren. Das gilt auch für result.person_records und result.evidence.",
  "5b. Werte in der Schreibweise der Quelle übernehmen, mit Umlauten und ß. Keine Umschrift: Nürnberg, nicht Nuernberg; Lechstraße, nicht Lechstrasse. Schweizer Adressen behalten ihr ss.",
  "6. Bei Zugriffshürden Web-Stack-Unlocking verwenden; bei Anmeldung den CTOX-Browser öffnen und erst nach sichtbarer Bestätigung des Nutzers mit derselben persistenten Sitzung fortsetzen. Eine Quelle, die blockiert oder vorübergehend nicht erreichbar ist, belegt nichts — weder den Wert noch sein Fehlen.",
  "7. Unklare oder widersprüchliche Daten als prüfbedürftig markieren und nicht automatisch an Sellify übergeben. Bei Widerspruch zwischen zwei Quellen das Feld leer lassen und beide Werte mit ihrer Quelle festhalten.",
].join('\n');
const CURRENT_USER_COPY = Object.freeze({
  de: {
    adapterPending: 'Noch nicht eingerichtet',
    adapterBuilding: 'Datenzugriff wird eingerichtet',
    buildAdapter: 'Datenzugriff einrichten',
    authRequested: 'Browser-Anmeldung angefordert',
    credentialReady: 'Zugang hinterlegt',
  },
  en: {
    adapterPending: 'Not configured',
    adapterBuilding: 'Data access is being configured',
    buildAdapter: 'Configure data access',
    authRequested: 'Browser sign-in requested',
    credentialReady: 'Credentials available',
  },
});

const state = {
  ctx: null,
  collections: {},
  sources: [],
  adapters: [],
  imports: [],
  leads: [],
  // Only full records enter leads. The list has its own read-only DTOs.
  leadListRows: null,
  fullLeadReadSequence: 0,
  fullLeadAppliedSequence: new Map(),
  selectedLeadId: '',
  activeContactTabs: new Map(),
  // Eigene Aenderungen, bis die Datenbank sie zurueckliefert (siehe patchLead).
  pendingLeadPatches: new Map(),
  // Ein Reiter fuer alle Leads: wer auf "Unternehmen" steht, bleibt beim
  // Durchklicken der Leads auf "Unternehmen".
  activeDetailTab: restoreActiveDetailTab(),
  scrollPositions: { campaigns: 0, leads: 0, detailByLead: new Map() },
  selectedLeadIds: new Set(),
  selectionAnchorId: '',
  selectedCampaign: '',
  campaignViewMode: 'table',
  leadEditorOpen: false,
  leadEditorId: '',
  leadDraft: null,
  sourcePanelOpen: false,
  sourcePanelView: 'sources',
  pendingResearchIds: new Set(),
  sourceTogglePending: new Set(),
  sourceToggleIntent: new Map(),
  reconcilingCommands: false,
  researchCommandQueryLogged: false,
  reconcilingCampaignRuns: false,
  reconcilingAdapterCommands: false,
  commandRefreshTimer: null,
  collectionReloadTimer: null,
  sellifyCompanies: null,
  sellifyPeople: null,
  sellifyLookupCache: new Map(),
  sellifyLookupInflight: new Map(),
  sellifyMatch: null,
  recipientEligibility: new Map(),
  recipientEligibilityReady: new Set(),
  recipientEligibilitySignatures: new Map(),
  recipientEligibilityTimedOut: new Set(),
  sellifyPrecheckTried: new Map(),
  recipientEligibilityBusy: new Set(),
  registry: new Map(),
  registryStand: 0,
  registryLaeuft: false,
  registryFehler: '',
  recipientRemovalNotices: new Map(),
  reconcilingRecipientEligibility: false,
  search: '',
  sourceSearch: '',
  leadTrayOpen: false,
  sourcePanelSignature: '',
  leadSortKey: 'name',
  leadSortDir: 'asc',
  leadStatusFilter: new Set(),
  researchPolicy: DEFAULT_RESEARCH_POLICY,
  researchPolicyDraft: DEFAULT_RESEARCH_POLICY,
  researchPolicyFollowup: '',
  researchPolicyFollowupDraft: '',
  researchPolicyRecord: null,
  researchFieldKeys: [...RESEARCH_FIELDS],
  researchFieldKeysDraft: [...RESEARCH_FIELDS],
  // Entwurf der optionalen Felder (null = gespeicherter Stand).
  optionalFieldsDraft: null,
  adapterReconciliationPending: false,
  subscriptions: [],
  messages: {},
  campaignRuns: new Map(),
  replicationBridges: new Map(),
  campaignMutationMessage: '',
  adapterInspectorSourceId: '',
  syncPending: true,
  // Waehrend des Starts schreibt die App nichts automatisch (25.09.2026: ein
  // frisch gestarteter Browser schrieb 13 Leads mit altem Stand zurueck).
  startLaeuft: true,
  syncError: '',
  syncMessage: 'Daten werden verbunden',
  syncRetryTimer: null,
  syncInFlight: false,
  syncWaitingCollections: new Set(REPLICATED_COLLECTIONS),
};

export async function mount(ctx) {
  await ensureStyles();
  state.ctx = ctx;
  state.uiMounted = true;
  globalThis.__olgDialogZiel = ctx.host;
  ctx.host.classList.add('outbound-lead-generation');
  const locale = ctx.locale === 'en' ? 'en' : 'de';
  const loadedMessages = await loadModuleMessages(import.meta.url, locale).catch(() => ({}));
  state.messages = { ...loadedMessages, ...CURRENT_USER_COPY[locale] };
  state.collections = {
    sources: ctx.db.collection('outbound_lead_generation_sources'),
    adapters: ctx.db.collection('outbound_lead_generation_adapters'),
    imports: ctx.db.collection('outbound_lead_generation_imports'),
    researchPolicies: ctx.db.collection('outbound_lead_generation_research_policies'),
    leads: ctx.db.collection('outbound_lead_generation_leads'),
  };
  state.sellifyCompanies = createSellifyLookupFacade('company');
  state.sellifyPeople = createSellifyLookupFacade('person');
  // Lokales Replikat der CRM-Firmen (im Manifest deklariert; die Shell
  // registriert das Schema des Sellify-Moduls mit). Damit laeuft die
  // Sellify-Weiche komplett OHNE Kanal-Rundreise — die Rundreise brauchte
  // unter Last ~60s und liess die Weiche fail-closed abbrechen.
  try {
    state.sellifyCompaniesLocal = ctx.db.collection('sellify_companies') || null;
  } catch {
    state.sellifyCompaniesLocal = null;
  }
  state.syncPending = true;
  state.syncError = '';
  state.syncMessage = navigator.onLine === false ? 'Keine Netzwerkverbindung' : 'Daten werden verbunden';
  state.collectionReadErrors = new Map();
  state.syncWaitingCollections = new Set(REPLICATED_COLLECTIONS);
  const handleOffline = () => {
    state.syncPending = false;
    state.syncError = 'offline';
    state.syncMessage = 'Keine Netzwerkverbindung. Vorhandene Daten bleiben sichtbar.';
    render();
  };
  const handleOnline = () => {
    state.syncMessage = 'Netzwerk wieder verfügbar. Daten werden synchronisiert';
    void retryInitialSync();
  };
  globalThis.addEventListener('offline', handleOffline);
  globalThis.addEventListener('online', handleOnline);
  bindCollections();
  bindUi();
  // Zuerst zeichnen, dann laden. Am 03.09.2026 auf Kundeninstanz gemessen: vom
  // Seitenaufruf bis zur nutzbaren Oberflaeche vergingen 167 Sekunden, davon
  // fast alles im `await reload()` - die fuenf Collections antworten erst,
  // wenn ihre Replikation steht, und die Shell faehrt 201 Collections hoch.
  // Der Nutzer sah in dieser Zeit ein totes "Modul-Workspace wird geladen".
  // Die Daten kommen dadurch nicht frueher, aber die App ist sofort da und
  // sagt ehrlich, dass sie noch synchronisiert.
  render();
  reload()
    .then(() => render())
    .catch((error) => {
      console.warn('[outbound-lead-generation] Erstes Laden fehlgeschlagen', String(error?.message || error).slice(0, 140));
    });
  retryInitialSync();
  state.kampagnenPersonenTimer = globalThis.setInterval(() => {
    if (!/^fertig/.test(String(globalThis.document?.documentElement?.dataset?.olgBoot || ''))) return;
    kampagnenPersonenPflege().catch(() => {});
  }, 20_000);
  state.commandRefreshTimer = globalThis.setInterval(() => {
    Promise.all([
      reconcileCampaignResearchRuns({ authoritative: true }),
      reconcileResearchCommands({ authoritative: true }),
      reconcileAdapterCommands({ authoritative: true }),
    ]).then(async (changes) => {
      if (!changes.some(Boolean)) return;
      const keys = new Set();
      if (changes[0]) { keys.add('imports'); keys.add('leads'); }
      if (changes[1]) keys.add('leads');
      if (changes[2]) { keys.add('adapters'); keys.add('sources'); }
      await reload([...keys]);
      render();
    }).catch(() => {});
  }, COMMAND_REFRESH_MS);
  return () => {
    state.uiMounted = false;
    state.collectionReloader?.dispose();
    state.collectionReloader = null;
    state.collectionBindingGeneration = (state.collectionBindingGeneration || 0) + 1;
    if (state.leerNachladenTimer) globalThis.clearTimeout(state.leerNachladenTimer);
    state.leerNachladenTimer = null;
    if (state.commandRefreshTimer) globalThis.clearInterval(state.commandRefreshTimer);
    if (state.kampagnenPersonenTimer) globalThis.clearInterval(state.kampagnenPersonenTimer);
    if (state.freitextTakt) globalThis.clearInterval(state.freitextTakt);
    state.freitextTakt = null;
    state.commandRefreshTimer = null;
    state.replicationBridges.clear();
    if (state.syncRetryTimer) globalThis.clearTimeout(state.syncRetryTimer);
    state.syncRetryTimer = null;
    globalThis.removeEventListener('offline', handleOffline);
    globalThis.removeEventListener('online', handleOnline);
    if (state.collectionReloadTimer) globalThis.clearTimeout(state.collectionReloadTimer);
    state.collectionReloadTimer = null;
    state.subscriptions.forEach((subscription) => subscription?.unsubscribe?.());
    state.subscriptions = [];
    ctx.host.classList.remove('outbound-lead-generation');
    ctx.host.replaceChildren();
  };
}

// P0-Selbstheilung (31.08.): Nach einem CTOX-Service-Neustart ist der
// business_commands-Kanal "cancelled" und erholt sich im lebenden Tab nicht -
// jeder Recherche-Start, jedes Speichern und Loeschen starb daran, dialoglos.
// Die Modul-Fassade exponiert restartCollection auf der GETEILTEN Shell-Sync-
// Runtime: Kanal neu aufbauen, Collection-Handles neu aufloesen, Operation
// genau EINMAL wiederholen.
function istKanalAbriss(error) {
  return /was cancelled|wurde abgebrochen|MODULE_CONTEXT_CLOSED|nicht mit CTOX verbunden|no authenticated WebRTC peer|collection peer: not-connected/i.test(String(error?.message || error));
}

// Der Auftrag hat CTOX nie erreicht: der Browser hatte keine Sync-Verbindung
// ("business_commands has no authenticated WebRTC peer after 45000 ms").
// Kundeninstanz 23.09.2026: CARBAGAS, DuPont und CHT standen danach als
// "fehlgeschlagen" da, obwohl am Server kein einziger Befehl ankam.
function auftragNichtZugestellt(error) {
  return /no authenticated WebRTC peer|collection peer: not-connected|nicht mit CTOX verbunden/i.test(String(error?.message || error));
}
const NICHT_ZUGESTELLT_HINWEIS = 'Der Auftrag hat CTOX nicht erreicht: Die Verbindung dieses Browsers zu CTOX ist unterbrochen. Der Lead bleibt offen. Bitte die Seite neu laden und die Recherche erneut starten.';

async function recoverCommandChannel(reason) {
  console.info('[olg] Kanal-Selbstheilung', { reason });
  try { await state.ctx?.sync?.restartCollection?.('business_commands'); } catch (error) {
    console.warn('[olg] restartCollection business_commands fehlgeschlagen', error);
  }
  const namen = {
    sources: 'outbound_lead_generation_sources',
    adapters: 'outbound_lead_generation_adapters',
    imports: 'outbound_lead_generation_imports',
    researchPolicies: 'outbound_lead_generation_research_policies',
    leads: 'outbound_lead_generation_leads',
  };
  for (const [key, name] of Object.entries(namen)) {
    try {
      await state.ctx?.sync?.restartCollection?.(name);
      state.collections[key] = state.ctx.db.collection(name);
    } catch (error) {
      console.warn('[olg] Handle-Neuaufloesung fehlgeschlagen', { name, message: String(error?.message || error) });
    }
  }
  // Recovered handles need new invalidation subscriptions, not observers on
  // the cancelled handles. Rebinding also invalidates their in-flight reads.
  if (state.uiMounted !== false) bindCollections();
}

// Generischer Einmal-Retry fuer Kanalabrisse.
async function mitKanalHeilung(operation, label) {
  try {
    return await operation();
  } catch (error) {
    if (!istKanalAbriss(error)) throw error;
    await recoverCommandChannel(label);
    return operation();
  }
}

function bootSchritt(name) {
  try { document.documentElement.dataset.olgBoot = `${name}@${new Date().toISOString().slice(11, 19)}`; } catch { /* nur Diagnose */ }
}
function bootFehler(schritt, error) {
  const text = `${schritt}: ${String(error?.message || error).slice(0, 200)}`;
  console.warn('[outbound-lead-generation] Startschritt fehlgeschlagen', text);
  try { document.documentElement.dataset.olgBootFehler = text; } catch { /* nur Diagnose */ }
}
// Reparatur- und Abgleichsschritte beim Start sind Pflege, keine Voraussetzung.
// Warf einer, verwarf retryInitialSync den ganzen Start und begann nach 8 s von
// vorn - gemessen 25.09.2026: seed -> reload -> repair im 20-s-Takt, "fertig"
// nie erreicht, jede Runde mit Schreibvorgaengen. Jetzt laeuft der Rest weiter.
async function pflegeSchritt(schritt, arbeit) {
  try { return await arbeit(); } catch (error) {
    bootFehler(schritt, error);
    // Ein gescheitertes Laden (QUERY_COLLECTOR_TIMEOUT im frischen Browser,
    // 26.09.2026) liess die Kampagnenliste ~2,5 min leer, bis zufaellig ein
    // spaeteres Nachladen kam. Jetzt sofort erneut, gedrosselt.
    if (/^reload/.test(schritt)) scheduleCollectionReload();
    return null;
  }
}

async function synchronizeInitialData() {
  bootSchritt('start-collections');
  await Promise.all(REPLICATED_COLLECTIONS.map(async (collection) => {
    const bridge = await withTimeout(
      Promise.resolve(state.ctx.sync?.startCollection?.(collection)),
      `${collection} konnte nicht gestartet werden.`,
    );
    state.replicationBridges.set(collection, bridge);
  }));
  bootSchritt('readiness');
  // Die Liste erscheint sofort mit dem vorhandenen Stand; der Hinweis "wird
  // synchronisiert" bleibt, bis der Start fertig ist (vorher 6-7 s leer).
  void reload().then(() => render()).catch(() => { scheduleCollectionReload(); });
  // Readiness ist eine BESCHRIFTUNG, kein Tor. `catching-up` ist im
  // Readiness-Vertrag der Sammeleimer fuer JEDEN nicht-terminalen Zustand -
  // auch fuer den dauerhaften: ein Tab, der nicht Multi-Tab-Leader ist,
  // bekommt Follower-Bridges ohne `initialReplicationState` und meldet
  // deshalb bis zum Seitenschluss `catching-up`. Weil der Listener nur bei
  // ZUSTANDSWECHSELN feuert, kommt nie ein Signal, der 60-s-Timeout wirft,
  // der ganze Bootlauf wird verworfen und alle 4 s neu versucht - die App
  // haengt dann dauerhaft in "CTOX-Verbindung wird hergestellt (0/5)".
  // Jedes andere Business-OS-Modul rendert unabhaengig von Readiness und
  // nutzt sie nur, um eine leere Liste als "laedt noch" zu beschriften.
  // Wir warten weiterhin, aber ein nicht erreichter Live-Zustand darf den
  // Start nicht mehr abbrechen.
  const nichtLive = [];
  await Promise.all(REPLICATED_COLLECTIONS.map((collection) => waitForCollectionReadiness(collection)
    .catch((error) => {
      nichtLive.push(collection);
      console.info('[outbound-lead-generation] collection not live yet, continuing', {
        collection,
        message: error?.message || String(error),
      });
      return null;
    })));
  bootSchritt('seed-sources');
  const sourceContractChanged = await pflegeSchritt('seed-sources', () => seedSources());
  bootSchritt('reload');
  await pflegeSchritt('reload-0', () => reload());
  if (!listLeads().length) planeLeerNachladen();
  // Reparatur- und Abgleichsroutinen schreiben ganze Datensaetze. Auf einem
  // noch nicht live abgeglichenen Stand schrieben sie alte Staende zurueck
  // (needs_review -> failed, CH -> DE; 25.09.2026). Sie laufen deshalb nur,
  // wenn alle Sammlungen live sind; sonst beim naechsten Start.
  if (!nichtLive.length) {
    bootSchritt('repair-adapters');
    // Nachladen nur, wenn ein Schritt etwas geaendert hat: jedes reload sind
    // mehrere Abfragen gegen das Abfragekontingent des Browsers.
    if (await pflegeSchritt('repair-adapters', () => repairAdapterActivationDrift())) {
      await pflegeSchritt('reload-1', () => reload());
    }
    bootSchritt('repair-status');
    if (await pflegeSchritt('repair-status', () => repairUntrackedResearchStatuses())) {
      await pflegeSchritt('reload-2', () => reload());
    }
    bootSchritt('reconcile');
    await pflegeSchritt('reconcile-campaign', () => reconcileCampaignResearchRuns({ authoritative: true }));
    await pflegeSchritt('reconcile-research', () => reconcileResearchCommands({ authoritative: true }));
    await pflegeSchritt('reconcile-adapter', () => reconcileAdapterCommands({ authoritative: true }));
    await pflegeSchritt('reload-3', () => reload());
  } else {
    console.info('[outbound-lead-generation] Abgleich beim Start ausgelassen, nicht live:', nichtLive);
  }
  bootSchritt('fertig');
  state.startLaeuft = false;
  if (!nichtLive.includes('outbound_lead_generation_leads')) starteLaenderkorrektur();
  state.syncPending = false;
  state.syncError = '';
  state.syncWaitingCollections.clear();
  render();
  // Die Sellify-Sperrvermerk-Pruefung lief je neuem Lead nacheinander (bis zu
  // 20 s) und hielt die ganze App in "Daten werden synchronisiert" — gemessen
  // 93 s (Klicktest P2 BG-09 / V10). Sie laeuft jetzt im Hintergrund; bis zu
  // ihrem Ergebnis stehen Empfaenger als "wird geprueft".
  void enforceRecipientEligibility().catch((fehler) => {
    console.warn('[outbound-lead-generation] Empfaengerpruefung im Hintergrund fehlgeschlagen', fehler);
  });
  if (sourceContractChanged) {
    void queueAdapterReconciliationAfterSourceChange('builtin_source_contract_upgraded');
  }
}

async function retryInitialSync() {
  if (state.syncInFlight) return;
  state.syncInFlight = true;
  if (state.syncRetryTimer) globalThis.clearTimeout(state.syncRetryTimer);
  state.syncRetryTimer = null;
  state.syncPending = true;
  state.syncError = '';
  // "Netzwerk wieder verfügbar" wurde hier sofort ueberschrieben und war nie
  // zu sehen (Klicktest P1 SYN-03a).
  state.syncMessage = navigator.onLine === false
    ? 'Keine Netzwerkverbindung'
    : (String(state.syncMessage || '').startsWith('Netzwerk wieder') ? state.syncMessage : 'Daten werden synchronisiert');
  state.syncWaitingCollections = new Set(REPLICATED_COLLECTIONS);
  state.replicationBridges.clear();
  render();
  try {
    await synchronizeInitialData();
    state.syncMessage = '';
    // Endet ein Abgleich, waehrend das Netz schon weg ist, blieb die Fusszeile
    // leer statt "offline" zu sagen (Klicktest P1 SYN-01a).
    if (navigator.onLine === false) {
      state.syncError = 'offline';
      state.syncMessage = 'Keine Netzwerkverbindung. Vorhandene Daten bleiben sichtbar.';
      render();
    }
  } catch (error) {
    bootFehler('start', error);
    const message = error?.message || String(error);
    const offline = navigator.onLine === false;
    const catchingUp = /status:\s*catching-up/i.test(message);
    const transportPending = /nicht mit CTOX verbunden|offline-pending/i.test(message);
    state.syncPending = catchingUp || transportPending;
    state.syncError = offline ? 'offline' : (catchingUp || transportPending) ? '' : message;
    state.syncMessage = offline
      ? 'Keine Netzwerkverbindung. Vorhandene Daten bleiben sichtbar.'
      : (catchingUp || transportPending)
        ? 'CTOX-Verbindung wird hergestellt'
        : 'CTOX ist gerade nicht erreichbar. Erneuter Versuch läuft.';
    ((catchingUp || transportPending) ? console.info : console.warn)('[outbound-lead-generation] synchronization delayed', { message, offline, catchingUp, transportPending });
    render();
    state.syncRetryTimer = globalThis.setTimeout(() => {
      state.syncRetryTimer = null;
      void retryInitialSync();
    }, (catchingUp || transportPending) ? 4_000 : 8_000);
  } finally {
    state.syncInFlight = false;
  }
}

async function flushReplicatedCollection(collection, documents = []) {
  let bridge = state.replicationBridges.get(collection);
  if (!bridge) {
    bridge = await withTimeout(
      Promise.resolve(state.ctx.sync?.startCollection?.(collection)),
      `${collection} konnte nicht gestartet werden.`,
    );
    state.replicationBridges.set(collection, bridge);
  }
  if (!bridge?.state && bridge?.ready) {
    bridge = await withTimeout(Promise.resolve(bridge.ready), `${collection} konnte nicht verbunden werden.`);
    state.replicationBridges.set(collection, bridge);
  }
  if (bridge?.mode === 'follower' && typeof bridge.flush === 'function') {
    await withTimeout(
      Promise.resolve(bridge.flush()),
      `${collection} konnte nicht repliziert werden.`,
      REPLICATION_WRITE_TIMEOUT_MS,
    );
    return;
  }
  const replicationState = bridge?.state;
  await waitForReplicationPeer(replicationState, collection);
  if (documents.length && typeof replicationState?.pushDocumentsToRemotePeers === 'function') {
    await withTimeout(
      Promise.resolve(replicationState.pushDocumentsToRemotePeers(documents)),
      `${collection} konnte nicht repliziert werden.`,
      REPLICATION_WRITE_TIMEOUT_MS,
    );
    return;
  }
  if (typeof replicationState?.pushToRemotePeers === 'function') {
    await withTimeout(
      Promise.resolve(replicationState.pushToRemotePeers({ requireSuccess: true })),
      `${collection} konnte nicht repliziert werden.`,
      REPLICATION_WRITE_TIMEOUT_MS,
    );
    return;
  }
  if (typeof replicationState?.scheduleLocalWritePush === 'function') {
    await withTimeout(
      Promise.resolve(replicationState.scheduleLocalWritePush()),
      `${collection} konnte nicht repliziert werden.`,
      REPLICATION_WRITE_TIMEOUT_MS,
    );
    return;
  }
  throw new Error(`${collection} hat keinen aktiven Replikationskanal.`);
}

async function waitForReplicationPeer(replicationState, collection) {
  if (!replicationState) throw new Error(`${collection} hat keinen aktiven Replikationskanal.`);
  const deadline = Date.now() + REPLICATION_WRITE_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (String(replicationState.activeRemotePeerId || '').trim()) return;
    const status = replicationState.getTransportStatus?.() || {};
    if (Number(status.activePeerCount || 0) > 0) return;
    const connections = Array.isArray(status.connectionStates) ? status.connectionStates : [];
    if (connections.some((connection) => {
      const channelState = connection?.channelState || connection?.channelReadyState || '';
      const peerState = connection?.peerConnectionState || '';
      return connection?.open === true
        || (channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(peerState));
    })) return;
    await new Promise((resolve) => globalThis.setTimeout(resolve, 100));
  }
  throw new Error(`${collection} ist nicht mit dem CTOX-Server verbunden.`);
}

async function withTimeout(promise, message, timeoutMs = 20000) {
  let timer = null;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(message)), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function waitForReplicationBridge(bridge, collection, timeoutMs = REPLICATION_WRITE_TIMEOUT_MS) {
  let readyBridge = bridge;
  if (!readyBridge?.state && readyBridge?.ready) {
    readyBridge = await withTimeout(
      Promise.resolve(readyBridge.ready),
      `${collection} konnte nicht verbunden werden.`,
      REPLICATION_WRITE_TIMEOUT_MS,
    );
    state.replicationBridges.set(collection, readyBridge);
  }
  const replicationState = readyBridge?.state;
  if (!replicationState) {
    throw new Error(`${collection} hat keinen aktiven Replikationskanal.`);
  }
  const wait = typeof replicationState?.awaitInSync === 'function'
    ? replicationState.awaitInSync.bind(replicationState)
    : typeof replicationState?.awaitInitialReplication === 'function'
      ? replicationState.awaitInitialReplication.bind(replicationState)
      : null;
  if (!wait) {
    await waitForReplicationPeer(replicationState, collection);
    return;
  }
  await Promise.race([
    wait(),
    new Promise((_, reject) => {
      setTimeout(() => reject(new Error(`${collection} konnte nicht synchronisiert werden.`)), timeoutMs);
    }),
  ]);
}

async function waitForCollectionReadiness(collection, timeoutMs = REPLICATION_WRITE_TIMEOUT_MS) {
  const read = state.ctx.sync?.collectionReadiness;
  const subscribe = state.ctx.sync?.subscribeCollectionReadiness;
  if (typeof read !== 'function' || typeof subscribe !== 'function') {
    return waitForReplicationBridge(state.replicationBridges.get(collection), collection, timeoutMs);
  }

  const initial = read.call(state.ctx.sync, collection);
  if (initial?.ready === true || initial?.state === 'live') {
    state.syncWaitingCollections.delete(collection);
    return;
  }

  await new Promise((resolve, reject) => {
    let settled = false;
    let unsubscribe = () => {};
    const finish = (handler, value) => {
      if (settled) return;
      settled = true;
      globalThis.clearTimeout(timer);
      unsubscribe();
      handler(value);
    };
    const timer = globalThis.setTimeout(() => {
      const snapshot = read.call(state.ctx.sync, collection);
      const readiness = String(snapshot?.state || 'unbekannt');
      finish(reject, new Error(`${collection} konnte nicht synchronisiert werden (Status: ${readiness}).`));
    }, timeoutMs);
    const onReadiness = (snapshot) => {
      if (snapshot?.ready === true || snapshot?.state === 'live') {
        state.syncWaitingCollections.delete(collection);
        finish(resolve);
        return;
      }
      if (snapshot?.state === 'offline-pending') {
        finish(reject, new Error(`${collection} ist derzeit nicht mit CTOX verbunden.`));
      }
    };
    const subscription = subscribe.call(state.ctx.sync, collection, onReadiness);
    unsubscribe = typeof subscription === 'function' ? subscription : () => {};
    if (settled) unsubscribe();
  });
}

async function ensureStyles() {
  const href = new URL(`./index.css?v=${STYLE_BUILD}`, import.meta.url).href;
  // Die Wiedererkennung fragte ein Attribut ab, das nie gesetzt wurde
  // (data-outbound-lead-generation-styles gegen dataset.leadgenOutboundStyles).
  // Sie traf deshalb nie, und jeder Mount haengte ein weiteres <link> an.
  if (document.querySelector(`link[data-leadgen-outbound-styles="${STYLE_BUILD}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  link.dataset.leadgenOutboundStyles = STYLE_BUILD;
  document.head.append(link);
  await new Promise((resolve) => {
    link.addEventListener('load', resolve, { once: true });
    link.addEventListener('error', () => {
      console.warn('[outbound-lead-generation] styles could not be loaded; mounting with cached shell styles');
      resolve();
    }, { once: true });
  });
}

function source(id, label, url, countries, fieldKeys, credentialSecretName = '', options = {}) {
  return {
    id, label, url, countries, fieldKeys,
    targetKey: id.replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, ''),
    credentialSecretName,
    inputDriven: options.inputDriven === true,
    startUrlSource: String(options.startUrlSource || ''),
  };
}

// Zugangsverweise, die eine mitgelieferte Quelle abgeloest hat. Nur exakt
// dieser Altwert wird ersetzt; ein vom Owner selbst gesetzter Verweis bleibt.
// Mehrere Altwerte je Quelle: LinkedIn trug erst den Browser-Login, dann bis
// 1.0.185 den Namen BRIGHTDATA_API_TOKEN — hinterlegt ist der Schluessel aber als
// BRIGHTDATA_API_KEY, das Skript liest genau diesen (22.09.2026).
const VERALTETE_ZUGANGSVERWEISE = {
  'linkedin.com': ['LINKEDIN_BROWSER_LOGIN', 'BRIGHTDATA_API_TOKEN'],
};

// Aendert sich mit dem Zugang auch das Wesen der Quelle, muss die Bezeichnung
// mitziehen: sonst steht dort weiter "LinkedIn" und niemand sieht, dass die
// Daten ueber den Bright Data People Scraper kommen (Owner 18.09.2026:
// "ich sehe ihn noch nicht"). Nur die alte Standardbezeichnung wird ersetzt,
// eine selbst vergebene bleibt.
const VERALTETE_BEZEICHNUNGEN = {
  'linkedin.com': 'LinkedIn',
};

// Keine Quellen: Dokumentations- und Versuchsadressen, die ein Worker am
// 18.09.2026 als Ziel angelegt hatte. Owner 23.09.2026: example.com darf
// ueberhaupt kein Adapter sein; beide standen im taeglichen Update-Bericht als
// "gestoerte Quelle". Sie werden samt Adaptern entfernt, nicht nur verborgen.
const ENTFERNTE_QUELLEN = ['example.com', 'html.duckduckgo.com'];
// Felder, die eine mitgelieferte Quelle nicht (mehr) liefern kann. Die sonst
// additive Zusammenfuehrung wuerde sie ewig mitschleppen.
const ENTFERNTE_QUELLFELDER = {};

async function entferneScheinquellen(quellDokumente = null) {
  // Beide wurden als "eigene" Quellen (builtin: false) angelegt; entfernt
  // werden sie trotzdem, sie sind keine.
  let entfernt = 0;
  const adapter = await state.collections.adapters.find().exec();
  for (const doc of adapter) {
    const daten = doc.toJSON?.() || doc;
    if (!ENTFERNTE_QUELLEN.includes(String(daten?.source_id || ''))) continue;
    await doc.remove();
    entfernt += 1;
  }
  for (const id of ENTFERNTE_QUELLEN) {
    const quelle = quellDokumente
      ? (quellDokumente.get(id) || null)
      : await state.collections.sources.findOne(id).exec();
    if (quelle) {
      await quelle.remove();
      entfernt += 1;
    }
  }
  if (entfernt) console.info(`[outbound] ${entfernt} Schein-Quelle(n)/Adapter entfernt: ${ENTFERNTE_QUELLEN.join(', ')}`);
  return entfernt;
}

// Eine Abfrage fuer alle Quellen statt je Quelle ein findOne: der Server
// begrenzt Abfragen je Browser (Burst 32, 16/s, geteilt mit der Shell). 21
// Einzelabfragen beim Start liefen am 25.09.2026 in RATE_LIMITED, der Start
// begann dann endlos von vorn.
async function alleQuellDokumente() {
  const docs = await state.collections.sources.find().exec();
  return new Map((docs || []).map((doc) => [String(doc?.id ?? doc?.get?.('id') ?? doc?.toJSON?.().id ?? ''), doc]));
}

async function seedSources() {
  const now = Date.now();
  let adapterContractChanged = false;
  let quellDokumente = await alleQuellDokumente();
  try {
    if (await entferneScheinquellen(quellDokumente)) {
      adapterContractChanged = true;
      quellDokumente = await alleQuellDokumente();
    }
  } catch (error) {
    console.error('[outbound] Schein-Quellen konnten nicht entfernt werden', error);
  }
  for (const definition of SOURCE_DEFS) {
    const existing = quellDokumente.get(definition.id) || null;
    const builtinDefinition = {
      id: definition.id,
      label: definition.label,
      url: definition.url,
      countries: definition.countries,
      field_keys: definition.fieldKeys,
      requires_credential: Boolean(definition.credentialSecretName),
      credential_secret_name: definition.credentialSecretName,
      target_key: definition.targetKey,
    };
    if (existing) {
      const current = existing.toJSON?.() || existing;
      if (current.payload?.builtin !== false) {
        // Built-in bedeutet „mitgeliefert“, nicht „unveränderlich“. Beim
        // Upgrade ergänzen wir neue Pflichtabdeckung additiv, lassen aber die
        // vom Nutzer gepflegte URL, Bezeichnung, Aktivierung und Zugangsdaten
        // unangetastet. Eine vollständige Werkseinstellung würde dynamisch
        // editierbare Quellen beim nächsten Reload heimlich zurücksetzen.
        const ohne = new Set(ENTFERNTE_QUELLFELDER[definition.id] || []);
        const mergedFields = [...new Set([
          ...normalizeResearchFieldKeys(current.field_keys || []),
          ...definition.fieldKeys,
        ])].filter((key) => !ohne.has(key));
        const mergedCountries = [...new Set([...(current.countries || []), ...definition.countries])];
        const patch = {
          field_keys: mergedFields,
          countries: mergedCountries,
          payload: {
            ...(current.payload || {}),
            builtin: true,
            input_driven: definition.inputDriven,
            start_url_source: definition.startUrlSource,
            definition_version: '1.0.45',
            secret_value_in_payload: false,
          },
          updated_at_ms: now,
        };
        if (!String(current.label || '').trim()) patch.label = definition.label;
        // Nur setzen, wenn die Definition eine URL HAT: "impressum" hat keine,
        // und das leere patch.url galt bei JEDEM Start als Aenderung — mehrere
        // offene Browser schrieben die Quelle im Minutentakt (Klicktest P4 T18).
        if (!String(current.url || '').trim() && definition.url) patch.url = definition.url;
        if (!String(current.target_key || '').trim()) patch.target_key = definition.targetKey;
        if (typeof current.requires_credential !== 'boolean') {
          patch.requires_credential = Boolean(definition.credentialSecretName);
        }
        // Wechselt eine mitgelieferte Quelle den Zugangstyp (LinkedIn: vom
        // Browser-Login auf den Bright-Data-Schluessel), muss der Verweis
        // mitwandern — sonst zeigt die Zeile auf ein Geheimnis, das der
        // Datenzugriff gar nicht mehr verwendet (Owner 18.09.2026).
        const alterVerweis = String(current.credential_secret_name || '').trim();
        if (alterVerweis && definition.credentialSecretName
          && alterVerweis !== definition.credentialSecretName
          && (VERALTETE_ZUGANGSVERWEISE[definition.id] || []).includes(alterVerweis)) {
          patch.credential_secret_name = definition.credentialSecretName;
          patch.requires_credential = true;
          patch.auth_status = 'required';
        }
        // Unabhaengig vom Zugang: traegt die Quelle noch die alte
        // Standardbezeichnung, waehrend die Definition eine neue nennt, wird
        // sie nachgezogen. Sonst haengt die Umbenennung an einer Migration,
        // die schon gelaufen ist (Owner 18.09.2026: "ich sehe ihn noch nicht").
        if (VERALTETE_BEZEICHNUNGEN[definition.id]
          && String(current.label || '').trim() === VERALTETE_BEZEICHNUNGEN[definition.id]
          && definition.label !== VERALTETE_BEZEICHNUNGEN[definition.id]) {
          patch.label = definition.label;
        }
        if (!String(current.credential_secret_name || '').trim() && definition.credentialSecretName) {
          patch.credential_secret_name = definition.credentialSecretName;
          // Kommt die Zugangspflicht neu aus der Definition (Google ab
          // 1.0.179), muss sie auch gesetzt werden: sonst bleibt
          // requires_credential=false, die Zeile zeigt kein Zahnrad und der
          // Zugang laesst sich nicht hinterlegen. Nur hier, also nur wenn die
          // Quelle bisher ueberhaupt keinen Verweis trug — ein vom Owner
          // entfernter Zugang wird dadurch nicht wieder erzwungen.
          patch.requires_credential = true;
        }
        const definitionChanged = JSON.stringify(current.field_keys || []) !== JSON.stringify(mergedFields)
          || JSON.stringify(current.countries || []) !== JSON.stringify(mergedCountries)
          || current.payload?.definition_version !== '1.0.45'
          || Object.keys(patch).some((key) => !['field_keys', 'countries', 'payload', 'updated_at_ms'].includes(key));
        if (definitionChanged) {
          await existing.incrementalPatch(patch);
          adapterContractChanged = true;
        }
      }
      continue;
    }
    await state.collections.sources.insert({
      ...builtinDefinition,
      enabled: true,
      adapter_status: 'draft',
      scrape_status: 'target_available',
      auth_status: definition.credentialSecretName ? 'required' : 'not_required',
      payload: {
        builtin: true,
        input_driven: definition.inputDriven,
        start_url_source: definition.startUrlSource,
        definition_version: '1.0.45',
        secret_value_in_payload: false,
      },
      created_at_ms: now,
      updated_at_ms: now,
    });
    adapterContractChanged = true;
  }
  const sellifyVorhanden = quellDokumente.get(SELLIFY_SOURCE_ID) || null;
  if (!sellifyVorhanden) {
    await state.collections.sources.insert(sellifyQuellenEintrag(now));
  }
  return adapterContractChanged;
}

function sellifyQuellenEintrag(now = Date.now()) {
  return {
    id: SELLIFY_SOURCE_ID,
    label: SELLIFY_SOURCE_LABEL,
    url: '',
    countries: ['DE', 'AT', 'CH'],
    field_keys: ['Firma', 'Anschrift', 'Person', 'E-Mail', 'Telefon'],
    enabled: true,
    requires_credential: false,
    credential_secret_name: '',
    target_key: SELLIFY_SOURCE_ID,
    adapter_status: 'not_required',
    scrape_status: 'not_required',
    auth_status: 'not_required',
    payload: { builtin: true, internal: true, secret_value_in_payload: false },
    created_at_ms: now,
    updated_at_ms: now,
  };
}

function isInternalResearchSource(item) {
  return item?.id === SELLIFY_SOURCE_ID || item?.payload?.internal === true;
}

function listedSources() {
  const sellify = state.sources.find((item) => item.id === SELLIFY_SOURCE_ID) || sellifyQuellenEintrag();
  // "example.com" stand als Quelle in der Liste - ein Worker hatte ein Ziel
  // mit einer Dokumentationsadresse ausprobiert. Solche Eintraege sind keine
  // Quellen und werden nicht gezeigt.
  // Vom Nutzer selbst angelegte Quellen bleiben sichtbar, sonst waeren sie
  // weder pruef- noch loeschbar (UI-Test 11.09.2026: "angelegt", aber
  // unsichtbar). Neue Beispieladressen lehnt addSource ab.
  return [sellify, ...state.sources.filter((item) => item.id !== SELLIFY_SOURCE_ID
    && (item.payload?.builtin === false
      || !isDocumentationSourceKey(evidenceSourceKey({ source_id: item.id, source_url: item.url }))))];
}

function merkeSellifyProjektion(reachable, lead = null, firma = null) {
  state.sellifyMatch = {
    reachable: Boolean(reachable),
    leadId: lead?.id || '',
    contactId: firma?.contact_id || null,
    name: String(firma?.name || '').trim(),
  };
}

function sellifySourceStatus() {
  const eintrag = state.sources.find((item) => item.id === SELLIFY_SOURCE_ID);
  if (eintrag?.enabled === false) {
    return { code: 'disabled', label: tr('sourceDisabled', 'Deaktiviert — wird bei der Recherche übersprungen.'), chip: null };
  }
  const projectionDa = Boolean(state.sellifyCompanies?.find);
  const match = state.sellifyMatch;
  if (!projectionDa && match?.reachable !== true) {
    return { code: 'internal_unavailable', label: tr('sellifyProjectionMissing', 'CRM-Projektion nicht erreichbar'), chip: null };
  }
  const lead = selectedLead();
  if (lead && match?.leadId === lead.id && match.contactId) {
    return {
      code: 'internal_matched',
      label: `Bereit · Datensatz vorhanden (contact_id ${match.contactId})`,
      chip: null,
    };
  }
  if (lead && match?.leadId === lead.id) {
    return { code: 'internal_empty', label: 'Bereit · kein Datensatz zur Firma', chip: null };
  }
  return { code: 'internal_ready', label: 'Bereit · interne Quelle, keine Anmeldung', chip: null };
}

function bindCollections() {
  state.collectionReloader?.dispose();
  state.collectionBindingGeneration = (state.collectionBindingGeneration || 0) + 1;
  state.collectionReloader = createCollectionReloader({
    collections: state.collections,
    reload: (keys) => reload(keys),
    afterReload: (keys) => {
      if (keys.length === 1 && keys[0] === 'leads' && state.lastLeadReloadChanged === false && listLeads().length) return;
      render();
      if (!keys.includes('leads')) return;
      if (listLeads().length) state.nachladenFehlschlaege = 0;
      else planeLeerNachladen();
      void loadSelectedLeadDetails();
    },
    onError: (error) => {
      render();
      console.warn('[outbound-lead-generation] Nachladen fehlgeschlagen, neuer Versuch', { message: error?.message || String(error), collections: error?.details });
    },
  });
}

// Only invalidated collections are reloaded, with one serialized consumer.
// Explicit action reloads still read complete documents through the Shell's
// demand path; no projected document is used for provenance or a mutation.
// Eine abgebrochene Bedarfsabfrage liefert kein Fehlersignal, sondern eine
// leere Liste; ohne spaetere Datenaenderung blieb die App dann bei „Noch keine
// Kampagne“ stehen (30.09.2026). Leere oder gescheiterte Ergebnisse werden mit
// wachsendem Abstand (2 s bis 60 s) erneut geladen.
function planeLeerNachladen() {
  if (state.leerNachladenTimer) return;
  state.nachladenFehlschlaege = (state.nachladenFehlschlaege || 0) + 1;
  const warten = Math.min(60_000, 2_000 * 2 ** Math.min(state.nachladenFehlschlaege - 1, 5));
  state.leerNachladenTimer = globalThis.setTimeout(() => {
    state.leerNachladenTimer = null;
    scheduleCollectionReload(['leads']);
  }, warten);
}

function scheduleCollectionReload(keys) {
  state.collectionReloader?.request(keys);
}

function meldeAktionsfehler(event, error) {
  const aktion = event?.target?.closest?.('[data-action]')?.dataset?.action || 'unbekannt';
  const text = String(error?.message || error || 'Unbekannter Fehler');
  console.error('[olg] Aktion fehlgeschlagen', { aktion, text });
  const wartung = /wird aktualisiert|schreibgesch|read-?only|MAINTENANCE/i.test(text);
  const meldung = wartung
    ? `Das hat nicht geklappt: CTOX wird gerade aktualisiert und Apps sind vorübergehend schreibgeschützt. Bitte in ein paar Minuten erneut versuchen.\n\n(${text})`
    : `Das hat nicht geklappt: ${text}`;
  void showBusinessAlert(meldung);
}

function bindUi() {
  state.ctx.host.addEventListener('change', (event) => {
    const sortSel = event.target?.closest?.('[data-action="lead-sort"]');
    if (sortSel) { state.leadSortKey = sortSel.value || 'name'; renderCenter(); }
  });
  const host = state.ctx.host;
  // Antworten des CTOX-Agenten auf Sellify-Vermerkpruefungen einsammeln.
  if (!state.freitextTakt) state.freitextTakt = globalThis.setInterval(() => { void verarbeiteFreitextAntworten().catch(() => {}); }, 20_000);
  host.addEventListener('pointerdown', () => { state.sourcePanelPointerAt = Date.now(); }, true);
  host.addEventListener('click', (event) => {
    if (event.target?.closest?.('[data-action]')) state.sourcePanelUserActionAt = Date.now();
    // Jede Aktion ist async; ein Fehler (Wartung/Schreibschutz, abgerissene
    // Verbindung, Validierung) flog bisher als unbehandelte Ablehnung aus dem
    // Klick und der Nutzer sah nichts. Klicktest 11.09.2026: ~15 stille
    // Fehlschlaege, u. a. "Quelle ohne Zugang anlegen" und "Rechercheablauf
    // speichern" waehrend der Wartung (P4 T66/T68).
    return Promise.resolve(handleClick(event)).catch((error) => meldeAktionsfehler(event, error));
  });
  host.addEventListener('keydown', handleKeydown);
  host.addEventListener('input', (event) => {
    if (event.target.matches('[data-lead-search]')) {
      state.search = event.target.value;
      renderCenter();
    }
    if (event.target.matches('[data-sellify-campaign-query]')) {
      state.sellifyImportQuery = event.target.value;
    }
    if (event.target.matches('[data-source-search]')) {
      state.sourceSearch = event.target.value;
      // Waehrend des Tippens schreibt renderSourcePanel nicht neu (Fokus);
      // die Zeilen werden deshalb direkt gefiltert (Klicktest-Befund P4 V2).
      filtereQuellenZeilen();
      renderSourcePanel();
    }
    if (event.target.matches('[data-research-policy]')) {
      state.researchPolicyDraft = event.target.value;
    }
    if (event.target.matches('[data-research-policy-followup]')) {
      state.researchPolicyFollowupDraft = event.target.value;
    }
    if (event.target.matches('[data-digest-field], [data-digest-day]')) {
      updateDigestDraftFromForm(event.target);
    }
    if (event.target.matches('[data-digest-period]')) {
      state.digestPeriod = event.target.value;
    }
    if (event.target.matches('[data-lead-edit-field]') && state.leadDraft) {
      state.leadDraft[event.target.dataset.leadEditField] = event.target.value;
    }
  });
}

// Kompaktes Glossar der konfigurierten Quellen fuer den Agenten-Prompt:
// welche Quelle liefert welche Felder, und was ist ihr Zustand.
function quellenGlossar() {
  return state.sources
    .filter((item) => item.enabled !== false && !isInternalResearchSource(item))
    .map((item) => {
      const felder = (Array.isArray(item.field_keys) && item.field_keys.length ? item.field_keys : ['alle Felder']).slice(0, 10).join(', ');
      const zustand = item.scrape_status === 'succeeded' || item.adapter_status === 'active' ? 'bereit'
        : /auth/.test(String(item.auth_status || '')) && item.auth_status !== 'not_required' ? 'Anmeldung noetig'
        : String(item.scrape_status || '').includes('blocked') ? 'derzeit blockiert'
        : 'ungeprueft';
      return `- ${item.label || item.id} (${item.id}): ${felder} [${zustand}]`;
    })
    .join('\n') || '(keine aktiven Quellen)';
}

// Owner 23.09.2026: "fuer jedes Feld genuegt EINE passende belegte Quelle".
// Die Regel kam nur in den Standardtext; der auf Kundeninstanz gespeicherte Ablauf
// (v32, 23.09. 13:13) trug weiter die alte Zwei-Quellen-Regel, der Agent liess
// Registerfelder, WZ-Code und Land deshalb offen (25.09.2026). Ersetzt werden
// nur die beiden alten Zeilen, wortgleich; jede eigene Fassung bleibt stehen.
const VERALTETE_BELEGREGEL = Object.freeze(["5. Belegregel. Angaben, die Dritte unabhängig prüfen können, brauchen zwei unabhängige Quellen: firma_name, firma_anschrift, firma_plz, firma_ort, firma_land, firma_aktivitaetsstatus, firma_fruehere_namen, firma_geschaeftstaetigkeit, firma_geschaeftsfuehrung, firma_prokura, wz_code, umsatz, mitarbeiter. Zwei Seiten derselben Quelle sind eine Quelle, und Sellify allein belegt nichts.", "5a. Selbstauskünfte genügen mit EINER Quelle, weil es dafür keine zweite unabhängige geben kann: firma_domain, firma_email, firma_telefon, firma_fax, firma_postfach, firma_besucheranschrift, firma_postanschrift, firma_homepage_fact_sheet sowie alle person_-Felder. Beleg ist die Unternehmensseite (Impressum, Kontakt, Team) bzw. das Profil selbst, mit URL und wörtlichem Zitat. Einen so belegten Wert eintragen, niemals als no_match verwerfen mit der Begründung, er stehe nur auf der eigenen Website."]);
function aktuelleBelegregel() {
  const zeilen = DEFAULT_RESEARCH_POLICY.split('\n');
  return [zeilen.find((z) => z.startsWith('5. Belegregel')), zeilen.find((z) => z.startsWith('5a. '))];
}
function hebeBelegregelAn(text) {
  const [neu5, neu5a] = aktuelleBelegregel();
  if (!neu5 || !neu5a) return text;
  return String(text).split('\n').map((zeile) => (
    zeile === VERALTETE_BELEGREGEL[0] ? neu5 : zeile === VERALTETE_BELEGREGEL[1] ? neu5a : zeile
  )).join('\n');
}
function researchPolicyInstructions(policy) {
  const text = String(policy?.instructions || DEFAULT_RESEARCH_POLICY).trim() || DEFAULT_RESEARCH_POLICY;
  return hebeBelegregelAn(text);
}

// Eigener Prompt fuer die Nachrecherche (Owner-Vorgabe 31.08.). Leer heisst:
// der Prompt der Neuen Recherche gilt fuer beide Wege.
function followupResearchPolicyInstructions(policy) {
  return String(policy?.followup_instructions || '').trim();
}

function normalizeResearchFieldKeys(value) {
  const entries = Array.isArray(value) ? value : String(value || '').split(/[\n,;]+/);
  return [...new Set(entries
    .map((entry) => String(entry || '').trim().toLowerCase())
    .filter((entry) => /^[a-z][a-z0-9_]{1,63}$/.test(entry))
    .filter((entry) => RESEARCH_FIELD_SET.has(entry)))];
}

function activeResearchFields() {
  return state.researchFieldKeys.length ? [...state.researchFieldKeys] : [...RESEARCH_FIELDS];
}

function researchPolicyRecord(
  existing,
  instructions,
  now = Date.now(),
  fieldKeys = state.researchFieldKeysDraft,
  followupInstructions = state.researchPolicyFollowupDraft,
) {
  const currentVersion = Number(existing?.version_number);
  const minIndependentSources = Number(existing?.min_independent_sources);
  const createdAt = Number(existing?.created_at_ms);
  return {
    id: RESEARCH_POLICY_ID,
    title: String(existing?.title ?? 'CTOX Quellenstandard'),
    version_number: (Number.isFinite(currentVersion) ? currentVersion : 0) + 1,
    status: String(existing?.status ?? 'active'),
    skill_name: String(existing?.skill_name ?? 'outbound-lead-generation-research'),
    skill_version: String(existing?.skill_version ?? '1.0.0'),
    min_independent_sources: 1,
    rules: Array.isArray(existing?.rules) ? existing.rules : [],
    instructions: String(instructions || '').trim(),
    followup_instructions: String(followupInstructions || '').trim(),
    field_keys: normalizeResearchFieldKeys(fieldKeys).length
      ? normalizeResearchFieldKeys(fieldKeys)
      : [...RESEARCH_FIELDS],
    created_at_ms: Number.isFinite(createdAt) ? createdAt : now,
    updated_at_ms: now,
  };
}

// Explicit action reads may overlap. Results are ordered per collection so
// an older response cannot overwrite newer data or suppress unrelated data.
function reload(keys = Object.keys(state.collections)) {
  const lauf = (state.reloadLauf = (state.reloadLauf || 0) + 1);
  const promise = reloadAusfuehren(lauf, keys, state.collectionBindingGeneration);
  state.reloadPromise = promise;
  return promise;
}

// Die Business-OS-Datenschicht liefert je Abfrage hoechstens 200 Dokumente
// (DEFAULT_WINDOW_LIMIT), ohne Fehler. find() ohne Blaettern zeigte am
// 25.09.2026 193 von 209 Leads: Kampagnen wirkten unvollstaendig importiert
// (Maschinenbau 27/29, Chemie D 73/81). Geblaettert wird ueber den
// Primaerschluessel, nicht ueber skip: geloeschte Zeilen fallen clientseitig
// aus dem Fenster (200 -> 193), ein skip-Zaehler wuerde Leads ueberspringen.
const SEITENGROESSE = 200;
// Leads sind gross (~100 KB je Lead). Eine 200er-Seite ueber die
// Bedarfsabfrage waren ~20 MB in einer Antwort; am Kunden-Knoten lief sie in
// QUERY_COLLECTOR_TIMEOUT, reload() scheiterte, und die App zeigte „Noch keine
// Kampagne“ ueber einem vollstaendigen Datenbestand (30.09.2026).
const LEAD_SEITENGROESSE = 25;
async function alleDokumente(collection, selector = {}, seitengroesse = SEITENGROESSE) {
  const alle = [];
  const gesehen = new Set();
  let letzteId = '';
  for (let seite = 0; seite < 100; seite += 1) {
    const bedingung = letzteId ? { ...selector, id: { $gt: letzteId } } : { ...selector };
    const docs = await collection.find({ selector: bedingung, sort: [{ id: 'asc' }], limit: seitengroesse }).exec();
    let neu = 0;
    for (const doc of docs || []) {
      const daten = doc?.toJSON?.() || doc;
      const id = String(daten?.id || '');
      if (!id || gesehen.has(id)) continue;
      gesehen.add(id);
      alle.push(doc);
      neu += 1;
      if (id > letzteId) letzteId = id;
    }
    if (!neu) break;
  }
  return alle;
}

async function reloadAusfuehren(lauf, keys, bindingGeneration) {
  const collections = state.collections;
  const requested = [...new Set(keys)].filter((key) => collections[key]);
  let leadChanges = null;
  const previousLeads = state.leadHydrationBindingGeneration === bindingGeneration ? listLeads() : [];
  const outcomes = await Promise.allSettled(requested.map(async (key) => {
    const collection = collections[key];
    if (key === 'leads') {
      leadChanges = await withLeadQueryAuthority(state.ctx.sync,
        signal => loadLeadList(collection, previousLeads, { signal }), {
          isCurrent: () => state.collectionBindingGeneration === bindingGeneration,
        });
      return [key, leadChanges.rows];
    }
    const docs = await collection.find().exec();
    return [key, docs.map((doc) => doc.toJSON())];
  }));
  // Unmount or a recovered collection handle invalidates the old read. Order
  // results per collection: a newer source read must not discard a lead read.
  if (bindingGeneration !== state.collectionBindingGeneration) return;
  const applied = (state.reloadAngewendetJeSammlung ||= new Map());
  const readErrors = (state.collectionReadErrors ||= new Map());
  const failures = [];
  const results = [];
  for (let index = 0; index < outcomes.length; index++) {
    const key = requested[index];
    if (lauf < (applied.get(key) || 0)) continue;
    const outcome = outcomes[index];
    if (outcome.status === 'fulfilled') {
      results.push(outcome.value);
      readErrors.delete(key);
    } else {
      // Keep the last complete data, but never present a rejected read as
      // empty/successful. One failed collection must not discard healthy reads.
      applied.set(key, lauf);
      readErrors.set(key, String(outcome.reason?.message || outcome.reason));
      failures.push(key);
    }
  }
  try {
  const fresh = new Map(results.filter(([key]) => lauf >= (applied.get(key) || 0)));
  for (const key of fresh.keys()) applied.set(key, lauf);
  if (!fresh.size) return;
  const sources = fresh.get('sources');
  const adapters = fresh.get('adapters');
  const imports = fresh.get('imports');
  const researchPolicies = fresh.get('researchPolicies');
  const leads = fresh.get('leads');
  // Ein gerade umgeschalteter Schalter behaelt seine Stellung, bis der Abgleich
  // sie bestaetigt: ein Nachladen mit dem aelteren Serverstand liess ihn sonst
  // zuruckspringen, und der naechste Klick schaltete falsch herum (Rundgang
  // 25.09.2026). Nach zwei Minuten gilt wieder, was die Datenbank sagt.
  if (fresh.has('sources')) state.sources = sources.map((source) => {
    const absicht = state.sourceToggleIntent.get(source.id);
    if (!absicht) return source;
    if ((source.enabled !== false) === absicht.enabled || Date.now() > absicht.bis) {
      state.sourceToggleIntent.delete(source.id);
      return source;
    }
    return { ...source, enabled: absicht.enabled };
  }).sort((a, b) => a.label.localeCompare(b.label, 'de'));
  if (fresh.has('adapters')) state.adapters = adapters;
  if (fresh.has('researchPolicies')) {
  const policy = researchPolicies.find((item) => item.id === RESEARCH_POLICY_ID);
  state.researchPolicyRecord = policy || null;
  state.digestRecord = researchPolicies.find((item) => item.id === UPDATE_DIGEST_ID) || null;
  state.digestStatus = researchPolicies.find((item) => item.id === UPDATE_DIGEST_STATUS_ID) || null;
  if (!state.digestDirty) state.digestDraft = updateDigestDraftFrom(state.digestRecord);
  // Ungespeicherte Entwuerfe ueberleben das Nachladen. Vorher setzte jeder
  // Sync-Tick den Entwurf auf den gespeicherten Stand zurueck, und "Speichern"
  // speicherte still den alten Text (Klicktest-Befund P4 V7 / Inventar).
  const hatteStand = state.researchPolicyLoaded === true;
  const policyEntwurfOffen = hatteStand && state.researchPolicyDraft !== state.researchPolicy;
  const followupEntwurfOffen = hatteStand && state.researchPolicyFollowupDraft !== state.researchPolicyFollowup;
  const felderEntwurfOffen = hatteStand
    && JSON.stringify(state.researchFieldKeysDraft || []) !== JSON.stringify(state.researchFieldKeys || []);
  state.researchPolicy = researchPolicyInstructions(policy);
  if (!policyEntwurfOffen) state.researchPolicyDraft = state.researchPolicy;
  state.researchPolicyFollowup = followupResearchPolicyInstructions(policy);
  if (!followupEntwurfOffen) state.researchPolicyFollowupDraft = state.researchPolicyFollowup;
  state.researchPolicyLoaded = true;
  state.researchFieldKeys = normalizeResearchFieldKeys(policy?.field_keys).length
    ? normalizeResearchFieldKeys(policy.field_keys)
    : [...RESEARCH_FIELDS];
  if (!felderEntwurfOffen) state.researchFieldKeysDraft = [...state.researchFieldKeys];
  }
  if (fresh.has('imports')) state.imports = imports
    .filter((item) => item.id !== LEGACY_RESEARCH_POLICY_IMPORT_ID)
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms);
  if (fresh.has('leads')) {
    const sameBinding = state.leadHydrationBindingGeneration === bindingGeneration;
    state.leadHydrationBindingGeneration = bindingGeneration;
    state.lastLeadReloadChanged = Boolean(leadChanges.changedIds.size || leadChanges.removedIds.size);
    state.leadListRows = leads.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
    const revisions = new Map(leads.map(lead => [lead.id, lead._rev]));
    state.leads = sameBinding ? state.leads.filter(lead => revisions.get(lead.id) === lead._rev) : [];
    if (state.lastLeadReloadChanged) invalidateChangedRecipientEligibility(state.leads);
  }
  if (!fresh.has('leads') && !fresh.has('imports')) return;
  const campaigns = campaignRows();
  const nachImport = state.kampagneNachImport;
  if (nachImport && (Date.now() > nachImport.bis || campaigns.some((campaign) => campaign.name === nachImport.titel))) {
    if (Date.now() <= nachImport.bis) {
      state.selectedCampaign = nachImport.titel;
      state.selectedLeadId = campaignListLeads(nachImport.titel)[0]?.id || '';
    }
    state.kampagneNachImport = null;
  }
  // Ein Tick, der kurz KEINE Leads sieht (Replikation laeuft noch), verwarf
  // bisher gewaehlte Kampagne, Auswahl und Detail-Lead (Klicktest P2 V13).
  // Ohne Leads bleibt die Auswahl stehen, bis wieder Daten da sind.
  if (fresh.has('leads') && !leads.length && state.selectedCampaign && (state.syncPending || (state.syncWaitingCollections?.size || 0) > 0)) return;
  if (!state.selectedCampaign || !campaigns.some((campaign) => campaign.name === state.selectedCampaign)) {
    state.selectedCampaign = campaigns[0]?.name || '';
  }
  const selectedCampaignLeads = campaignListLeads(state.selectedCampaign);
  const campaignLeadIds = new Set(selectedCampaignLeads.map((lead) => lead.id));
  state.selectedLeadIds = new Set(
    [...state.selectedLeadIds].filter((id) => campaignLeadIds.has(id)),
  );
  if (!selectedCampaignLeads.some((lead) => lead.id === state.selectedLeadId)) {
    state.selectedLeadId = selectedCampaignLeads[0]?.id || '';
  }
  void loadSelectedLeadDetails();
  } finally {
    if (failures.length) {
      throw Object.assign(new Error('Daten konnten nicht geladen werden: ' + failures.join(', ')), {
        code: 'OUTBOUND_COLLECTION_READ_FAILED', failedKeys: failures,
        details: Object.fromEntries(failures.map(key => [key, readErrors.get(key)])),
      });
    }
  }
}

function listLeads() { return state.leadListRows || state.leads; }
function campaignListLeads(campaign) { return listLeads().filter(lead => leadKampagnen(lead).includes(campaign)); }

async function ensureFullLeads(ids, { fresh = false } = {}) {
  const generation = state.collectionBindingGeneration;
  const requested = [...new Set(ids.filter(Boolean))];
  const summaries = new Map(listLeads().map(row => [row.id, row]));
  const cached = new Map(state.leads.map(row => [row.id, row]));
  const missing = requested.filter(id => fresh || !cached.has(id) || cached.get(id)._rev !== summaries.get(id)?._rev);
  if (!missing.length) return requested.map(id => cached.get(id));
  const sequence = ++state.fullLeadReadSequence;
  const rows = await withLeadQueryAuthority(state.ctx.sync,
    signal => loadFullLeadRows(state.collections.leads, missing, { signal }), {
      isCurrent: () => state.collectionBindingGeneration === generation && state.uiMounted !== false,
    });
  if (generation !== state.collectionBindingGeneration || state.uiMounted === false) {
    throw new Error('Die CTOX-Verbindung hat sich geändert. Bitte die Aktion erneut versuchen.');
  }
  const currentSummaries = new Map(listLeads().map(row => [row.id, row]));
  for (const row of rows) {
    if (summaries.get(row.id)?._rev !== currentSummaries.get(row.id)?._rev
      && row._rev !== currentSummaries.get(row.id)?._rev) {
      throw new Error('Der Lead wurde während des Ladens aktualisiert. Bitte die Aktion erneut versuchen.');
    }
  }
  const current = new Map(state.leads.map(row => [row.id, row]));
  for (const row of rows) {
    if (sequence < (state.fullLeadAppliedSequence.get(row.id) || 0)) continue;
    state.fullLeadAppliedSequence.set(row.id, sequence);
    const full = normalizeLeadRecipientShape(row);
    current.set(row.id, full);
    if (state.leadListRows) {
      const index = state.leadListRows.findIndex(entry => entry.id === row.id);
      if (index >= 0) state.leadListRows[index] = leadListRow(full);
    }
  }
  state.leads = applyPendingLeadPatches([...current.values()]);
  return requested.map(id => state.leads.find(row => row.id === id));
}

async function loadSelectedLeadDetails() {
  const id = state.selectedLeadId;
  if (!id || !listLeads().some(row => row.id === id)) return;
  const generation = state.collectionBindingGeneration;
  const key = `${generation}:${id}:${listLeads().find(row => row.id === id)?._rev}`;
  state.selectedDetailRequestedKey = key;
  // Selected-lead revisions can change every second during research. Keep one
  // detail read in flight and coalesce changes instead of piling up full reads.
  if (state.selectedDetailLoadingKey) return;
  state.selectedDetailLoadingKey = key;
  try {
    await ensureFullLeads([id]);
    if (state.selectedLeadId !== id || generation !== state.collectionBindingGeneration) return;
    state.selectedDetailError = '';
    renderDetail();
    const lead = selectedLead();
    if (lead && !state.recipientEligibilityReady.has(id)) {
      void refreshLeadRecipientEligibility(lead)
        .then(() => { if (state.selectedLeadId === id && generation === state.collectionBindingGeneration) renderDetail(); })
        .catch(() => {});
    }
  } catch (error) {
    if (state.selectedLeadId === id && generation === state.collectionBindingGeneration) {
      state.selectedDetailError = String(error?.message || error);
      renderDetail();
    }
  } finally {
    if (state.selectedDetailLoadingKey === key) state.selectedDetailLoadingKey = '';
    if (state.selectedDetailRequestedKey !== key && state.uiMounted !== false) void loadSelectedLeadDetails();
  }
}

const FULL_SELECTION_ACTIONS = new Set(['reset-selection-research', 'research-selection',
  'move-selection-campaign', 'research-selection-new', 'research-selection-followup', 'export-selection-xlsx']);
const FULL_CAMPAIGN_ACTIONS = new Set(['rename-campaign', 'delete-campaign', 'research-campaign',
  'research-campaign-gaps', 'check-campaign-remarks', 'export-campaign-xlsx', 'recheck-sellify']);
const FULL_SINGLE_ACTIONS = new Set(['research-lead', 'research-lead-new', 'research-lead-followup',
  'research-lead-gaps', 'cancel-research', 'validate-lead', 'edit-lead', 'save-lead-editor',
  'approve-field', 'release-empty-field', 'unrelease-empty-field', 'toggle-contact-recipient',
  'export-lead-xlsx', 'sellify-update-only', 'sellify-update-campaign']);
async function prepareFullLeadAction(action, id, campaign) {
  let ids = [];
  if (FULL_SELECTION_ACTIONS.has(action)) ids = [...state.selectedLeadIds];
  else if (FULL_CAMPAIGN_ACTIONS.has(action)) ids = campaignListLeads(campaign).map(row => row.id);
  else if (FULL_SINGLE_ACTIONS.has(action)) ids = [id || state.selectedLeadId].filter(Boolean);
  if (!ids.length) return;
  const previousNotice = state.notice;
  const notice = `Vollständige Leads werden geladen (${ids.length}) …`;
  state.notice = notice;
  renderCenter();
  try {
    await ensureFullLeads(ids, { fresh: !action.startsWith('export-') });
    if (FULL_SELECTION_ACTIONS.has(action) && (ids.length !== state.selectedLeadIds.size || ids.some(id => !state.selectedLeadIds.has(id)))) {
      throw new Error('Die Auswahl hat sich während des Ladens geändert. Bitte die Aktion erneut ausführen.');
    }
    if (FULL_SINGLE_ACTIONS.has(action) && !id && ids[0] !== state.selectedLeadId) {
      throw new Error('Der ausgewählte Lead hat sich geändert. Bitte die Aktion erneut ausführen.');
    }
  } finally {
    if (state.notice === notice) state.notice = previousNotice;
    renderCenter();
  }
}

async function repairAdapterActivationDrift() {
  const sourceById = new Map(state.sources.map((source) => [source.id, source]));
  const mismatches = state.adapters.filter((adapter) => {
    const source = sourceById.get(adapter.source_id);
    return source && (
      (typeof source.enabled === 'boolean' && adapter.enabled !== source.enabled)
      || JSON.stringify(adapter.field_keys || []) !== JSON.stringify(source.field_keys || [])
      || JSON.stringify(adapter.countries || []) !== JSON.stringify(source.countries || [])
      || String(adapter.url || '') !== String(source.url || '')
      || Boolean(adapter.requires_credential) !== Boolean(source.requires_credential)
    );
  });
  if (!mismatches.length) return 0;
  const now = Date.now();
  await Promise.all(mismatches.map(async (adapter) => {
    const source = sourceById.get(adapter.source_id);
    const adapterDoc = await state.collections.adapters.findOne(adapter.id).exec();
    await adapterDoc?.incrementalPatch({
      enabled: source.enabled,
      field_keys: source.field_keys || [],
      countries: source.countries || [],
      url: source.url || '',
      requires_credential: Boolean(source.requires_credential),
      updated_at_ms: now,
    });
  }));
  return mismatches.length;
}

function captureScrollPositions() {
  const host = state.ctx?.host;
  if (!host) return;
  const campaigns = host.querySelector('[data-campaigns-pane] .leadgen-scroll');
  const leads = host.querySelector('[data-leads-pane] .leadgen-scroll');
  const detail = host.querySelector('[data-detail-pane] .leadgen-detail-body');
  if (campaigns) state.scrollPositions.campaigns = campaigns.scrollTop;
  if (leads) state.scrollPositions.leads = leads.scrollTop;
  if (detail && state.selectedLeadId) state.scrollPositions.detailByLead.set(state.selectedLeadId, detail.scrollTop);
}

function restoreScrollPositions() {
  const host = state.ctx?.host;
  if (!host) return;
  const campaigns = host.querySelector('[data-campaigns-pane] .leadgen-scroll');
  const leads = host.querySelector('[data-leads-pane] .leadgen-scroll');
  const detail = host.querySelector('[data-detail-pane] .leadgen-detail-body');
  if (campaigns) campaigns.scrollTop = state.scrollPositions.campaigns;
  if (leads) leads.scrollTop = state.scrollPositions.leads;
  if (detail && state.selectedLeadId) detail.scrollTop = state.scrollPositions.detailByLead.get(state.selectedLeadId) || 0;
}

function render() {
  // An initial/action read can resolve after the Shell has closed this App.
  // It must not recreate the removed layout in the former module host.
  if (state.uiMounted === false) return;
  captureScrollPositions();
  ensureLayoutSkeleton();
  renderSyncLine();
  renderCampaigns();
  renderCenter();
  renderDetail();
  renderSourcePanel();
  renderLeadEditor();
  restoreScrollPositions();
}

// Statisches Shell-V2-Skelett: wird genau EINMAL gebaut. Jede Spalte ist ein
// Grid aus zwei Header-Zeilen in Shell-Hoehe (--shell-v2-header-row-size),
// Scroll-Koerper und Fusszeile - exakt das Knowledge-Referenzmuster. Die
// Renderfunktionen schreiben nur noch in die Koerper-Container; Header werden
// per textContent aktualisiert. Damit gibt es genau EIN Header-System (das
// der Shell), einheitliche Zeilenhoehen ueber alle Spalten, keine
// Scroll-Spruenge und keine Animations-Neustarts durch innerHTML-Neubau.
function ensureLayoutSkeleton() {
  const host = state.ctx.host;
  if (host.querySelector('.outbound-lead-generation-layout')) return;
  host.innerHTML = `
    <div class="outbound-lead-generation-layout" data-resize-frame>
      <section class="ctox-pane leadgen-pane leadgen-campaigns" data-campaigns-pane>
        <header class="ctox-pane-header ctox-pane-band" data-shell-v2-header-row="1">
          <div class="ctox-pane-title-row">
            <div class="ctox-pane-titles"><h2 class="ctox-pane-title">${tr('campaigns', 'Kampagnen')}</h2><span class="leadgen-count" data-campaign-count>0</span></div>
            <div class="ctox-pane-actions"><button class="ctox-pane-icon" data-action="new-campaign" title="${tr('newCampaign', 'Neue Kampagne')}" aria-label="${tr('newCampaign', 'Neue Kampagne')}">${icon('plus')}</button></div>
          </div>
        </header>
        <div class="leadgen-toolbar leadgen-band" data-shell-v2-header-row="2">
          <button class="ctox-pane-icon" data-action="import-leads" title="${tr('importFile', 'Leads aus Datei importieren (Excel/CSV)')}" aria-label="${tr('importFile', 'Leads aus Datei importieren (Excel/CSV)')}">${icon('import')}</button>
          <!-- Owner 11.09.2026: der Sellify-Import war als "Anmelden"-Pfeil nicht zu erkennen. -->
          <button class="leadgen-sellify-import" type="button" data-action="import-sellify-campaign" title="${tr('sellifyImport', 'Leadliste einer Sellify-Kampagne importieren (Nachrecherche)')}" aria-label="${tr('sellifyImport', 'Leadliste einer Sellify-Kampagne importieren (Nachrecherche)')}"><span class="leadgen-sellify-mark" aria-hidden="true">S</span><span>${tr('sellifyImportShort', 'Aus Sellify')}</span></button>
          <!-- 25.09.2026: das namenlose Zahnrad wurde nicht gefunden ("wo sind die settings?"). -->
          <button class="leadgen-settings-button" type="button" data-action="open-sources" title="${tr('sourceSettings', 'Recherche-Einstellungen: Quellen, Rechercheablauf, Pflichtfelder, Update-Verteiler')}">${icon('settings')}<span>Einstellungen</span></button>
        </div>
        <div class="leadgen-scroll leadgen-campaign-list" data-campaigns-body></div>
        <footer class="ctox-pane-footer leadgen-pane-footer"><span data-sync-line role="status" aria-live="polite"></span></footer>
      </section>
      <button class="ctox-column-resizer leadgen-resizer" type="button" data-resizer="left" data-resizer-var="--leadgen-left" data-resizer-min="220" data-resizer-max="520" aria-label="Kampagnenbreite ändern"></button>
      <section class="ctox-pane leadgen-pane leadgen-leads" data-leads-pane>
        <header class="ctox-pane-header ctox-pane-band" data-shell-v2-header-row="1">
          <div class="ctox-pane-title-row">
            <div class="ctox-pane-titles"><h2 class="ctox-pane-title" data-center-title></h2><span class="leadgen-count" data-center-count></span></div>
            <div class="ctox-pane-actions" data-center-actions></div>
          </div>
        </header>
        <div class="leadgen-toolbar leadgen-band" data-shell-v2-header-row="2" data-center-toolbar></div>
        <div class="leadgen-center-tray" data-center-tray hidden></div>
        <div class="leadgen-center-status" data-center-status></div>
        <div class="leadgen-scroll" data-leads-body></div>
        <footer class="ctox-pane-footer leadgen-pane-footer"><span data-center-foot></span></footer>
      </section>
      <button class="ctox-column-resizer leadgen-resizer" type="button" data-resizer="right" data-resizer-var="--leadgen-right" data-resizer-min="340" data-resizer-max="620" aria-label="Detailbreite ändern"></button>
      <section class="ctox-pane leadgen-pane leadgen-detail" data-detail-pane>
        <header class="ctox-pane-header ctox-pane-band" data-shell-v2-header-row="1">
          <div class="ctox-pane-title-row">
            <div class="ctox-pane-titles"><span class="ctox-pane-kicker">Lead</span><h2 class="ctox-pane-title" data-detail-title>—</h2></div>
            <div class="ctox-pane-actions" data-detail-actions></div>
          </div>
        </header>
        <div class="leadgen-band leadgen-detail-tabs-row" data-shell-v2-header-row="2" data-detail-tabs></div>
        <div class="leadgen-scroll leadgen-detail-body" data-detail-body></div>
      </section>
    </div>
    <div data-source-panel></div>
    <div data-app-dialog></div>`;
}

// Sync-Zustand als Fusszeilen-Text statt als layoutverschiebendes Banner.
function renderSyncLine() {
  const line = state.ctx.host.querySelector('[data-sync-line]');
  if (!line) return;
  const waiting = state.syncWaitingCollections.size;
  if (state.collectionReadErrors?.size) {
    const labels = { leads: 'Leads', sources: 'Quellen', adapters: 'Adapter', imports: 'Kampagnen', researchPolicies: 'Einstellungen' };
    const failed = [...state.collectionReadErrors.keys()].map(key => labels[key] || key).join(', ');
    line.innerHTML = `${escapeHtml(failed)} konnten nicht geladen werden. Der vorhandene Stand bleibt erhalten. <button class="leadgen-approve-link" data-action="retry-sync">Neu verbinden</button>`;
    line.className = 'is-error';
  } else if (state.syncPending) {
    line.textContent = `${state.syncMessage || 'Daten werden verbunden'} (${REPLICATED_COLLECTIONS.length - waiting}/${REPLICATED_COLLECTIONS.length})`;
    line.className = 'is-syncing';
  } else if (state.syncError) {
    line.innerHTML = `${escapeHtml(state.syncMessage || 'Datenverbindung konnte nicht hergestellt werden.')} <button class="leadgen-approve-link" data-action="retry-sync">Neu verbinden</button>`;
    line.className = 'is-error';
  } else {
    line.textContent = '';
    line.className = '';
  }
}

// Ein Bereich wird nur neu geschrieben, wenn der Nutzer nicht gerade darin
// tippt - sonst verliert das Suchfeld bei jedem Sync-Tick den Fokus.
function regionHasFocus(node) {
  const active = state.ctx.host.ownerDocument?.activeElement || globalThis.document?.activeElement;
  if (!active || !node || !node.contains(active)) return false;
  // Nur TIPPEN schuetzt vor dem Neuschreiben. Chromium fokussiert Knoepfe und
  // Checkboxen beim Klick; dann blieb z. B. nach "Auswahl aufheben" die alte
  // Auswahlleiste stehen (Klicktest-Befund P2 V1) und "Sellify aktualisieren"
  // blieb nach dem Anhaken gesperrt, bis der Fokus wanderte (P3).
  const tag = String(active.tagName || '').toLowerCase();
  if (tag === 'textarea' || tag === 'select' || active.isContentEditable) return true;
  if (tag === 'input') {
    return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file']
      .includes(String(active.type || '').toLowerCase());
  }
  return false;
}

// Eine Firma kann in mehreren Sellify-Kampagnen stehen. Der Import legte sie
// nur einmal an und meldete sie in der zweiten Kampagne bloss als "bereits in
// ..." - die Kampagne "Maschinenbau - Welle 1 - 20.04.2025" zeigte deshalb 29
// statt 55 Firmen (26 standen in "Maschinenbau 2024 - Welle 2"; 26.09.2026).
// Die Heimatkampagne bleibt `campaign`; weitere Mitgliedschaften stehen in
// payload.weitere_kampagnen und zaehlen ueberall mit.
function heimatKampagne(lead) {
  return String(lead?.campaign || tr('annualResearch', 'Jahresrecherche')).trim();
}
function leadKampagnen(lead) {
  const weitere = Array.isArray(lead?.payload?.weitere_kampagnen) ? lead.payload.weitere_kampagnen : [];
  return [...new Set([heimatKampagne(lead), ...weitere.map((name) => String(name || '').trim()).filter(Boolean)])];
}
function campaignRows() {
  const counts = new Map();
  for (const lead of listLeads()) {
    for (const name of leadKampagnen(lead)) counts.set(name, (counts.get(name) || 0) + 1);
  }
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => a.name.localeCompare(b.name, 'de'));
}

// Every pane re-renders by rewriting innerHTML, which destroys the scroll
// containers and drops the reader back to the top on ANY click. Capture the
// offset of each `.leadgen-scroll` region before the rewrite and restore it
// afterwards, matched by position so a list keeps its place while its rows
// update around it.
function setPaneHtml(pane, html) {
  if (!pane) return;
  const previous = Array.from(pane.querySelectorAll('.leadgen-scroll')).map((node) => node.scrollTop);
  pane.innerHTML = html;
  if (!previous.length) return;
  pane.querySelectorAll('.leadgen-scroll').forEach((node, index) => {
    const offset = previous[index];
    if (typeof offset === 'number' && offset > 0) node.scrollTop = offset;
  });
}

function renderCampaigns() {
  const pane = state.ctx.host.querySelector('[data-campaigns-pane]');
  if (!pane) return;
  const campaigns = campaignRows();
  const count = pane.querySelector('[data-campaign-count]');
  if (count) count.textContent = String(campaigns.length);
  const body = pane.querySelector('[data-campaigns-body]');
  if (!body) return;
  const scrollTop = body.scrollTop;
  // Owner-Befund 18.09.2026: Die Liste wurde ~11x in 10 s komplett neu
  // geschrieben (gemessen im Owner-Tab). Jedes Neuschreiben ersetzt die
  // Knoepfe: der Zeiger verliert die Zeile (Papierkorb und Stift
  // verschwinden), und ein Klick zwischen Druecken und Loslassen faellt aus.
  // So wirkten Auswaehlen und Loeschen tot. Neu geschrieben wird nur noch,
  // wenn sich der sichtbare Inhalt wirklich geaendert hat — wie im
  // Quellen-Panel.
  const kampagnenHtml = `
      ${state.campaignMutationMessage ? `<div class="leadgen-empty" role="status">${escapeHtml(state.campaignMutationMessage)}</div>` : ''}
      ${campaigns.map((campaign) => `
        <div class="leadgen-campaign-row" data-selected="${campaign.name === state.selectedCampaign}">
          <button class="leadgen-campaign-select" data-action="select-campaign" data-campaign="${escapeHtml(campaign.name)}" aria-pressed="${campaign.name === state.selectedCampaign}">
            <span>${escapeHtml(campaign.name)}</span><strong>${campaign.count}</strong>
          </button>
          <div class="leadgen-campaign-actions">
            <button class="ctox-pane-icon" data-action="rename-campaign" data-campaign="${escapeHtml(campaign.name)}" title="Kampagne umbenennen" aria-label="${escapeHtml(campaign.name)} umbenennen">${icon('edit')}</button>
            <button class="ctox-pane-icon is-danger" data-action="delete-campaign" data-campaign="${escapeHtml(campaign.name)}" title="Kampagne „${escapeHtml(campaign.name)}“ löschen" aria-label="${escapeHtml(campaign.name)} löschen">${icon('trash')}</button>
          </div>
        </div>`).join('') || `<div class="leadgen-empty">${datenLadenNoch() ? 'Kampagnen werden geladen …' : tr('noCampaigns', 'Noch keine Kampagne.')}</div>`}`;
  if (state.kampagnenHtml === kampagnenHtml && body.childElementCount) return;
  state.kampagnenHtml = kampagnenHtml;
  body.innerHTML = kampagnenHtml;
  body.scrollTop = scrollTop;
}

function filtereQuellenZeilen() {
  const mount = state.ctx.host.querySelector('[data-source-panel]');
  if (!mount) return;
  const needle = String(state.sourceSearch || '').trim().toLowerCase();
  const eintraege = new Map(listedSources().map((item) => [item.id, item]));
  mount.querySelectorAll('.leadgen-source-row[data-source-id]').forEach((row) => {
    const item = eintraege.get(row.dataset.sourceId);
    const text = `${item?.label || ''} ${item?.url || ''}`.toLowerCase();
    row.hidden = Boolean(needle) && !text.includes(needle);
  });
}

function planeQuellenRender(ms) {
  if (state.sourcePanelRenderTimer) return;
  state.sourcePanelRenderTimer = globalThis.setTimeout(() => {
    state.sourcePanelRenderTimer = null;
    renderSourcePanel();
  }, Math.max(250, Number(ms) || 0));
}

function renderSourcePanel() {
  const mount = state.ctx.host.querySelector('[data-source-panel]');
  if (!mount) return;
  if (!state.sourcePanelOpen) {
    state.sourcePanelSignature = '';
    mount.replaceChildren();
    return;
  }
  // Jeder Sync-Tick rief render() -> renderSourcePanel() und schrieb den
  // OFFENEN Dialog komplett neu. Klicks verloren das Rennen gegen den
  // Rewrite (Toggle, Skript-Inspector, Loeschen wirkten tot), Scroll und
  // Fokus sprangen. Neu geschrieben wird nur, wenn sich der sichtbare
  // Inhalt tatsaechlich geaendert hat.
  // Der Secret-Katalog gehoert in die Signatur: ohne ihn blieb nach dem
  // Speichern eines Zugangs "Zugang fehlt" stehen (UI-Test 11.09.2026).
  const signatur = JSON.stringify([
    state.sourcePanelView, state.sourceSearch, state.adapterReconciliationPending,
    state.digestRecord?.updated_at_ms || 0, state.digestStatus?.updated_at_ms || 0,
    state.digestBusy || '', state.digestPreview?.subject || '', state.digestPreview?.error || '',
    state.researchPolicy === state.researchPolicyDraft,
    state.researchPolicyFollowup === state.researchPolicyFollowupDraft,
    state.secretKatalogStand || 0,
    [...optionalFieldsDraft()].sort().join(','),
    [...optionalResearchFields()].sort().join(','),
    // Uebertragungsstand der Zugaenge: ohne ihn blieb "wird uebertragen" bzw.
    // "NICHT uebertragen" unsichtbar (Klicktest P0e).
    [...(state.zugangUebertragung instanceof Map ? state.zugangUebertragung : new Map())].map(([name, eintrag]) => [name, eintrag?.fehler || 'laeuft']),
    state.sources.map((s2) => [s2.id, s2.enabled, s2.adapter_status, s2.scrape_status, s2.auth_status, s2.updated_at_ms]),
  ]);
  if (signatur === state.sourcePanelSignature && mount.childElementCount) return;
  // Nur TIPP-Fokus schuetzt vor dem Rewrite - ein fokussierter Button (nach
  // Klick) darf das Anzeigen des neuen Zustands nicht blockieren.
  const aktiv = mount.ownerDocument?.activeElement;
  const tippt = aktiv && mount.contains(aktiv) && /^(input|textarea|select)$/i.test(aktiv.tagName || '');
  if (tippt && mount.childElementCount) { state.sourcePanelSignature = ''; planeQuellenRender(1500); return; }
  // Laufende Recherchen aendern Adapter-Status im Sekundentakt; jeder Tick
  // schrieb den OFFENEN Dialog neu und Klicks verloren das Rennen zwischen
  // Druecken und Loslassen. Sync-getriebene Rewrites laufen deshalb
  // gedrosselt (alle 5 s, nie kurz nach einem Zeigerkontakt); eigene
  // Aktionen des Nutzers rendern sofort.
  if (mount.childElementCount) {
    const now = Date.now();
    const userAktion = now - Number(state.sourcePanelUserActionAt || 0) < 1200;
    const zeigerKontakt = now - Number(state.sourcePanelPointerAt || 0) < 1500;
    const letzterRewrite = Number(state.sourcePanelRenderedAt || 0);
    if (!userAktion && (zeigerKontakt || now - letzterRewrite < 5000)) {
      state.sourcePanelSignature = '';
      // Gedrosselt heisst verschoben, nicht verworfen: vorher kam der neue
      // Stand nie an, wenn danach kein Sync-Tick mehr folgte — eine
      // geloeschte Quelle blieb in der Liste stehen (UI-Test 11.09.2026).
      planeQuellenRender(zeigerKontakt ? 1600 : 5000 - (now - letzterRewrite));
      return;
    }
  }
  state.sourcePanelRenderedAt = Date.now();
  state.sourcePanelSignature = signatur;
  const needle = state.sourceSearch.trim().toLowerCase();
  const sources = listedSources().filter((item) => !needle || `${item.label} ${item.url}`.toLowerCase().includes(needle));
  const showingPolicy = state.sourcePanelView === 'policy';
  const showingDigest = state.sourcePanelView === 'digest';
  const showingPflicht = state.sourcePanelView === 'pflichtfelder';
  const panelCount = showingDigest
    ? updateDigestRecipients(state.digestDraft?.recipients).valid.length
    : showingPflicht ? requiredResearchFieldCount()
    : showingPolicy ? '' : sources.length;
  const panelTitle = showingDigest
    ? 'Update-Verteiler'
    : showingPflicht
    ? 'Pflichtfelder'
    : showingPolicy
    ? tr('researchPolicy', 'Rechercheablauf')
    : tr('sourcesAndAccounts', 'Quellen & Zugänge');
  setPaneHtml(mount, `
    <div class="leadgen-source-backdrop" data-action="close-sources">
      <section class="leadgen-source-panel" role="dialog" aria-modal="true" aria-label="${panelTitle}" data-source-dialog>
        <header class="ctox-pane-header ctox-pane-band leadgen-header">
          <div>${panelCount === '' ? '' : `<span class="ctox-pane-kicker">${panelCount}</span>`}<h2 class="ctox-pane-title">${panelTitle}</h2></div>
          <div class="leadgen-header-actions">
            ${!showingPolicy && !showingDigest && !showingPflicht ? `<button class="ctox-pane-icon" data-action="add-source" title="${tr('addSource', 'Quelle hinzufügen')}" aria-label="${tr('addSource', 'Quelle hinzufügen')}">${icon('plus')}</button>` : ''}
            <button class="ctox-pane-icon" data-action="close-sources" title="${tr('close', 'Schließen')}" aria-label="${tr('close', 'Schließen')}">${icon('close')}</button>
          </div>
        </header>
        <div class="leadgen-source-tabs" role="tablist" aria-label="${tr('sourceSettings', 'Recherche-Einstellungen')}">
          <button role="tab" aria-selected="${!showingPolicy && !showingDigest && !showingPflicht}" data-action="source-view" data-view="sources">${tr('sourcesAndAccounts', 'Quellen & Zugänge')}</button>
          <button role="tab" aria-selected="${showingPolicy}" data-action="source-view" data-view="policy">${tr('researchPolicy', 'Rechercheablauf')}</button>
          <button role="tab" aria-selected="${showingPflicht}" data-action="source-view" data-view="pflichtfelder">Pflichtfelder</button>
          <button role="tab" aria-selected="${showingDigest}" data-action="source-view" data-view="digest">Update-Verteiler</button>
        </div>
        ${showingPolicy || showingDigest || showingPflicht ? '' : `<div class="leadgen-toolbar"><input class="ctox-pane-search" data-source-search value="${escapeHtml(state.sourceSearch)}" placeholder="${tr('searchSource', 'Quelle suchen')}" /></div>`}
        ${showingDigest
          ? renderUpdateDigest()
          : showingPflicht
          ? `<div class="leadgen-scroll leadgen-policy-editor">${renderOptionalFieldSettings()}</div>`
          : showingPolicy
          ? renderResearchPolicy()
          : `<div class="leadgen-scroll leadgen-source-list">${listedSources().map((item) => {
            // Alle Zeilen stehen im DOM, die Suche blendet nur aus: sonst konnte
            // das Loeschen des Suchtexts waehrend des Tippens die Liste nicht
            // wieder erweitern (Nachtest P4 T07).
            const zeile = renderSourceRow(item);
            return sources.some((entry) => entry.id === item.id) ? zeile : zeile.replace('<div class="leadgen-source-row"', '<div class="leadgen-source-row" hidden');
          }).join('')}</div>`}
      </section>
    </div>`);
}

function requiredResearchFieldCount(optional = optionalFieldsDraft()) {
  return RESEARCH_FIELD_GROUPS.flatMap(group => group.fields).filter(([key]) => !optional.has(key)).length;
}

function renderOptionalFieldSettings() {
  const draft = optionalFieldsDraft();
  const gespeichert = optionalResearchFields();
  const geaendert = draft.size !== gespeichert.size || [...draft].some((key) => !gespeichert.has(key));
  return `<section class="leadgen-optional-fields" aria-label="Pflichtfelder">
    <label class="leadgen-policy-label">Pflichtfelder<span class="leadgen-policy-hint"> — angehakt = Pflicht: diese Felder müssen für die Freigabe geprüft sein. Nicht angehakte Felder sind optional; sie werden weiterhin recherchiert und belegte Werte an Sellify übertragen.</span></label>
    ${RESEARCH_FIELD_GROUPS.map((group) => `<fieldset class="leadgen-optional-group"><legend>${escapeHtml(group.label)}</legend>
      ${group.fields.map(([key, label]) => `<label class="leadgen-optional-field"><input type="checkbox" data-action="toggle-optional-field" data-field="${escapeHtml(key)}"${!draft.has(key) ? ' checked' : ''}> <span>${escapeHtml(label)}</span></label>`).join('')}
    </fieldset>`).join('')}
    <div class="leadgen-optional-actions">
      <span class="leadgen-muted">${requiredResearchFieldCount(draft)} Pflichtfelder${geaendert ? ' · nicht gespeichert' : ''}</span>
      <button class="ctox-button ctox-button--sm${geaendert ? ' is-primary' : ''}" data-action="save-optional-fields"${geaendert ? '' : ' disabled'}>Pflichtfelder speichern</button>
    </div>
  </section>`;
}

async function saveOptionalFields() {
  const draft = optionalFieldsDraft();
  const keys = [...draft].sort();
  const doc = await mitKanalHeilung(() => state.collections.researchPolicies.findOne(RESEARCH_POLICY_ID).exec(), 'save-optional-read');
  if (!doc) {
    showBusinessAlert('Der Rechercheablauf ist noch nicht geladen. Bitte kurz warten und erneut speichern.');
    return;
  }
  // Nur dieses Feld aendern: kein neuer Prompt, keine Version, kein
  // Adapter-Abgleich - die Adapter lesen die Pflichtfeld-Auswahl nicht.
  const jetzt = Date.now();
  await mitKanalHeilung(() => doc.incrementalPatch({ optional_field_keys: keys, updated_at_ms: jetzt }), 'save-optional-write');
  state.optionalFieldsSaved = { keys, at: jetzt };
  state.researchPolicyRecord = { ...(state.researchPolicyRecord || {}), optional_field_keys: keys, updated_at_ms: jetzt };
  state.optionalFieldsDraft = null;
  renderSourcePanel();
  render();
  showBusinessAlert(`${requiredResearchFieldCount(new Set(keys))} Pflichtfelder gespeichert. Nicht angehakte Felder bleiben optional.`);
}

// Ein Feld ohne gefundene Information fuer DIESEN Lead freigeben: es bleibt
// leer, blockiert die Freigabe aber nicht mehr. Nicht fuer Felder, ohne die
// die Kampagne nicht starten kann oder eine Owner-Regel verletzt waere.
async function setEmptyFieldRelease(fieldKey, freigeben) {
  const lead = selectedLead();
  if (!lead || !fieldKey || !leerFreigebbar(fieldKey)) return;
  if (freigeben && researchFieldValue(lead, fieldKey)) return;
  const bisher = new Set(lead.payload?.operator_released_empty_field_keys || []);
  if (freigeben) bisher.add(fieldKey); else bisher.delete(fieldKey);
  const aenderung = {
    operator_released_empty_field_keys: [...bisher].sort(),
    operator_released_empty_updated_at_ms: Date.now(),
  };
  // Sofort anzeigen, aber bei abgelehntem Speichern den Vorzustand
  // zurueckholen: patchLead nahm seinen Ruecksprungpunkt aus dem bereits
  // geaenderten Stand, eine abgelehnte Freigabe blieb lokal aktiv (Issue 226).
  const eintrag = state.leads.find((entry) => entry.id === lead.id);
  const vorher = eintrag ? eintrag.payload : undefined;
  if (eintrag) eintrag.payload = { ...(eintrag.payload || {}), ...aenderung };
  renderCenter();
  renderDetail();
  try {
    // Nur die eigenen Schluessel, zusammengefuehrt mit dem aktuellen Dokument:
    // ein zwischenzeitlicher Writeback am Payload bleibt erhalten.
    await patchLead(lead.id, { payload: aenderung }, { payloadMerge: true });
  } catch (error) {
    const zurueck = state.leads.find((entry) => entry.id === lead.id);
    if (zurueck) zurueck.payload = vorher;
    state.pendingLeadPatches.delete(lead.id);
    renderCenter();
    renderDetail();
    throw new Error(`${freigeben ? 'Freigabe' : 'Rücknahme'} wurde nicht gespeichert: ${error?.message || error}`);
  }
}

function renderResearchPolicy() {
  const record = state.researchPolicyRecord || {};
  const reconciliationStatus = String(record.reconciliation_status || '').trim();
  const reconciliationError = String(record.reconciliation_error || '').trim();
  const reconciliationTaskId = String(record.reconciliation_task_id || '').trim();
  const reconciliationLabel = !ADAPTER_RECONCILIATION_VIA_WORKER
    ? 'Der gespeicherte Rechercheablauf gilt ab dem nächsten Recherchestart.'
    : state.adapterReconciliationPending
    ? 'Adapter-Abgleich wird übergeben …'
    : reconciliationStatus === 'completed'
      ? 'Alle Adapter wurden für diesen Rechercheablauf abgeglichen.'
      : reconciliationStatus === 'queued' || reconciliationStatus === 'running'
        ? 'Adapter-Abgleich läuft.'
        : reconciliationStatus === 'failed'
          ? `Adapter-Abgleich fehlgeschlagen${reconciliationError ? `: ${reconciliationError}` : '.'}`
          : 'Beim Speichern werden alle Adapter automatisch geprüft und aktualisiert.';
  return `<section class="leadgen-policy-editor">
    <label class="leadgen-policy-label">${tr('policyNew', 'Prompt: Neue Recherche')}</label>
    <textarea data-research-policy placeholder="${state.researchPolicyLoaded ? '' : 'Prompt wird geladen – Synchronisierung läuft …'}" aria-label="${tr('policyNew', 'Prompt: Neue Recherche')}">${escapeHtml(state.researchPolicyDraft)}</textarea>
    <label class="leadgen-policy-label">${tr('policyFollowup', 'Prompt: Nachrecherche')}<span class="leadgen-policy-hint"> — ${tr('policyFollowupHint', 'leer = Prompt der Neuen Recherche wird verwendet')}</span></label>
    <textarea data-research-policy-followup placeholder="${tr('policyFollowupHint', 'leer = Prompt der Neuen Recherche wird verwendet')}" aria-label="${tr('policyFollowup', 'Prompt: Nachrecherche')}">${escapeHtml(state.researchPolicyFollowupDraft || '')}</textarea>
    <p class="leadgen-muted">Welche Felder für die Freigabe Pflicht sind, steht im Reiter „Pflichtfelder“.</p>
    <div class="leadgen-adapter-state ${reconciliationStatus === 'completed' ? 'is-ready' : ''}" role="status">
      ${reconciliationStatus === 'completed' ? icon('check') : '<i></i>'}${escapeHtml(reconciliationLabel)}
      ${reconciliationTaskId ? `<button class="ctox-button ctox-button--sm" data-action="track-task" data-task-id="${escapeHtml(reconciliationTaskId)}" data-command-id="${escapeHtml(record.reconciliation_command_id || '')}">Vorgang öffnen</button>` : ''}
    </div>
    <footer>
      <button class="ctox-button" data-action="reset-policy">${tr('reset', 'Zurücksetzen')}</button>
      <button class="ctox-button is-primary" data-action="save-policy">${icon('check')}<span>${tr('save', 'Speichern')}</span></button>
    </footer>
  </section>`;
}

// ---------------------------------------------------------------------------
// Update-Verteiler (Owner 22.09.2026): eine oder mehrere Adressen bekommen zu
// den gewaehlten Zeiten, z. B. werktags 07:00, ein Update aus dieser App:
// was seit dem letzten Update recherchiert und an Sellify uebergeben wurde und
// was blockiert. Den Bericht baut und versendet der CTOX-Dienst aus den
// Datensaetzen dieser App; hier wird nur konfiguriert, angesehen und getestet.

function updateDigestDraftFrom(record) {
  const cfg = record?.update_digest || {};
  const weekdays = Array.isArray(cfg.weekdays) ? cfg.weekdays.map(Number).filter((d) => d >= 1 && d <= 7) : [1, 2, 3, 4, 5];
  return {
    enabled: cfg.enabled === true,
    recipients: Array.isArray(cfg.recipients) ? cfg.recipients.join('\n') : String(cfg.recipients || ''),
    weekdays: record ? weekdays : [1, 2, 3, 4, 5],
    time: /^\d{2}:\d{2}$/.test(String(cfg.time || '')) ? cfg.time : '07:00',
    timezone: UPDATE_DIGEST_TIMEZONES.some(([id]) => id === cfg.timezone) ? cfg.timezone : 'Europe/Berlin',
    sender_email: String(cfg.sender_email || ''),
  };
}

function updateDigestRecipients(text) {
  const valid = [];
  const invalid = [];
  for (const teil of String(text || '').split(/[\s,;]+/)) {
    const adresse = teil.trim().toLowerCase();
    if (!adresse) continue;
    if (/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(adresse)) {
      if (!valid.includes(adresse)) valid.push(adresse);
    } else invalid.push(teil.trim());
  }
  return { valid, invalid };
}

function updateDigestConfigFromDraft(draft = state.digestDraft || updateDigestDraftFrom(null)) {
  return {
    enabled: draft.enabled === true,
    recipients: updateDigestRecipients(draft.recipients).valid,
    weekdays: [...new Set((draft.weekdays || []).map(Number))].filter((d) => d >= 1 && d <= 7).sort((a, b) => a - b),
    time: draft.time || '07:00',
    timezone: draft.timezone || 'Europe/Berlin',
    sender_email: String(draft.sender_email || '').trim().toLowerCase(),
  };
}

function updateDigestDraftFromForm(target) {
  const draft = state.digestDraft || updateDigestDraftFrom(state.digestRecord);
  if (target.matches('[data-digest-day]')) {
    const day = Number(target.dataset.digestDay);
    const days = new Set(draft.weekdays || []);
    if (target.checked) days.add(day); else days.delete(day);
    draft.weekdays = [...days].sort((a, b) => a - b);
  } else {
    const field = target.dataset.digestField;
    draft[field] = target.type === 'checkbox' ? target.checked : target.value;
  }
  state.digestDraft = draft;
  state.digestDirty = true;
}

function updateDigestScheduleText(config) {
  const tage = config.weekdays.length === 5 && config.weekdays.every((d, i) => d === i + 1)
    ? 'werktags'
    : config.weekdays.length === 7
      ? 'täglich'
      : config.weekdays.map((d) => UPDATE_DIGEST_WEEKDAYS.find(([id]) => id === d)?.[1]).filter(Boolean).join(', ');
  return tage ? `${tage} um ${config.time} Uhr` : 'an keinem Tag';
}

function updateDigestFehlertext(roh) {
  const text = String(roh || '').replace(/\s+/g, ' ').trim();
  return text.length > 400 ? `${text.slice(0, 400)} …` : text;
}

function updateDigestStatusText() {
  const status = state.digestStatus || {};
  const zeit = (ms) => new Date(ms).toLocaleString('de-DE', { weekday: 'short', day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
  const zeilen = [];
  if (Number(status.last_sent_at_ms) > 0) {
    const an = Array.isArray(status.last_recipients) ? status.last_recipients.length : 0;
    zeilen.push(`Zuletzt planmäßig gesendet: ${zeit(Number(status.last_sent_at_ms))}${an ? ` an ${an} ${an === 1 ? 'Empfänger' : 'Empfänger'}` : ''}.`);
  }
  if (Number(status.last_test_at_ms) > 0) {
    zeilen.push(`Test ${status.last_test_ok ? 'gesendet' : 'fehlgeschlagen'}: ${zeit(Number(status.last_test_at_ms))}.`);
  }
  if (status.last_error) zeilen.push(`Letzter Fehler: ${updateDigestFehlertext(status.last_error)}`);
  return zeilen;
}

function renderUpdateDigest() {
  const draft = state.digestDraft || updateDigestDraftFrom(state.digestRecord);
  state.digestDraft = draft;
  const config = updateDigestConfigFromDraft(draft);
  const { invalid } = updateDigestRecipients(draft.recipients);
  const busy = state.digestBusy || '';
  const statusZeilen = updateDigestStatusText();
  const plan = state.digestRecord?.update_digest?.enabled === true
    ? `Aktiv: ${updateDigestScheduleText(updateDigestConfigFromDraft(updateDigestDraftFrom(state.digestRecord)))}.`
    : 'Noch nicht aktiv.';
  const preview = state.digestPreview;
  return `<section class="leadgen-digest" aria-label="Update-Verteiler">
    <div class="leadgen-digest-scroll">
      <p class="leadgen-digest-intro">CTOX schickt zu den gewählten Zeiten eine Zusammenfassung dieser App: abgeschlossene Recherchen, belegte Felder, Übergaben an Sellify und was blockiert — jeweils seit dem letzten Update.</p>
      <label class="leadgen-digest-switch">
        <input type="checkbox" data-digest-field="enabled" ${draft.enabled ? 'checked' : ''} />
        <span>Update-Verteiler aktiv</span>
      </label>
      <label class="leadgen-digest-field">
        <span>Empfänger</span>
        <textarea data-digest-field="recipients" rows="3" spellcheck="false" placeholder="name@firma.de — eine Adresse pro Zeile">${escapeHtml(draft.recipients)}</textarea>
        ${invalid.length ? `<small class="leadgen-digest-warn">Keine gültige Adresse: ${escapeHtml(invalid.join(', '))}</small>` : ''}
      </label>
      <fieldset class="leadgen-digest-field leadgen-digest-days">
        <legend>Wochentage</legend>
        <div>${UPDATE_DIGEST_WEEKDAYS.map(([day, label]) => `<label class="leadgen-digest-day"><input type="checkbox" data-digest-day="${day}" ${config.weekdays.includes(day) ? 'checked' : ''} /><span>${label}</span></label>`).join('')}</div>
      </fieldset>
      <div class="leadgen-digest-row">
        <label class="leadgen-digest-field">
          <span>Uhrzeit</span>
          <input type="time" data-digest-field="time" value="${escapeHtml(draft.time)}" step="300" />
        </label>
        <label class="leadgen-digest-field">
          <span>Zeitzone</span>
          <select data-digest-field="timezone">${UPDATE_DIGEST_TIMEZONES.map(([id, label]) => `<option value="${id}" ${draft.timezone === id ? 'selected' : ''}>${escapeHtml(label)}</option>`).join('')}</select>
        </label>
      </div>
      <label class="leadgen-digest-field">
        <span>Absender-Postfach</span>
        <input type="email" data-digest-field="sender_email" value="${escapeHtml(draft.sender_email)}" placeholder="leer = Postfach der CTOX-Instanz" spellcheck="false" />
        <small>Ein in der Mail-App verbundenes Postfach, z. B. das persönliche Exchange-Postfach.</small>
      </label>
      <label class="leadgen-digest-field">
        <span>Zeitraum für Vorschau und „Jetzt senden“</span>
        <select data-digest-period>${UPDATE_DIGEST_PERIODS.map(([id, label]) => `<option value="${id}" ${(state.digestPeriod || 'letzte') === id ? 'selected' : ''}>${escapeHtml(label)}</option>`).join('')}</select>
        <small>Der geplante Versand berichtet immer über die Zeit seit der letzten Mail.</small>
      </label>
      <div class="leadgen-digest-state" role="status">
        <strong>${escapeHtml(plan)}</strong>
        ${statusZeilen.map((zeile) => `<span>${escapeHtml(zeile)}</span>`).join('')}
      </div>
      ${preview ? `<div class="leadgen-digest-preview">
        <header><span>${preview.error ? 'Vorschau nicht möglich' : preview.sent ? 'Gesendet' : 'Vorschau'}</span><button class="ctox-pane-icon" data-action="digest-preview-close" title="Vorschau schließen" aria-label="Vorschau schließen">${icon('close')}</button></header>
        ${preview.error
          ? `<p class="leadgen-digest-warn">${escapeHtml(preview.error)}</p>`
          : `<dl>
              <dt>Von</dt><dd>${escapeHtml(preview.sender || 'Postfach der CTOX-Instanz')}</dd>
              <dt>An</dt><dd>${escapeHtml((preview.recipients || []).join(', ') || '— noch keine Empfänger —')}</dd>
              <dt>Betreff</dt><dd>${escapeHtml(preview.subject || '')}</dd>
            </dl>
            ${preview.html
              ? `<iframe class="leadgen-digest-frame" title="Vorschau der Update-Mail" sandbox="" srcdoc="${escapeHtml(preview.html)}"></iframe>`
              : `<pre>${escapeHtml(preview.body || '')}</pre>`}`}
      </div>` : ''}
    </div>
    <footer>
      <button class="ctox-button" data-action="digest-preview" ${busy ? 'disabled' : ''}>${busy === 'preview' ? 'Vorschau lädt …' : 'Vorschau'}</button>
      <button class="ctox-button" data-action="digest-send" ${busy || !config.recipients.length ? 'disabled' : ''}>${busy === 'send' ? 'Sendet …' : 'Jetzt senden'}</button>
      <button class="ctox-button is-primary" data-action="digest-save" ${busy ? 'disabled' : ''}>${icon('check')}<span>${busy === 'save' ? 'Speichert …' : tr('save', 'Speichern')}</span></button>
    </footer>
  </section>`;
}

async function saveUpdateDigest() {
  const draft = state.digestDraft || updateDigestDraftFrom(state.digestRecord);
  const config = updateDigestConfigFromDraft(draft);
  const { invalid } = updateDigestRecipients(draft.recipients);
  if (invalid.length) {
    await showBusinessAlert(`Diese Empfänger sind keine gültigen Adressen: ${invalid.join(', ')}`);
    return;
  }
  if (config.sender_email && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(config.sender_email)) {
    await showBusinessAlert('Das Absender-Postfach ist keine gültige Adresse.');
    return;
  }
  if (config.enabled && !config.recipients.length) {
    await showBusinessAlert('Für einen aktiven Update-Verteiler braucht es mindestens einen Empfänger.');
    return;
  }
  if (config.enabled && !config.weekdays.length) {
    await showBusinessAlert('Bitte mindestens einen Wochentag wählen.');
    return;
  }
  state.digestBusy = 'save';
  renderSourcePanel();
  try {
    const now = Date.now();
    await mitKanalHeilung(async () => {
      const doc = await state.collections.researchPolicies.findOne(UPDATE_DIGEST_ID).exec();
      const record = {
        id: UPDATE_DIGEST_ID,
        title: 'Update-Verteiler',
        version_number: Number(doc?.version_number || 0) + 1,
        status: 'digest_config',
        skill_name: '',
        skill_version: '',
        min_independent_sources: 0,
        rules: [],
        update_digest: config,
        created_at_ms: Number(doc?.created_at_ms) || now,
        updated_at_ms: now,
      };
      if (doc) await doc.incrementalPatch(record);
      else await state.collections.researchPolicies.insert(record);
    }, 'save-update-digest');
    state.digestDirty = false;
    state.digestRecord = { ...(state.digestRecord || {}), id: UPDATE_DIGEST_ID, update_digest: config, updated_at_ms: now };
    zeigeHinweis(config.enabled
      ? `Update-Verteiler gespeichert: ${updateDigestScheduleText(config)} an ${config.recipients.length} ${config.recipients.length === 1 ? 'Empfänger' : 'Empfänger'}.`
      : 'Update-Verteiler gespeichert (nicht aktiv).');
  } finally {
    state.digestBusy = '';
    state.sourcePanelUserActionAt = Date.now();
    renderSourcePanel();
  }
}

// Owner 28.09.2026: eine erneut gesendete Mail soll das ganze Wochenende
// abdecken. Nur Vorschau und „Jetzt senden“ waehlen den Zeitraum; der
// geplante Versand bleibt bei „seit der letzten Mail“ (CTOX prueft die Grenze
// von 31 Tagen).
const UPDATE_DIGEST_PERIODS = Object.freeze([
  ['letzte', 'seit der letzten Mail'],
  ['wochenende', 'seit Samstag 00:00'],
  ['woche', 'die letzten 7 Tage'],
]);
function updateDigestSinceMs(period = state.digestPeriod, now = new Date()) {
  if (period === 'woche') return now.getTime() - 7 * 24 * 60 * 60 * 1000;
  if (period === 'wochenende') {
    const samstag = new Date(now);
    samstag.setHours(0, 0, 0, 0);
    // getDay(): 0 = Sonntag, 6 = Samstag; zurueck zum juengsten Samstag.
    samstag.setDate(samstag.getDate() - ((samstag.getDay() + 1) % 7));
    return samstag.getTime();
  }
  return 0;
}

async function runUpdateDigest(dryRun) {
  const config = updateDigestConfigFromDraft();
  if (!dryRun) {
    if (!config.recipients.length) {
      await showBusinessAlert('Bitte zuerst mindestens einen Empfänger eintragen.');
      return;
    }
    const ok = await showBusinessConfirm(`Das aktuelle Update jetzt an ${config.recipients.join(', ')} senden?`, { title: 'Update senden', confirmLabel: 'Senden', kind: 'confirm' });
    if (!ok) return;
  }
  state.digestBusy = dryRun ? 'preview' : 'send';
  state.sourcePanelUserActionAt = Date.now();
  renderSourcePanel();
  const commandId = `cmd_leadgen_update_digest_${crypto.randomUUID()}`;
  try {
    const receipt = await sendeBefehl({
      id: commandId,
      command_id: commandId,
      module: 'outbound',
      command_type: 'outbound.update_digest.send_now',
      record_id: UPDATE_DIGEST_ID,
      inbound_channel: 'business_os.outbound_lead_generation',
      payload: {
        dry_run: dryRun,
        update_digest: config,
        ...(updateDigestSinceMs() ? { since_ms: updateDigestSinceMs() } : {}),
      },
      client_context: { source_module: 'outbound-lead-generation', record_id: UPDATE_DIGEST_ID, business_chat_auto_focus: false },
    }, { until: 'terminal', timeoutMs: dryRun ? 45_000 : 120_000, sync_queue_tasks: false });
    const result = receipt?.result?.result || receipt?.result || receipt;
    if (result?.ok !== true) throw new Error(result?.error || result?.message || 'CTOX hat das Update nicht bestätigt.');
    state.digestPreview = {
      sent: !dryRun,
      subject: result.subject || '',
      body: result.body || state.digestPreview?.body || '',
      html: result.html || (dryRun ? '' : state.digestPreview?.html || ''),
      sender: result.sender || config.sender_email || '',
      recipients: result.recipients || config.recipients,
    };
    if (!dryRun) zeigeHinweis(`Update gesendet an ${(result.recipients || config.recipients).length} Empfänger.`);
  } catch (error) {
    state.digestPreview = { error: updateDigestFehlertext(error?.message || error) };
  } finally {
    state.digestBusy = '';
    state.sourcePanelUserActionAt = Date.now();
    renderSourcePanel();
    // Die Vorschau steht unter dem Formular; ohne Scrollen sah man nach dem
    // Klick keine Reaktion (Rundgang 28.09.2026).
    globalThis.requestAnimationFrame?.(() => {
      state.ctx?.host?.querySelector?.('.leadgen-digest-preview')?.scrollIntoView?.({ block: 'nearest', behavior: 'smooth' });
    });
  }
}

function sourceStatus(item, adapter, now = Date.now()) {
  if (isInternalResearchSource(item)) return sellifySourceStatus();
  const status = String(adapter?.status ?? item?.adapter_status ?? '').toLowerCase();
  const scrapeStatus = String(adapter?.scrape_status ?? item?.scrape_status ?? '').toLowerCase();
  const authStatus = String(adapter?.auth_status ?? item?.auth_status ?? '').toLowerCase();
  const lastError = String(adapter?.last_error ?? '').trim();
  const updatedAt = Number(adapter?.updated_at_ms ?? item?.updated_at_ms) || 0;
  const requiresCredential = Boolean(item?.requires_credential);
  const checkedAt = updatedAt ? new Date(updatedAt).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }) : '';

  // Der Zugangs-Chip speist sich aus denselben persistierten Feldern wie die Statuszeile.
// Maschinenmeldungen in Nutzersprache. Unbekanntes wird gekuerzt, nie erfunden:
// steht hier nichts Passendes, bleibt der Satz allgemein und die Rohmeldung
// steht ausgeklappt darunter.
function fehlerInKlartext(rohtext) {
  const text = String(rohtext || '').trim();
  if (!text) return '';
  const klein = text.toLowerCase();
  if (klein.includes('was not found for this tenant') || (klein.includes('404') && klein.includes('llm.ctox.dev'))) {
    return 'der interne Sprachdienst hat die Anfrage nicht wiedergefunden';
  }
  if (klein.includes('temporary_unreachable') || klein.includes('temporarily unreachable')) return 'die Quelle war vorübergehend nicht erreichbar';
  if (klein.includes('timed out') || klein.includes('timeout') || klein.includes('zeitüberschreitung')) {
    return 'Zeitüberschreitung';
  }
  if (klein.includes('captcha')) return 'die Quelle verlangt eine Captcha-Freigabe';
  if (klein.includes('429') || klein.includes('rate limit') || klein.includes('too many requests')) {
    return 'die Quelle meldet zu viele Anfragen';
  }
  if (klein.includes('403') || klein.includes('forbidden')) return 'die Quelle hat den Zugriff verweigert';
  if (klein.includes('401') || klein.includes('unauthorized')) return 'die Quelle verlangt eine Anmeldung';
  if (klein.includes('5xx') || klein.includes('502') || klein.includes('503') || klein.includes('500')) {
    return 'die Quelle war nicht erreichbar';
  }
  if (klein.includes('dns') || klein.includes('enotfound') || klein.includes('econnrefused')) {
    return 'die Quelle war nicht erreichbar';
  }
  // Klicktest P4 T49/T17: Server-Antworten, die sonst als Rohtext oder
  // "Grund unbekannt" erschienen.
  if (klein.includes('no registered script')) return 'für diese Quelle ist noch kein Skript registriert';
  if (klein.includes('queue lease') || klein.includes('adapter_reconcile')) return 'der Adapter-Abgleich konnte nicht laufen (interner Warteschlangenfehler)';
  if (klein.includes('peer-not-open') || klein.includes('no authenticated webrtc peer') || klein.includes('rc_push')) {
    return 'die Verbindung dieses Browsers zu CTOX war unterbrochen – bitte die Seite neu laden';
  }
  // Unbekannt: ersten Satz zeigen, ohne Klammergeruest und ohne Kennungen.
  const kern = text
    .replace(/[A-Za-z]+\s*\{[^}]*\}/g, ' ')
    .replace(/cf-ray:[^,}]*/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  const satz = kern.split(/[.:]\s/)[0] || kern;
  return satz.length > 90 ? `${satz.slice(0, 87)}…` : satz;
}

  // credential_present und auth_verified_at_ms existieren serverseitig noch nicht (Spec §1.2)
  // und werden daher nie als bewiesen dargestellt.
  // Ist der Secret-Katalog geladen, entscheidet er; sonst die Statusfelder.
  const katalog = secretStand(item?.credential_secret_name);
  const credentialStored = katalog
    ? katalog.vorhanden
    : ['credential_available', 'authenticated'].includes(authStatus);
  // Ohne Katalogantwort (Befehl kam nicht an) ist "Zugang fehlt" eine
  // Behauptung ohne Messung — D&B zeigte das trotz Zugang vom 21.07.
  // (Klicktest P1 B2). Dann: Stand unbekannt.
  const katalogUnbekannt = !katalog && Boolean(item?.credential_secret_name) && !credentialStored;
  let chip = null;
  if (requiresCredential) {
    chip = authStatus === 'invalid_credentials'
      ? { code: 'invalid', label: tr('accountInvalid', 'Zugang abgelehnt') }
      : credentialStored
        ? { code: 'available', label: katalog?.datum ? `Zugang hinterlegt (${katalog.datum})` : tr('accountCredentialsAvailable', 'Zugang hinterlegt') }
        : katalogUnbekannt
          ? { code: 'unknown', label: 'Zugang: Stand unbekannt' }
          : { code: 'missing', label: tr('accountMissingShort', 'Zugang fehlt') };
    const uebertragung = zugangUebertragung(item?.credential_secret_name);
    if (uebertragung) {
      chip = uebertragung.fehler
        ? { code: 'invalid', label: 'Zugang NICHT übertragen', detail: uebertragung.fehler }
        : { code: 'unknown', label: 'Zugang wird übertragen …' };
    }
  }

  // §1.4: *_requested gilt nur mit verfolgtem Kommando und nur 15 Minuten lang.
  const requestPending = Boolean(adapter?.last_command_id) && updatedAt > 0 && now - updatedAt <= SOURCE_REQUEST_TIMEOUT_MS;
  const requestExpired = 'Die letzte Anfrage ist ohne Ergebnis abgelaufen. Status unbekannt.';

  // §1.3: strikte Priorität — Deaktiviert → Prüfung läuft → Anmeldung läuft →
  // Zugang abgelehnt → Zugang fehlt → Blockiert → Fehlgeschlagen → Bereit →
  // Registriert-ungeprüft → Zugang-hinterlegt-ungeprüft → Unbekannt.
  if (item?.enabled === false) {
    return { code: 'disabled', label: tr('sourceDisabled', 'Deaktiviert — wird bei der Recherche übersprungen.'), chip };
  }
  // Registry-Wahrheit schlaegt App-Vermutung: ist das Ziel in der Scrape-
  // Registry aktiv registriert, sagen wir das, statt "noch nie geprueft" zu
  // behaupten. Der App-eigene Pruefstand bleibt als Zusatz sichtbar.
  // Solange die erste Registry-Antwort aussteht, keine alten App-Pruefungen als
  // aktuellen Stand zeigen (Leadfeeder stand heute erfolgreich, die Liste
  // zeigte "fehlgeschlagen 18.09."; 25.09.2026).
  if (!state.registryStand && state.registryLaeuft && item?.target_key) {
    return { code: 'registry_loading', label: 'Aktueller Stand wird aus CTOX geladen …', detail: '', chip };
  }
  const registryZiel = state.registry?.get?.(String(item?.target_key || item?.id || '').trim())
    || state.registry?.get?.(String(item?.id || '').replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, ''));
  // Owner-Befund 04.09.2026: die Registry war nur ein Notnagel und damit
  // unsichtbar - sobald ein ALTER Pruefstand existierte, gewann der. In der
  // Liste stand deshalb weiter "Letzte Pruefung fehlgeschlagen (31.08.)",
  // obwohl die Registry das Ziel als aktiv fuehrt.
  //
  // Die Registry ist die Wahrheit ueber die Registrierung, der App-Pruefstand
  // nur eine Momentaufnahme. Ist er aelter als einen Tag, fuehrt die Registry -
  // das Alter der alten Pruefung bleibt sichtbar, damit nichts verschwiegen wird.
  const pruefungAlterMs = Number(adapter?.updated_at_ms || item?.updated_at_ms || 0)
    ? Date.now() - Number(adapter?.updated_at_ms || item?.updated_at_ms || 0)
    : Number.POSITIVE_INFINITY;
  const registryAktiv = registryZiel && String(registryZiel.status || '') === 'active';
  // Der echte letzte Abruf (Recherche oder Test) aus der Registry schlaegt den
  // eigenen, oft tagealten App-Test. Owner-Befund 24.09.2026: "alles alter
  // Stand" - die Liste zeigte Tests vom 18.09., waehrend die Abrufe laengst
  // mit neueren Skripten liefen.
  const zeitVon = (lauf) => Date.parse(String(lauf?.finished_at || lauf?.started_at || '')) || 0;
  const letzterLauf = registryZiel?.last_run || null;
  const letzterErfolg = registryZiel?.last_successful_run || null;
  const eigenerTestMs = Number(adapter?.updated_at_ms || item?.updated_at_ms || 0);
  // Owner-Beschwerde 27.09.2026: alte App-Tests (18./23.09.) standen als
  // aktueller Stand da. Der aktuelle Stand ist der echte letzte Abruf aus der
  // Registry; ein eigener App-Test wird nur noch als solcher benannt.
  const stand = registryZiel?.latest_script_revision_no ? ` · Skriptstand ${registryZiel.latest_script_revision_no}` : '';
  const eigenerTest = eigenerTestMs && checkedAt ? ` · eigener App-Test vom ${checkedAt}` : '';
  let registryBefund = null;
  if (registryAktiv && letzterLauf) {
    const datum = (lauf) => new Date(zeitVon(lauf)).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
    const erfolgreich = ['succeeded', 'completed_empty'].includes(String(letzterLauf.status || ''));
    const zuletztOk = letzterErfolg ? ` · zuletzt erfolgreich ${datum(letzterErfolg)}` : ' · noch kein erfolgreicher Abruf';
    // Konto beim Anbieter gesperrt/inaktiv ist weder "Zugang fehlt" noch ein
    // Netzproblem: LinkedIn lief am 27.09.2026 77-mal gegen Bright Data
    // "HTTP 400: Customer is not active" und stand als "temporary unreachable" da.
    const laufDetail = String(letzterLauf.detail || '').trim();
    const kontoInaktiv = /customer is not active|account (?:is )?(?:inactive|suspended|disabled)|konto (?:ist )?(?:inaktiv|gesperrt)/i.test(laufDetail);
    registryBefund = erfolgreich
      ? { code: 'registry_run_ok', label: `Letzter Abruf erfolgreich (${datum(letzterLauf)})${stand}`, detail: eigenerTest ? eigenerTest.slice(3) : '', chip }
      : kontoInaktiv
        ? {
          code: 'account_inactive',
          label: `Zugang hinterlegt, aber Konto beim Anbieter inaktiv — letzter Abruf am ${datum(letzterLauf)} abgewiesen („${laufDetail}“)${zuletztOk}${stand}`,
          detail: String(letzterLauf.run_id || ''),
          chip: { code: 'invalid', label: 'Konto beim Anbieter inaktiv' },
        }
      : {
        code: 'registry_run_failed',
        label: `Letzter Abruf fehlgeschlagen (${String(letzterLauf.status || 'unbekannt').replace(/_/g, ' ')}, ${datum(letzterLauf)})${laufDetail ? `: ${fehlerInKlartext(laufDetail)}` : ''}${zuletztOk}${stand}`,
        detail: eigenerTest ? eigenerTest.slice(3) : '',
        chip,
      };
  } else if (registryAktiv && (pruefungAlterMs > 24 * 60 * 60 * 1000 || (!status && !scrapeStatus && !lastError))) {
    registryBefund = {
      code: 'registry_active',
      label: `In der Scrape-Registry aktiv registriert, noch kein Abruf erfasst${stand}${eigenerTest}`,
      detail: lastError || '',
      chip,
    };
  }
  // Die Anmeldung zuerst: "auth_requested" endet ebenfalls auf _requested
  // und landete sonst bei "Prüfung läuft" (Nachtest P4 T50).
  if (['auth_requested', 'browser_session_requested'].includes(authStatus) || status === 'auth_requested') {
    return requestPending
      ? { code: 'auth_running', label: tr('authRequested', 'Browser-Anmeldung angefordert') + ' — bitte im Browser-Fenster abschließen.', chip }
      : { code: 'request_expired', label: tr('requestExpired', requestExpired), chip };
  }
  if (status === 'generation_queued' || status.endsWith('_requested') || scrapeStatus.endsWith('_requested')) {
    if (requestPending) {
      return {
        code: 'check_running',
        label: status === 'generation_queued'
          ? tr('adapterBuilding', 'Datenzugriff wird eingerichtet')
          : tr('checkRunning', 'Prüfung läuft …'),
        chip,
      };
    }
    return { code: 'request_expired', label: tr('requestExpired', requestExpired), chip };
  }
  if (authStatus === 'invalid_credentials') {
    return {
      detail: registryBefund ? registryBefund.label : '',
      code: 'credential_rejected',
      label: checkedAt
        ? `Zugang abgelehnt: Die Quelle hat die Zugangsdaten zuletzt am ${checkedAt} zurückgewiesen.`
        : tr('credentialRejected', 'Zugang abgelehnt: Die Quelle hat die Zugangsdaten zurückgewiesen.'),
      chip,
    };
  }
  // Zugang fehlt geht vor jedem Abrufstand: RocketReach, Google und
  // CompanyHouse zeigten ohne Zugang "Letzter Abruf erfolgreich" (Codex-Review
  // 27.09.2026). Der Abrufstand bleibt als Zusatz sichtbar.
  if (requiresCredential && !credentialStored && !katalogUnbekannt && !zugangUebertragung(item?.credential_secret_name)) {
    return {
      code: 'credential_missing',
      label: istSchluesselZugang(item?.credential_secret_name)
        ? 'Zugang fehlt — noch kein API-Schlüssel hinterlegt.'
        : 'Zugang fehlt — noch keine Zugangsdaten hinterlegt.',
      detail: registryBefund ? registryBefund.label : '',
      chip,
    };
  }
  if (registryBefund) return registryBefund;
  // Ab hier gibt es keinen aktuellen Abruf aus der Registry: was folgt, ist
  // ein eigener App-Test mit seinem Datum und wird so benannt (Owner-
  // Screenshot 27.09.2026: Bundesanzeiger "Letzte Prüfung fehlgeschlagen
  // (18.09.2026)" sah aus wie der aktuelle Stand).
  const ohneRegistry = state.registryStand && !registryAktiv ? ' · kein aktueller Abruf aus der Registry erfasst' : '';
  const appTest = checkedAt ? `Eigener App-Test vom ${checkedAt}` : 'Eigener App-Test';
  // Ein gescheiterter ABGLEICH mit dem Rechercheablauf ist keine gescheiterte
  // Pruefung der Quelle. Der Abgleich laeuft als Worker-Aufgabe und kann in
  // dessen Sandbox `ctox scrape upsert-target` nicht ausfuehren (Feldbefund
  // 10.09.2026); seitdem stand unter JEDER Quelle "Letzte Pruefung
  // fehlgeschlagen", auch unter denen, die die Recherche taeglich nutzt.
  const abgleichFehler = /reconciliation|reconcile|Adapter-Abgleich|command writeback failed|unsupported status|queue lease/i.test(lastError)
    || ['reconciliation_queued', 'reconciliation_failed'].includes(status);
  if (abgleichFehler && !requestPending) {
    if (registryAktiv) {
      const stand = registryZiel.latest_script_revision_no ? ` · Skriptstand ${registryZiel.latest_script_revision_no}` : '';
      return { code: 'registry_active', label: `In der Scrape-Registry aktiv registriert${stand}`, detail: lastError || '', chip };
    }
    return {
      code: 'web_research',
      label: tr('sourceViaWeb', 'Bereit · die Recherche liest die Quelle über das Web'),
      detail: lastError || '',
      chip,
    };
  }
  if (status.includes('blocked') || scrapeStatus === 'blocked') {
    // blocked_reason/failure_mode fehlen serverseitig (Spec §1.2) — Grund bleibt ehrlich unbekannt.
    // Liegt ein Grund vor, wird er genannt (Nachtest P4 T49).
    const grund = fehlerInKlartext(lastError);
    return grund
      ? { code: 'blocked', label: `${appTest}: Zugriff blockiert — ${grund}${ohneRegistry}`, detail: lastError, chip }
      : { code: 'blocked', label: `${appTest}: Zugriff blockiert, Grund unbekannt${ohneRegistry}`, chip };
  }
  if (status.includes('failed') || scrapeStatus.includes('failed') || lastError) {
    // Owner-Befund 03.09.2026: hier stand der rohe Rust-Debugauswurf im
    // Nutzertext ("ErrorEvent { message: \"unexpected status 404 ...\",
    // cf-ray: ... }"). Der Nutzer liest jetzt den Grund in seiner Sprache; die
    // technische Zeile bleibt erreichbar, aber eingeklappt.
    const klartext = fehlerInKlartext(lastError);
    return {
      code: 'failed',
      label: `${appTest} fehlgeschlagen${klartext ? `: ${klartext}` : ''}${ohneRegistry}`,
      detail: lastError || '',
      chip,
    };
  }
  if (adapterReady({ status, scrape_status: scrapeStatus })) {
    return {
      code: 'ready',
      label: `${checkedAt ? `Bereit laut eigenem App-Test vom ${checkedAt}` : tr('adapterReady', 'Datenzugriff bereit')}${ohneRegistry}`,
      chip,
    };
  }
  if (status.includes('zero_records') || scrapeStatus.includes('zero_records')) {
    return { code: 'empty_result', label: `${appTest} ohne Treffer — die Quelle lieferte keine Einträge${ohneRegistry}`, chip };
  }
  if (scrapeStatus === 'registered') {
    // adapter_revision fehlt serverseitig (Spec §1.2) — keine Revisionsnummer behaupten.
    return { code: 'registered', label: tr('adapterRegistered', 'Adapter registriert · noch nicht geprüft'), chip };
  }
  if (requiresCredential && credentialStored) {
    return { code: 'credential_unverified', label: tr('credentialStored', 'Zugang hinterlegt · Anmeldung noch nicht geprüft'), chip };
  }
  return { code: 'unknown', label: tr('sourceUnknown', 'Status unbekannt — diese Quelle wurde noch nie geprüft.'), chip };
}

function renderSourceRow(item) {
  const intern = isInternalResearchSource(item);
  const adapter = intern ? null : state.adapters.find((entry) => entry.source_id === item.id);
  const status = sourceStatus(item, adapter);
  const ready = intern ? !['internal_unavailable', 'disabled'].includes(status.code) : ['ready', 'registry_active', 'registry_run_ok', 'web_research'].includes(status.code);
  // Owner-Vorgabe 31.08.: Der Browser-Entsperr-Weg ist IMMER erreichbar -
  // Status-Raterei ('failed' vom letzten Command ueberschrieb 'blocked' vom
  // Scrape) hat den Knopf genau dann versteckt, wenn man ihn brauchte.
  // Ein Schluessel-Zugang (Bright Data) laeuft ueber die API und kennt keine
  // Browser-Anmeldung; der Pfeil daneben fuehrte ins Leere (22.09.2026).
  const needsBrowserAuthorization = intern || istSchluesselZugang(item.credential_secret_name)
    ? false
    : item.payload?.input_driven !== true;
  const userManaged = item.payload?.builtin === false;
  const hintergrund = intern
    ? ' · interne Quelle'
    : item.payload?.input_driven === true
      ? ' · pro Unternehmensdomain'
      : '';
  return `<div class="leadgen-source-row" data-source-id="${escapeHtml(item.id)}" data-context-record-id="${escapeHtml(item.id)}" data-context-record-type="research-source" data-context-label="${escapeHtml(item.label)}">
    <button class="leadgen-source-toggle" data-action="toggle-source" aria-pressed="${item.enabled}" ${state.sourceTogglePending.has(item.id) ? 'aria-busy="true" disabled title="Wird gespeichert …"' : `title="Quelle ${item.enabled ? 'deaktivieren' : 'aktivieren'}"`}><span class="leadgen-toggle-dot"></span></button>
    <div class="leadgen-source-copy">
      <strong>${escapeHtml(item.label)}</strong>
      <span>${escapeHtml(item.countries.join('/'))} · ${escapeHtml(item.field_keys.map(researchFieldLabel).join(', '))}${escapeHtml(hintergrund)}</span>
      <span class="leadgen-adapter-state ${ready ? 'is-ready' : ''}">${ready ? icon('check') : '<i></i>'}${escapeHtml(status.label)}</span>
      ${status.detail
        // Ein eigener App-Test ist keine Fehlermeldung (Rundgang 28.09.2026:
        // „Technische Meldung“ klappte bei erfolgreichen Quellen nur das
        // Testdatum auf).
        ? (/^eigener App-Test/i.test(String(status.detail).trim())
          ? `<span class="leadgen-source-detail">${escapeHtml(String(status.detail).trim().replace(/^eigener/i, 'Eigener'))}</span>`
          : `<details class="leadgen-source-detail"><summary>Technische Meldung</summary><code>${escapeHtml(status.detail)}</code></details>`)
        : ''}
      ${status.chip ? `<span class="leadgen-source-credential is-${escapeHtml(status.chip.code)}">${escapeHtml(status.chip.label)}</span>` : ''}
      ${status.chip?.detail ? `<p class="leadgen-muted leadgen-source-credential-detail" role="alert">${escapeHtml(status.chip.detail)}</p>` : ''}
    </div>
    ${intern ? '' : `<div class="leadgen-source-actions" role="group" aria-label="Adapter für ${escapeHtml(item.label)} verwalten">
      <button class="ctox-pane-icon" data-action="adapter-settings" title="Adapter-Einstellungen" aria-label="Adapter-Einstellungen für ${escapeHtml(item.label)}">${icon('settings')}</button>
      <button class="ctox-pane-icon" data-action="view-adapter-script" title="Adapter-Skript ansehen" aria-label="Adapter-Skript für ${escapeHtml(item.label)} ansehen">${icon('code')}</button>
      <button class="ctox-pane-icon" data-action="test-adapter" title="${tr('testAdapter', 'Datenzugriff prüfen')}" aria-label="${tr('testAdapter', 'Datenzugriff prüfen')}">${icon('test')}</button>
      ${item.requires_credential ? `<button class="ctox-pane-icon" data-action="check-credential" title="Zugang prüfen" aria-label="Zugang von ${escapeHtml(item.label)} prüfen">${icon('key')}</button>` : ''}
      ${needsBrowserAuthorization ? `<button class="ctox-pane-icon" data-action="auth-source" title="${tr('browserSignIn', 'Im CTOX-Browser anmelden / entsperren')}" aria-label="${tr('signIn', 'Anmelden')}">${icon('login')}</button>` : ''}
      <button class="ctox-pane-icon is-danger" data-action="delete-adapter" title="Adapter entfernen" aria-label="Adapter für ${escapeHtml(item.label)} entfernen">${icon('trash')}</button>
      ${userManaged ? `<button class="ctox-pane-icon is-danger" data-action="delete-source" title="Quelle vollständig löschen" aria-label="${escapeHtml(item.label)} vollständig löschen">${icon('close')}</button>` : ''}
    </div>`}
  </div>`;
}

function adapterScriptText(adapter) {
  if (!adapter) return '';
  const result = adapter.payload?.result || {};
  const candidates = [
    result.capture_script,
    result.captureScript,
    result.script,
    result.code,
    result.adapter?.capture_script,
    result.adapter?.captureScript,
    result.adapter?.script,
    result.outcome?.adapter?.capture_script,
    result.outcome?.adapter?.script,
  ];
  const script = candidates.find((value) => typeof value === 'string' && value.trim());
  if (script) return script.trim();
  const definition = result.adapter || result.outcome?.adapter || result.definition;
  return definition ? JSON.stringify(sanitizeCommandResult(definition), null, 2) : '';
}

function renderCenter() {
  const pane = state.ctx.host.querySelector('[data-leads-pane]');
  if (!pane) return;
  const statusFilter = state.leadStatusFilter;
  const leads = angezeigteLeads();
  const campaignProgress = campaignResearchProgress(
    state.selectedCampaign,
    listLeads(),
    state.campaignRuns.get(state.selectedCampaign),
  );
  const campaignAction = campaignProgress.trackingTaskId ? 'track-task' : 'research-campaign';
  const campaignActionLabel = campaignProgress.trackingTaskId
    ? tr('trackResearch', 'Recherche in CTOX öffnen')
    : tr('researchCampaign', 'Kampagne recherchieren');
  const selectedVisibleCount = selectedVisibleLeadCount(leads);
  const selectedCount = state.selectedLeadIds.size;
  const selectedRun = state.campaignRuns.get(state.selectedCampaign);
  const selectedActionable = campaignListLeads(state.selectedCampaign)
    .filter((lead) => state.selectedLeadIds.has(lead.id)
      && lead.validation_status !== 'validated'
      && (!['queued', 'running'].includes(lead.research_status)
        || staleCampaignParentPlaceholder(lead, selectedRun)))
    .length;
  const allVisibleSelected = leads.length > 0 && selectedVisibleCount === leads.length;
  const someVisibleSelected = selectedVisibleCount > 0 && !allVisibleSelected;

  const title = pane.querySelector('[data-center-title]');
  if (title) title.textContent = state.selectedCampaign || tr('newResearch', 'Neu- und Nachrecherche');
  const count = pane.querySelector('[data-center-count]');
  if (count) count.textContent = `${leads.length} Leads`;
  const actions = pane.querySelector('[data-center-actions]');
  if (actions) {
    setzeHtmlWennGeaendert(actions, `
        <button class="ctox-button leadgen-campaign-research" data-action="${campaignAction}" data-campaign="${escapeHtml(state.selectedCampaign)}"
          data-task-id="${escapeHtml(campaignProgress.trackingTaskId)}" data-command-id="${escapeHtml(campaignProgress.trackingCommandId)}"
          title="${escapeHtml(campaignActionLabel)}" aria-label="${escapeHtml(campaignActionLabel)}"
          ${!state.selectedCampaign || (!campaignProgress.trackingTaskId && (campaignProgress.active || campaignProgress.actionable === 0)) ? 'disabled' : ''}>
          ${icon(campaignProgress.trackingTaskId ? 'external' : 'search')}<span>${escapeHtml(campaignProgress.trackingTaskId ? 'Task öffnen' : `Alle recherchieren (${campaignProgress.actionable})`)}</span>
        </button>
        ${(() => {
          const fullById = new Map(state.leads.map(lead => [lead.id, lead]));
          const mitLuecken = leads.filter((lead) => hatRechercheErgebnis(lead) && !researchInFlight(lead) && !researchSubmissionPending(lead)
            && (!fullById.has(lead.id) || offeneRecherchefelder(fullById.get(lead.id)).length));
          const alleDetailsBekannt = leads.every(lead => fullById.has(lead.id));
          return mitLuecken.length
            ? `<button class="ctox-button" data-action="research-campaign-gaps" data-campaign="${escapeHtml(state.selectedCampaign)}" data-lead-ids="${escapeHtml(mitLuecken.map((lead) => lead.id).join(','))}" title="Noch offene Felder anhand der vollständigen Leads prüfen und recherchieren">${icon('search')}<span>Lücken schließen${alleDetailsBekannt ? ` (${mitLuecken.length})` : ''}</span></button>`
            : '';
        })()}
        ${(() => {
          if (!state.selectedCampaign || !leads.length) return '';
          const lauf = state.kampagnenVermerkLauf;
          if (lauf) return `<button class="ctox-button" disabled>${icon('search')}<span>Vermerke laden ${lauf.done}/${lauf.total}</span></button>`;
          const geprueft = state.leads.filter(lead => leads.some(row => row.id === lead.id) && vermerkPruefungErledigt(lead)).length;
          const alleBekannt = leads.every(row => state.leads.some(lead => lead.id === row.id));
          return `<button class="ctox-button" data-action="check-campaign-remarks" data-campaign="${escapeHtml(state.selectedCampaign)}" title="${escapeHtml('Sellify-Freitextvermerke aller Leads dieser Kampagne vom CTOX-Agenten auf Kontaktsperren prüfen lassen')}">${icon('check')}<span>Sellify-Vermerke${alleBekannt ? ` (${geprueft}/${leads.length})` : ' prüfen'}</span></button>`;
        })()}
        <button class="ctox-pane-icon" data-action="export-campaign-xlsx" data-campaign="${escapeHtml(state.selectedCampaign)}"
          title="${escapeHtml(tr('exportCampaignXlsxTitle', 'Kampagne als Excel-Datei herunterladen'))}" aria-label="${escapeHtml(tr('exportCampaignXlsxTitle', 'Kampagne als Excel-Datei herunterladen'))}"
          ${state.selectedCampaign && leads.length ? '' : 'disabled'}>${icon('download')}</button>
        <div class="ctox-view-toggle" role="group" aria-label="${tr('view', 'Darstellung')}">
          <button class="ctox-pane-icon" data-action="view-mode" data-view="shards" aria-pressed="${state.campaignViewMode === 'shards'}" title="${tr('shardView', 'Kartenansicht')}" aria-label="${tr('shardView', 'Kartenansicht')}">${icon('shards')}</button>
          <button class="ctox-pane-icon" data-action="view-mode" data-view="table" aria-pressed="${state.campaignViewMode === 'table'}" title="${tr('tableView', 'Tabellenansicht')}" aria-label="${tr('tableView', 'Tabellenansicht')}">${icon('table')}</button>
        </div>`);
  }
  const toolbar = pane.querySelector('[data-center-toolbar]');
  if (toolbar && !regionHasFocus(toolbar)) {
    // Tastaturfokus auf einem Knopf ueberlebt das Neuschreiben (Review):
    // derselbe data-action-Knopf wird wieder fokussiert, falls es ihn noch gibt.
    const fokusKnopf = toolbar.contains(toolbar.ownerDocument?.activeElement)
      ? toolbar.ownerDocument.activeElement.closest?.('[data-action]')?.dataset?.action || ''
      : '';
    queueMicrotask(() => {
      if (!fokusKnopf) return;
      toolbar.querySelector(`[data-action="${fokusKnopf}"]`)?.focus?.({ preventScroll: true });
    });
    const hinweisMarkup = state.notice
      ? `<span class="leadgen-status-note" role="status">${escapeHtml(state.notice)}</span>`
      : '';
    setzeHtmlWennGeaendert(toolbar, selectedCount ? `
        ${hinweisMarkup}
        <strong>${selectedCount} ausgewählt</strong>
        ${selectedVisibleCount !== selectedCount ? `<span>${selectedVisibleCount} sichtbar</span>` : ''}
        <button class="ctox-button" data-action="export-selection-xlsx" title="${escapeHtml(tr('exportSelectionXlsxTitle', 'Ausgewählte Leads als Excel-Datei herunterladen'))}">${icon('download')}<span>Als Excel (${selectedCount})</span></button>
        <button class="ctox-button is-danger" data-action="reset-selection-research">${icon('trash')}<span>Ergebnisse löschen (${selectedCount})</span></button>
        <button class="ctox-button is-primary" data-action="research-selection-new" ${selectedActionable ? '' : 'disabled'}>${icon('search')}<span>Auswahl neu recherchieren (${selectedActionable})</span></button>
        <button class="ctox-button" data-action="research-selection-followup" ${selectedActionable ? '' : 'disabled'}>${icon('search')}<span>Auswahl nachrecherchieren (${selectedActionable})</span></button>
        <button class="ctox-button" data-action="move-selection-campaign"><span>Auswahl verschieben (${selectedCount})</span></button>
        <button class="ctox-button" data-action="clear-selection">Auswahl aufheben</button>` : `
        ${hinweisMarkup}
        <input class="ctox-pane-search" data-lead-search value="${escapeHtml(state.search)}" placeholder="${tr('searchLead', 'Firma, Domain oder Ort')}" />
        <button class="ctox-pane-icon" type="button" data-action="lead-tray-toggle" aria-expanded="${state.leadTrayOpen}" aria-label="Filter und Sortierung" title="Filter und Sortierung"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="7" x2="20" y2="7"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="17" x2="20" y2="17"/><circle cx="9" cy="7" r="2.4"/><circle cx="15" cy="12" r="2.4"/><circle cx="8" cy="17" r="2.4"/></svg></button>`);
    toolbar.classList.toggle('leadgen-selection-bar', Boolean(selectedCount));
  }
  const tray = pane.querySelector('[data-center-tray]');
  if (tray && !regionHasFocus(tray)) {
    // Fokus auf einem Filter-Chip ueberlebt das Neuschreiben (Nachtest P2 KEY-10d).
    const fokusChip = tray.contains(tray.ownerDocument?.activeElement) ? tray.ownerDocument.activeElement : null;
    const chipSchluessel = fokusChip ? [fokusChip.dataset?.action || '', fokusChip.dataset?.status || ''] : null;
    queueMicrotask(() => {
      if (!chipSchluessel) return;
      const ziel = [...tray.querySelectorAll('[data-action]')].find((element) => element.dataset.action === chipSchluessel[0]
        && String(element.dataset.status || '') === chipSchluessel[1]);
      ziel?.focus?.({ preventScroll: true });
    });
    tray.hidden = !state.leadTrayOpen;
    setzeHtmlWennGeaendert(tray, !state.leadTrayOpen ? '' : `
      <div class="leadgen-filter-row">
        <select class="ctox-select" data-action="lead-sort" aria-label="Sortieren nach">
          ${[['name', 'Name'], ['status', 'Recherche-Status'], ['ort', 'Ort'], ['aktualisiert', 'Zuletzt aktualisiert']].map(([k, l]) => `<option value="${k}" ${state.leadSortKey === k ? 'selected' : ''}>${l}</option>`).join('')}
        </select>
        <button type="button" class="leadgen-sort-dir" data-action="lead-sort-dir" data-dir="${state.leadSortDir}" aria-label="Sortierrichtung umkehren" title="${state.leadSortDir === 'desc' ? 'Absteigend' : 'Aufsteigend'}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="${state.leadSortDir === 'desc' ? '6 13 12 19 18 13' : '6 11 12 5 18 11'}"/></svg></button>
        <button type="button" class="leadgen-sort-dir leadgen-filter-reset" data-action="lead-filter-reset" aria-label="Filter zurücksetzen" title="Filter zurücksetzen"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10a8 8 0 1 1 2 7"/><path d="M4 5v5h5"/></svg></button>
      </div>
      <div class="leadgen-filter-chips" role="group" aria-label="Recherche-Status">
        ${[['new', 'Neu'], ['queued', 'Wartet'], ['running', 'Läuft'], ['needs_review', 'Prüfung nötig'], ['completed', 'Abgeschlossen'], ['failed', 'Unvollständig']].map(([k, l]) => `<button type="button" class="ctox-chip" data-action="lead-status-chip" data-status="${k}" aria-pressed="${statusFilter.has(k)}">${l}</button>`).join('')}
      </div>`);
  }
  const status = pane.querySelector('[data-center-status]');
  if (status) {
    // <details>-Zustand ueberlebt den Neuaufbau (gleiche Falle wie im Detail).
    const openDisclosures = new Set([...status.querySelectorAll('details[open][data-disclosure]')].map((node) => node.dataset.disclosure));
    setzeHtmlWennGeaendert(status, `${renderCampaignProgress(campaignProgress)}${renderUnvollstaendigerImport(state.selectedCampaign)}${renderCampaignRecipientExclusions(state.selectedCampaign)}`);
    openDisclosures.forEach((key) => {
      const node = status.querySelector(`details[data-disclosure="${key}"]`);
      if (node) node.open = true;
    });
  }
  const foot = pane.querySelector('[data-center-foot]');
  // Letzter Import DIESER Kampagne, nicht irgendeiner (Nachtest P3).
  const letzterImport = state.imports.find((job) => String(job?.title || '').trim() === String(state.selectedCampaign || '').trim()) || null;
  if (foot) foot.textContent = letzterImport ? `${tr('lastImport', 'Letzter Import')}: ${letzterImport.title}` : datenLadenNoch() ? '' : tr('noImport', 'Noch kein Import');
  const body = pane.querySelector('[data-leads-body]');
  if (!body) return;
  const scrollTop = body.scrollTop;
  // Tastaturfokus auf einer Zeile (oder ihrer Checkbox) ueberlebt das
  // Neuschreiben; vorher fiel er nach Enter/Leertaste auf body (P2 KEY-05/06a).
  const fokusElement = body.ownerDocument?.activeElement;
  const fokusZeile = fokusElement && body.contains(fokusElement)
    ? fokusElement.closest?.('[data-action="select-lead"]')?.dataset?.id || ''
    : '';
  const fokusAufCheckbox = Boolean(fokusZeile) && fokusElement?.matches?.('input[type="checkbox"]');
  // Leere Liste unterscheiden: gar nichts importiert vs. Suche/Filter treffen
  // nichts (vorher immer "Noch keine Leads importiert", Klicktest-Befund P2 V6).
  const filterAktiv = Boolean(String(state.search || '').trim()) || (state.leadStatusFilter?.size || 0) > 0;
  const leerText = filterAktiv && campaignListLeads(state.selectedCampaign).length
    ? 'Keine Leads passen zu Suche oder Filter.'
    : datenLadenNoch()
      ? 'Leads werden geladen …'
      : tr('noLeads', 'Noch keine Leads importiert.');
  // Wie die Kampagnenliste: unveraenderte Liste NICHT neu schreiben, sonst
  // verliert jede Sekunde ein Klick oder der Zeiger seine Zeile
  // (Owner-Befund 18.09.2026).
  const leadsHtml = state.campaignViewMode === 'shards'
    ? `<div class="leadgen-lead-shards">${leads.map(renderLeadShard).join('') || `<div class="leadgen-empty">${escapeHtml(leerText)}</div>`}</div>`
    : `<table class="leadgen-table"><thead><tr><th class="leadgen-select-column"><input type="checkbox" data-action="toggle-visible-leads" aria-label="Alle sichtbaren Leads auswählen" ${allVisibleSelected ? 'checked' : ''} ${someVisibleSelected ? 'data-indeterminate="true"' : ''}></th><th>${tr('organization', 'Organisation')}</th><th>${tr('location', 'Ort')}</th><th>${tr('research', 'Recherche')}</th><th>Sellify</th></tr></thead>
          <tbody>${leads.map(renderLeadRow).join('') || `<tr><td colspan="5" class="leadgen-empty">${escapeHtml(leerText)}</td></tr>`}</tbody></table>`;
  const leadsUnveraendert = state.leadsHtml === leadsHtml && body.childElementCount;
  if (!leadsUnveraendert) {
    state.leadsHtml = leadsHtml;
    body.innerHTML = leadsHtml;
  }
  body.scrollTop = scrollTop;
  if (fokusZeile) {
    const zeile = [...body.querySelectorAll('[data-action="select-lead"]')].find((element) => element.dataset.id === fokusZeile);
    const ziel = fokusAufCheckbox ? zeile?.querySelector('input[type="checkbox"]') : zeile;
    ziel?.focus?.({ preventScroll: true });
  }
  const headerCheckbox = body.querySelector('[data-action="toggle-visible-leads"]');
  if (headerCheckbox) headerCheckbox.indeterminate = someVisibleSelected;
}

function campaignRecipientExclusions(campaign) {
  const rows = [];
  const removedKeys = new Set();
  for (const [leadId, notices] of state.recipientRemovalNotices) {
    for (const notice of notices || []) removedKeys.add(recipientEligibilityKey(leadId, notice.contact?.id));
  }
  for (const lead of campaignLeads(campaign)) {
    for (const contact of lead.contacts || []) {
      const decision = currentContactEligibility(lead, contact);
      if (decision.status === 'free') continue;
      rows.push({
        lead,
        contact,
        decision,
        removed: removedKeys.has(recipientEligibilityKey(lead.id, contact.id)),
      });
    }
  }
  return rows;
}

function renderUnvollstaendigerImport(campaign) {
  const job = unvollstaendigerImport(campaign);
  if (!job) return '';
  const erwartet = Number(job.payload?.expected_lead_count || 0);
  const vorhanden = campaignListLeads(campaign).length;
  return `<div class="leadgen-recipient-exclusions is-pending is-compact" role="status">
    <span>${escapeHtml(`Import unvollständig: ${vorhanden} von ${erwartet} Firmen übernommen`)}</span>
    <button class="ctox-button" data-action="resume-import" data-import-id="${escapeHtml(job.id)}">Fortsetzen</button>
  </div>`;
}
function renderCampaignRecipientExclusions(campaign) {
  const cached = new Set(state.leads.map(lead => lead.id));
  const missing = campaignListLeads(campaign).filter(lead => !cached.has(lead.id)).length;
  const notice = missing ? `<div class="leadgen-recipient-exclusions is-pending is-compact" role="status"><span>Kontaktdaten von ${missing} Leads werden erst bei Auswahl oder Prüfung geladen.</span><button class="ctox-button" data-action="recheck-sellify">Kontakte prüfen</button></div>` : '';
  return notice + renderLoadedCampaignRecipientExclusions(campaign);
}
function renderLoadedCampaignRecipientExclusions(campaign) {
  const rows = campaignRecipientExclusions(campaign);
  if (!rows.length) return '';
  // Owner-Befund 03.09.: Das Feld nannte einen Systemzustand und keine
  // Aufgabe — 48 Zeilen, derselbe Name mehrfach, Kontakte fremder Firmen,
  // und keine Handlung. Ein Kontakt, der geprueft werden MUSS, ist eine
  // Aufgabe; ein Abgleich, der nicht laufen KANN, ist genau eine Meldung mit
  // genau einem Knopf.
  const seen = new Set();
  const unique = [];
  for (const row of rows) {
    const name = personDisplayName(row.contact) || '';
    const key = `${row.lead.id}|${name.toLowerCase()}|${String(row.contact?.person_email || '').toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(row);
  }
  const pending = unique.filter((row) => row.decision.pending === true
    || /nicht prüfbar|wird geprüft/i.test(String(row.decision.label || '')));
  const blocked = unique.filter((row) => !pending.includes(row));
  if (!blocked.length) {
    // Nichts ist ausgeschlossen — die Pruefung konnte nur nicht laufen.
    // Laufende Pruefung von ausgebliebener Antwort unterscheiden: eine Pruefung
    // dauert gemessen bis 75 Sekunden. "Hat nicht geantwortet" waehrend sie noch
    // laeuft las sich wie ein Defekt.
    const laeuft = unique.some((row) => state.recipientEligibilityBusy.has(row.lead?.id));
    const zeitueberschritten = unique.filter((row) => state.recipientEligibilityTimedOut.has(row.lead?.id)).length;
    // Rundgang 11.09.2026: der rote Kasten nahm in schmalen Fenstern die ganze
    // Leadliste ein. Eine Zeile reicht; die Begruendung steht im Hinweistext.
    const erklaerung = 'Solange ein Kontakt nicht gegen Sellify geprüft ist, bleibt er für die Übergabe gesperrt – ohne Prüfung darf niemand angeschrieben werden. '
      + (laeuft ? 'Die Prüfung dauert je Lead bis zu 90 Sekunden.' : '');
    const text = laeuft
      ? `Sperrvermerk-Prüfung läuft · ${unique.length} Kontakt${unique.length === 1 ? '' : 'e'}`
      : zeitueberschritten
        ? `Sperrvermerk-Prüfung ohne Antwort · ${zeitueberschritten} Kontakt${zeitueberschritten === 1 ? '' : 'e'}`
        : `Sperrvermerk-Prüfung ausstehend · ${unique.length} Kontakt${unique.length === 1 ? '' : 'e'}`;
    return `<div class="leadgen-recipient-exclusions is-pending is-compact" role="status" title="${escapeHtml(erklaerung)}">
      <span>${escapeHtml(text)}</span>
      <button class="ctox-button" data-action="recheck-sellify"${laeuft ? ' disabled' : ''}>${laeuft ? 'Prüfung läuft …' : 'Jetzt prüfen'}</button>
    </div>`;
  }
  const foreign = unique.filter((row) => {
    const company = String(row.contact?.company || row.contact?.firma_name || '').trim().toLowerCase();
    return company && company !== String(row.lead?.name || '').trim().toLowerCase();
  });
  const shown = blocked.slice(0, 5);
  // "gesperrt" und "zu prüfen" getrennt zaehlen (Klicktest P2 V9) und eine
  // laufende Pruefung auch in dieser Form zeigen (PRG-02a).
  const hartGesperrt = blocked.filter((row) => row.decision?.status === 'blocked').length;
  const zuPruefen = blocked.length - hartGesperrt;
  const pruefungLaeuft = unique.some((row) => state.recipientEligibilityBusy.has(row.lead?.id));
  const teile = [
    hartGesperrt ? `${hartGesperrt} Kontakt${hartGesperrt === 1 ? '' : 'e'} gesperrt` : '',
    zuPruefen ? `${zuPruefen} zu prüfen` : '',
    pending.length ? `${pending.length} ungeprüft` : '',
    pruefungLaeuft ? 'Prüfung läuft …' : '',
  ].filter(Boolean);
  return `<details class="leadgen-recipient-exclusions" data-disclosure="recipient-exclusions">
    <summary>${escapeHtml(teile.join(' · '))}</summary>
    <ul>${shown.map(({ lead, contact, decision, removed }) => `<li>
      <strong>${escapeHtml(personDisplayName(contact) || tr('contact', 'Kontakt'))}</strong>
      <span>${escapeHtml(lead.name)} · ${escapeHtml(decision.label)}${removed ? ' · abgewählt' : ''}</span>
      ${decision.originalRemark ? `<q>${escapeHtml(decision.originalRemark)}</q>` : `<span>${escapeHtml(decision.reason || '')}</span>`}
    </li>`).join('')}</ul>
    ${blocked.length > shown.length ? `<p>… und ${blocked.length - shown.length} weitere.</p>` : ''}
    ${pending.length ? `<p>${pending.length} Kontakt${pending.length === 1 ? '' : 'e'} konnten nicht geprüft werden.
      <button class="ctox-button" data-action="recheck-sellify"${pruefungLaeuft ? ' disabled' : ''}>${pruefungLaeuft ? 'Prüfung läuft …' : 'Prüfung wiederholen'}</button></p>` : ''}
    ${foreign.length ? `<p role="status">${foreign.length} Kontakt${foreign.length === 1 ? '' : 'e'} gehören laut Datensatz zu einer anderen Firma als der Lead — bitte im Lead prüfen.</p>` : ''}
  </details>`;
}

function renderCampaignProgress(progress) {
  // Owner-Vorgabe 31.08.: Die Leiste ist ein LAUF-Monitor, kein Dauerzustand.
  // Ohne aktive Recherche ist sie ueberfluessig und verschwindet.
  const aktiv = ['queued', 'running'].includes(String(progress.status || ''))
    || (progress.counts?.queued || 0) > 0
    || (progress.counts?.running || 0) > 0;
  if (!aktiv) return '';
  const current = progress.currentLeadName
    ? `${tr('currentLead', 'Aktuell')}: ${progress.currentLeadName}`
    : campaignResearchStatusLabel(progress.status);
  return `
    <div class="leadgen-campaign-progress" role="status" aria-live="polite" data-campaign-status="${escapeHtml(progress.status)}">
      <div class="leadgen-progress-copy">
        <strong>${escapeHtml(current)}</strong>
        <span>${progress.processed} / ${progress.total}</span>
      </div>
      <div class="leadgen-progress-track" aria-hidden="true"><i style="width:${progress.percent}%"></i></div>
      <div class="leadgen-progress-states">
        ${campaignProgressState('new', tr('statusNew', 'Neu'), progress.counts.new)}
        ${campaignProgressState('queued', tr('statusQueued', 'Wartet'), progress.counts.queued)}
        ${campaignProgressState('running', tr('statusRunning', 'Laufend'), progress.counts.running)}
        ${campaignProgressState('completed', tr('statusCompleted', 'Abgeschlossen'), progress.counts.completed)}
        ${campaignProgressState('failed', tr('statusFailed', 'Unvollständig'), progress.counts.failed)}
        ${campaignProgressState('validated', tr('statusValidated', 'Validiert'), progress.counts.validated)}
      </div>
    </div>`;
}

function campaignProgressState(status, label, count) {
  return `<span class="leadgen-progress-state is-${status}"><i></i>${escapeHtml(label)} <strong>${count}</strong></span>`;
}

function renderLeadRow(lead) {
  return `<tr data-action="select-lead" data-id="${escapeHtml(lead.id)}" tabindex="0" data-context-record-id="${escapeHtml(lead.id)}" data-context-record-type="lead" data-context-label="${escapeHtml(lead.name)}" aria-selected="${lead.id === state.selectedLeadId}" data-checked="${state.selectedLeadIds.has(lead.id)}">
    <td class="leadgen-select-column"><input type="checkbox" data-action="toggle-lead" data-id="${escapeHtml(lead.id)}" aria-label="${escapeHtml(lead.name)} auswählen" ${state.selectedLeadIds.has(lead.id) ? 'checked' : ''}></td>
    <td><strong>${escapeHtml(lead.name)}</strong>${lead.domain || lead.website ? `<span>${escapeHtml(lead.domain || lead.website)}</span>` : ''}</td>
    <td>${escapeHtml([lead.city, lead.country].filter(Boolean).join(', ') || '—')}</td>
    <td><span class="ctox-badge ${lead.validation_status === 'validated' ? 'is-success' : ''}">${escapeHtml(researchLabel(lead))}</span></td>
    <td>${escapeHtml(sellifyLabel(lead))}</td>
  </tr>`;
}

function renderLeadShard(lead) {
  const submissionPending = researchSubmissionPending(lead);
  return `<article class="leadgen-lead-shard" data-checked="${state.selectedLeadIds.has(lead.id)}"
    data-context-record-id="${escapeHtml(lead.id)}" data-context-record-type="lead" data-context-label="${escapeHtml(lead.name)}">
    <input type="checkbox" data-action="toggle-lead" data-id="${escapeHtml(lead.id)}" aria-label="${escapeHtml(lead.name)} auswählen" ${state.selectedLeadIds.has(lead.id) ? 'checked' : ''}>
    <button class="leadgen-shard-main" data-action="select-lead" data-id="${escapeHtml(lead.id)}" aria-pressed="${lead.id === state.selectedLeadId}">
      <strong>${escapeHtml(lead.name)}</strong>
      <span>${escapeHtml([lead.city, lead.country].filter(Boolean).join(', ') || lead.domain || '—')}</span>
      <footer><span class="ctox-badge ${lead.validation_status === 'validated' ? 'is-success' : ''}">${escapeHtml(researchLabel(lead))}</span><span>${escapeHtml(lead.sellify_status === 'completed' ? 'Sellify' : '')}</span></footer>
    </button>
    <button class="ctox-pane-icon leadgen-shard-research" data-action="research-lead" data-id="${escapeHtml(lead.id)}" title="${submissionPending ? 'Recherche wird gestartet' : researchInFlight(lead) ? 'Recherche läuft' : 'Lead nachrecherchieren'}" aria-label="${escapeHtml(lead.name)} nachrecherchieren" ${submissionPending || researchInFlight(lead) ? 'disabled' : ''}>${icon('search')}</button>
  </article>`;
}

// Jede Collection-Änderung ruft render() und damit renderDetail(). Ein
// innerHTML-Neuaufbau reisst dabei den Fokus aus einem Eingabefeld und setzt
// die Scrollposition zurueck — Tippen war praktisch unmoeglich. Solange der
// Nutzer in diesem Bereich schreibt, wird der Neuaufbau aufgeschoben und nach
// dem Verlassen des Feldes einmal nachgeholt.
function detailPaneHasActiveInput(pane) {
  const active = state.ctx.host.ownerDocument?.activeElement || globalThis.document?.activeElement;
  if (!active || !pane.contains(active)) return false;
  // Checkboxen sind kein Tippen: nach dem Anhaken eines Empfaengers blieb
  // "Sellify aktualisieren" gesperrt, bis der Fokus wanderte (Klicktest P3).
  return regionHasFocus(pane);
}

function renderDetail() {
  const pane = state.ctx.host.querySelector('[data-detail-pane]');
  if (!pane) return;
  if (detailPaneHasActiveInput(pane)) {
    if (!state.detailRenderDeferred) {
      state.detailRenderDeferred = true;
      const active = state.ctx.host.ownerDocument?.activeElement || globalThis.document?.activeElement;
      active.addEventListener('blur', () => {
        state.detailRenderDeferred = false;
        renderDetail();
      }, { once: true });
    }
    return;
  }
  state.detailRenderDeferred = false;
  const title = pane.querySelector('[data-detail-title]');
  const actions = pane.querySelector('[data-detail-actions]');
  const tabsHost = pane.querySelector('[data-detail-tabs]');
  const body = pane.querySelector('[data-detail-body]');
  if (!title || !body) return;
  const lead = selectedLead();
  if (!lead) {
    // Waehrend eines Sync-Ticks ist die Lead-Liste kurz leer. Solange eine
    // Auswahl existiert und die Spalte Inhalt zeigt, bleibt sie stehen.
    const summary = listLeads().find(row => row.id === state.selectedLeadId);
    title.textContent = summary?.name || '—';
    if (actions) actions.innerHTML = '';
    if (tabsHost) tabsHost.innerHTML = '';
    body.innerHTML = `<div class="leadgen-empty" role="status">${escapeHtml(summary
      ? state.selectedDetailError || 'Details und Belege werden geladen …'
      : tr('selectLead', 'Lead auswählen.'))}${summary && state.selectedDetailError
        ? `<button class="ctox-button" data-action="select-lead" data-id="${escapeHtml(summary.id)}">Details erneut laden</button>` : ''}</div>`;
    return;
  }
  const scrollTop = body.scrollTop;
  // Der Personenreiter zeigt die Felder der AUSGEWAEHLTEN Person. Ohne diese
  // Skopierung liest jedes person_*-Feld contacts[0] und der Wechsel blieb
  // wirkungslos (Befund 03.09.).
  const contactScopedLead = contactTabLead(lead);
  const review = researchFieldReview(contactScopedLead);
  const readyForValidation = leadReadyForValidation(lead);
  const blockerDetails = validationBlockerDetails(lead);
  const blockers = blockerDetails.map((entry) => entry.text);
  const alreadyValidated = lead.validation_status === 'validated';
  const validateLabel = alreadyValidated
    ? tr('validated', 'Validiert')
    : readyForValidation
    ? tr('validate', 'Validieren')
    // Ein Haekchen mit "Noch nicht freigabefaehig" las sich wie erledigt.
    : `${tr('notReadyForValidation', 'Freigabe')}: ${tr('openPoints', 'noch')} ${blockers.length || 1} ${tr('openPointsSuffix', 'offene Punkte')}`;
  // Der Tooltip sagte "Recherche noch nicht abgeschlossen." genau dann, wenn
  // der Knopf freigegeben war (keine Sperren = leere Liste = Ersatztext).
  const validationDetails = blockers.join(' · ')
    || (alreadyValidated ? tr('validatedHint', 'Der Lead ist validiert und kann an Sellify übergeben werden.') : tr('validateHint', 'Alle Punkte erfüllt – Lead jetzt validieren.'));
  const handoffPrecondition = sellifyHandoffPrecondition(lead);
  const researchSubmissionRunning = researchSubmissionPending(lead);
  const handoffVorbereitung = state.sellifyUebergabeVorbereitung?.has?.(lead.id) === true;
  const handoffUnterbrochen = uebergabeUnterbrochen(lead);
  const handoffRunning = (lead.sellify_status === 'queued' && !handoffUnterbrochen) || handoffVorbereitung;
  const handoffDisabled = Boolean(handoffPrecondition) || handoffRunning;
  const handoffTitle = handoffVorbereitung
    ? 'Übergabe wird vorbereitet: Sellify-Sperrvermerke werden unmittelbar vor dem Schreiben erneut geprüft …'
    : handoffRunning ? tr('handoffRunning', 'Übergabe läuft')
      : handoffPrecondition || (handoffUnterbrochen ? 'Die letzte Übergabe wurde unterbrochen (Browser geschlossen). Erneut übergeben: bereits angelegte Datensätze werden erkannt und aktualisiert.' : '');
  const activeTab = DETAIL_TAB_IDS.includes(state.activeDetailTab) ? state.activeDetailTab : 'overview';
  // Owner-Vorgabe 31.08.: zwei getrennte Wege mit harter Sellify-Weiche.
  // "Neue Recherche" bricht ab, wenn die Firma bereits im CRM existiert;
  // "Nachrecherche" bricht ab, wenn sie dort NICHT gefunden wird.
  // Kennt der Vorabgleich die Antwort, steht nur der erlaubte Knopf da; der
  // andere endete ohnehin nur in einer Fehlermeldung.
  const sellifyBekannt = lead?.payload?.sellify_precheck?.known;
  const neueRechercheKnopf = `<button class="ctox-button is-primary" data-action="research-lead-new" data-id="${escapeHtml(lead.id)}" title="${escapeHtml(tr('researchNewTitle', 'Startet nur, wenn die Firma noch nicht in Sellify geführt wird.'))}">${icon('search')}<span>${tr('researchNew', 'Neue Recherche')}</span></button>`;
  const nachrechercheKnopf = `<button class="ctox-button is-primary" data-action="research-lead-followup" data-id="${escapeHtml(lead.id)}" title="${escapeHtml(sellifyBekannt ? `${tr('knownInSellify', 'In Sellify geführt')} (${lead.payload.sellify_precheck.contact_id || ''})` : tr('researchFollowupTitle', 'Startet nur, wenn die Firma bereits in Sellify geführt wird.'))}">${icon('search')}<span>${tr('researchLead', 'Nachrecherche')}</span></button>`;
  // Waehrend ein Auftrag laeuft, startete ein zweiter Klick eine parallele
  // zweite Recherche desselben Leads (Kiesow, 10.09.2026 zweimal). Stattdessen
  // steht dort der Abbruch - ein haengender Lauf blieb sonst bis zur
  // Harness-Frist von 60 min auf "Läuft" (Sasol, 11.09.2026).
  const luecken = hatRechercheErgebnis(lead) ? offeneRecherchefelder(lead) : [];
  const lueckenKnopf = luecken.length
    ? `<button class="ctox-button" data-action="research-lead-gaps" data-id="${escapeHtml(lead.id)}" title="${escapeHtml(`Nur die ${luecken.length} noch nicht belegten Felder erneut recherchieren: ${luecken.map(researchFieldLabel).join(', ')}`)}">${icon('search')}<span>Lücken schließen (${luecken.length})</span></button>`
    : '';
  const researchAction = researchSubmissionRunning
    ? `<button class="ctox-button is-primary" data-action="research-lead" data-id="${escapeHtml(lead.id)}" disabled>${icon('search')}<span>Recherche wird gestartet …</span></button>`
    : researchInFlight(lead)
      ? (state.researchCancelling?.has(lead.id)
        ? `<button class="ctox-button" disabled>${icon('close')}<span>Wird abgebrochen …</span></button>`
        : `<button class="ctox-button" data-action="cancel-research" data-id="${escapeHtml(lead.id)}" title="${escapeHtml(tr('cancelResearchTitle', 'Bricht den laufenden Auftrag ab. Bereits gespeicherte Werte und Belege bleiben erhalten.'))}">${icon('close')}<span>${tr('cancelResearch', 'Recherche abbrechen')}</span></button>`)
    : lueckenKnopf + (sellifyBekannt === true
      ? nachrechercheKnopf
      : sellifyBekannt === false
        ? neueRechercheKnopf
        : neueRechercheKnopf + nachrechercheKnopf);
  // Owner-Vorgabe 30.08.: kein eigener Entscheidungs-Tab - die Aktionen
  // stehen kompakt in der Uebersicht.
  const decisionActions = `<div class="leadgen-detail-actions">
    ${researchAction}
    <button class="ctox-button" data-action="validate-lead" data-id="${escapeHtml(lead.id)}" ${readyForValidation && !alreadyValidated ? '' : 'disabled'} title="${escapeHtml(validationDetails)}">${alreadyValidated || readyForValidation ? icon('check') : ''}<span>${escapeHtml(validateLabel)}</span></button>
    <button class="ctox-button" data-action="sellify-update-only" data-id="${escapeHtml(lead.id)}" ${handoffDisabled ? 'disabled' : ''} title="${escapeHtml(handoffTitle || tr('sellifyUpdateOnlyTitle', 'Organisation und ausgewählte Personen in Sellify aktualisieren.'))}">${icon('send')}<span>${handoffVorbereitung ? 'Wird vorbereitet …' : handoffRunning ? 'Übergabe läuft …' : tr('sellifyUpdateOnly', 'Sellify aktualisieren')}</span></button>
    <button class="ctox-button" data-action="export-lead-xlsx" data-id="${escapeHtml(lead.id)}" title="${escapeHtml(tr('exportLeadXlsxTitle', 'Recherche dieses Leads als Excel-Datei herunterladen'))}" aria-label="${escapeHtml(tr('exportLeadXlsxTitle', 'Recherche dieses Leads als Excel-Datei herunterladen'))}">${icon('download')}<span>${tr('exportXlsx', 'Excel')}</span></button>
    <button class="ctox-button" data-action="sellify-update-campaign" data-id="${escapeHtml(lead.id)}" ${handoffDisabled ? 'disabled' : ''} title="${escapeHtml(handoffTitle || tr('sellifyUpdateCampaignTitle', 'Organisation und ausgewählte Personen aktualisieren und der Kampagne hinzufügen.'))}">${icon('send')}<span>${handoffVorbereitung ? 'Wird vorbereitet …' : handoffRunning ? 'Übergabe läuft …' : tr('sellifyUpdateCampaign', 'Sellify + Kampagne')}</span></button>
  </div>`;
  const tabContent = activeTab === 'overview'
    ? `${renderResearchReviewSummary(review, contactScopedLead)}${decisionActions}${!readyForValidation ? `<details class="leadgen-validation-details" data-disclosure="validation-blockers"><summary>${blockers.length || 1} offene Punkte bis zur Freigabe</summary><ul>${(blockerDetails.length ? blockerDetails : [{ text: validationDetails, key: '' }]).map((blocker) => `<li>${escapeHtml(blocker.text)}${renderBlockerActions(blocker.key)}</li>`).join('')}</ul></details>` : ''}${renderReleasedEmptyFields(lead)}${renderContactRecipientSelection(lead)}`
    : renderResearchReviewGroups(review, contactScopedLead, activeTab);
  title.textContent = lead.name || '—';
  if (actions) {
    setzeHtmlWennGeaendert(actions, `<button class="ctox-pane-icon" data-action="edit-lead" data-id="${escapeHtml(lead.id)}" title="Lead bearbeiten" aria-label="${escapeHtml(lead.name)} bearbeiten">${icon('edit')}</button>`);
  }
  // Fokus auf einem Detailreiter ueberlebt das Neuschreiben (Nachtest P2 KEY-03).
  const reiterFokus = tabsHost && tabsHost.contains(tabsHost.ownerDocument?.activeElement)
    ? tabsHost.ownerDocument.activeElement.dataset?.detailTab || ''
    : '';
  if (tabsHost) tabsHost.innerHTML = renderDetailTabs(lead, activeTab);
  if (reiterFokus) tabsHost.querySelector(`[data-detail-tab="${reiterFokus}"]`)?.focus?.({ preventScroll: true });
  // <details> ist DOM-Zustand: ein innerHTML-Neuaufbau klappte "offene
  // Punkte" nach jedem Sync-Tick sofort wieder zu. Zustand sichern und
  // nach dem Neuaufbau wiederherstellen.
  const openDisclosures = new Set([...body.querySelectorAll('details[open][data-disclosure]')].map((node) => node.dataset.disclosure));
  body.innerHTML = tabContent;
  openDisclosures.forEach((key) => {
    const node = body.querySelector(`details[data-disclosure="${key}"]`);
    if (node) node.open = true;
  });
  body.scrollTop = scrollTop;
}

function renderLeadEditor() {
  const mount = state.ctx.host.querySelector('[data-app-dialog]');
  if (!mount) return;
  if (state.adapterInspectorSourceId) {
    const source = state.sources.find((entry) => entry.id === state.adapterInspectorSourceId);
    const adapter = state.adapters.find((entry) => entry.source_id === state.adapterInspectorSourceId);
    if (!source) {
      state.adapterInspectorSourceId = '';
      mount.replaceChildren();
      return;
    }
    // Das Skript kommt aus der Scrape-Registry (Kern liefert es mit
    // registry_read/include_script_target); vorher konnte der Dialog nie ein
    // Skript zeigen (Klicktest-Befund P4 V14).
    const registriert = state.adapterInspectorScript;
    const skriptBereit = registriert && registriert.target_key === source.target_key;
    const script = skriptBereit && registriert.available ? String(registriert.text || '') : adapterScriptText(adapter);
    const stand = !registriert || state.adapterInspectorScriptLaeuft
      ? 'Skript wird aus der Scrape-Registry geladen …'
      : skriptBereit && registriert.available
        ? `Skriptstand ${registriert.revision_no ?? '?'}${registriert.created_at ? ` vom ${new Date(registriert.created_at).toLocaleDateString('de-DE')}` : ''}${registriert.truncated ? ' (gekürzt)' : ''}`
        : (registriert?.error ? `Skript nicht lesbar: ${registriert.error}` : 'Für diese Quelle ist in der Scrape-Registry noch kein Skript registriert.');
    // Nicht bei jedem Sync-Tick neu schreiben (Scrollposition, P4 V15).
    const signatur = JSON.stringify([source.id, stand, script.length, adapter?.updated_at_ms || 0]);
    if (mount.dataset.adapterSignatur === signatur && mount.querySelector('.leadgen-adapter-inspector')) return;
    mount.dataset.adapterSignatur = signatur;
    mount.innerHTML = `
      <div class="leadgen-source-backdrop" data-action="close-adapter-script">
        <section class="leadgen-lead-editor leadgen-adapter-inspector" role="dialog" aria-modal="true" aria-label="Adapter-Skript ${escapeHtml(source.label)}">
          <header class="ctox-pane-header ctox-pane-band leadgen-header">
            <div><span class="ctox-pane-kicker">Adapter-Skript</span><h2 class="ctox-pane-title">${escapeHtml(source.label)}</h2></div>
            <button class="ctox-pane-icon" data-action="close-adapter-script" title="Schließen" aria-label="Schließen">${icon('close')}</button>
          </header>
          <p class="leadgen-muted" role="status">${escapeHtml(stand)}</p>
          <pre class="leadgen-adapter-script"><code>${escapeHtml(script || '—')}</code></pre>
          <footer class="leadgen-dialog-actions">
            <button class="ctox-button" data-action="close-adapter-script">Schließen</button>
            <button class="ctox-button is-primary" data-action="build-adapter" data-id="${escapeHtml(source.id)}">${adapter ? 'Adapter neu erzeugen' : 'Adapter erzeugen'}</button>
          </footer>
        </section>
      </div>`;
    return;
  }
  if (state.sellifyImportOpen) {
    const busy = Boolean(state.sellifyImportBusy);
    // Neuzeichnen verlor Eingabe und Fokus des Suchfelds (P1 #15): Wert steht
    // im Zustand (input-Handler), Fokus und Cursor werden wiederhergestellt.
    const suchfeld = mount.querySelector('[data-sellify-campaign-query]');
    const suchfeldFokus = suchfeld && suchfeld === (mount.ownerDocument?.activeElement);
    const cursor = suchfeldFokus ? [suchfeld.selectionStart, suchfeld.selectionEnd] : null;
    queueMicrotask(() => {
      if (!suchfeldFokus) return;
      const neu = mount.querySelector('[data-sellify-campaign-query]');
      if (!neu || neu.disabled) return;
      neu.focus();
      try { neu.setSelectionRange(cursor[0], cursor[1]); } catch { /* type=search */ }
    });
    const results = Array.isArray(state.sellifyImportResults) ? state.sellifyImportResults : [];
    const resultRows = results.map((entry) => `
      <button class="ctox-button leadgen-sellify-campaign-row" data-action="sellify-campaign-pick" data-name="${escapeHtml(entry.name)}" ${busy ? 'disabled' : ''}>
        <span>${escapeHtml(entry.name)}</span><span class="leadgen-count">${entry.count}${entry.truncated ? '+' : ''} Kontakte</span>
      </button>`).join('');
    mount.innerHTML = `
      <div class="leadgen-source-backdrop" data-action="close-sellify-import">
        <section class="leadgen-lead-editor" role="dialog" aria-modal="true" aria-label="Sellify-Kampagne importieren">
          <header class="ctox-pane-header ctox-pane-band leadgen-header">
            <div><span class="ctox-pane-kicker">Nachrecherche</span><h2 class="ctox-pane-title">Sellify-Kampagne importieren</h2></div>
            <button class="ctox-pane-icon" data-action="close-sellify-import" title="Schließen" aria-label="Schließen" ${busy ? 'disabled' : ''}>${icon('close')}</button>
          </header>
          <div class="leadgen-dialog-body">
            <p>Eine bestehende Sellify-Kampagne wird als Kampagne mit ihren Firmen übernommen; jede Firma ist damit für die Nachrecherche bereit.</p>
            <div class="leadgen-inline-form">
              <input type="search" data-sellify-campaign-query value="${escapeHtml(state.sellifyImportQuery || '')}" placeholder="Kampagnenname suchen …" ${busy ? 'disabled' : ''} />
              <button class="ctox-button is-primary" data-action="sellify-campaign-search" ${busy || state.sellifyImportSucheLaeuft ? 'disabled' : ''}>${icon('search')}<span>Suchen</span></button>
            </div>
            ${state.sellifyImportNotice ? `<p class="leadgen-import-notice" role="status" data-sellify-import-notice>${escapeHtml(state.sellifyImportNotice)}</p>` : ''}
            <div class="leadgen-sellify-campaign-list">${resultRows || (busy ? '' : '<p class="leadgen-empty">Noch keine Suche – Kampagnenname eingeben.</p>')}</div>
          </div>
          <footer class="leadgen-dialog-actions">
            <button class="ctox-button" data-action="close-sellify-import" ${busy ? 'disabled' : ''}>Schließen</button>
          </footer>
        </section>
      </div>`;
    return;
  }
  if (!state.leadEditorOpen || !state.leadDraft) {
    mount.replaceChildren();
    return;
  }
  // Waehrend im Editor getippt wird, nicht neu schreiben: jeder Sync-Tick
  // ersetzte das Formular und verschluckte Tastendruecke (Klicktest P3
  // EDT-F42a: 2 von 7 Zeichen verloren). Der Entwurf steht ohnehin im Zustand
  // (input-Handler); gleicher Lead = nichts Neues zu zeigen.
  const offenerEditor = mount.querySelector('[data-lead-editor]');
  if (offenerEditor && offenerEditor.dataset.leadId === String(state.leadEditorId || '')) {
    const aktiv = mount.ownerDocument?.activeElement;
    if (aktiv && offenerEditor.contains(aktiv)) return;
  }
  const draft = state.leadDraft;
  // Die Freigabe erkennt bestimmte Formulierungen; die Vorschlaege zeigen sie.
  const suggestions = {
    firma_aufnahmeeignung: ['geeignet', 'nicht geeignet'],
    firma_ausschlussgrund: ['kein Ausschluss', 'Ausschluss: Wettbewerber', 'Ausschluss: Bestandskunde'],
    fachliche_aufnahmeentscheidung: ['aufnehmen', 'nicht aufnehmen', 'zurückstellen'],
    listenstatus: ['neu', 'in Prüfung', 'freigegeben', 'gesperrt'],
  };
  const fieldInput = (key, label, value, options = {}) => `
    <label class="leadgen-form-field ${options.wide ? 'is-wide' : ''}">
      <span>${escapeHtml(label)}</span>
      <input data-lead-edit-field="${escapeHtml(key)}" value="${escapeHtml(value || '')}" ${options.required ? 'required' : ''}${suggestions[key] ? ` list="leadgen-suggest-${escapeHtml(key)}"` : ''}>
      ${suggestions[key] ? `<datalist id="leadgen-suggest-${escapeHtml(key)}">${suggestions[key].map((item) => `<option value="${escapeHtml(item)}"></option>`).join('')}</datalist>` : ''}
    </label>`;
  // firma_land hat im Editor genau EIN Feld ("Land (Länderkennzeichen)" =
  // country). Ein zweites Feld "Länderkennzeichen" schrieb denselben Wert und
  // der zweite gewann still (Klicktest P3 EDT-F17b / V-02).
  const governanceInputs = GOVERNANCE_FIELDS.filter((key) => key !== 'firma_land').map((key) => fieldInput(
    key,
    REVIEW_FIELD_LABELS[key] || key,
    draft[key],
    { wide: !['bearbeiter_initialen', 'listenstatus', 'aenderungsart'].includes(key) },
  )).join('');
  mount.innerHTML = `
    <div class="leadgen-source-backdrop" data-action="close-lead-editor">
      <section class="leadgen-lead-editor" role="dialog" aria-modal="true" aria-label="Lead bearbeiten" data-lead-editor data-lead-id="${escapeHtml(String(state.leadEditorId || ''))}">
        <header class="ctox-pane-header ctox-pane-band leadgen-header">
          <div><span class="ctox-pane-kicker">Lead</span><h2 class="ctox-pane-title">Bearbeiten</h2></div>
          <button class="ctox-pane-icon" data-action="close-lead-editor" title="Schließen" aria-label="Schließen">${icon('close')}</button>
        </header>
        <div class="leadgen-lead-form">
          ${fieldInput('name', 'Organisation', draft.name, { required: true, wide: true })}
          ${fieldInput('website', 'Website', draft.website, { wide: true })}
          ${fieldInput('address_line', 'Straße', draft.address_line, { wide: true })}
          ${fieldInput('postal_code', 'PLZ', draft.postal_code)}
          ${fieldInput('city', 'Ort', draft.city)}
          ${fieldInput('country', 'Land (Länderkennzeichen)', draft.country)}
          ${fieldInput('email', 'E-Mail', draft.email)}
          ${fieldInput('phone', 'Telefon', draft.phone)}
          ${fieldInput('campaign', 'Kampagne', draft.campaign, { required: true, wide: true })}
          ${EXTRA_COMPANY_EDIT_FIELDS.map((key) => fieldInput(key, REVIEW_FIELD_LABELS[key] || key, draft[key])).join('')}
          <div class="leadgen-form-section-title">Fachliche Datenpflege und Aufnahmeentscheidung</div>
          ${governanceInputs}
          ${state.leadEditorContactId ? `<div class="leadgen-form-section-title">${state.leadEditorContactId === NEUER_ANSPRECHPARTNER ? 'Neuer Ansprechpartner (optional)' : `Ansprechpartner: ${escapeHtml(state.leadEditorContactName || '')}`}</div>
          ${PERSON_EDIT_FIELDS.map((key) => fieldInput(`person:${key}`, REVIEW_FIELD_LABELS[key] || key, draft[`person:${key}`], { wide: ['person_email', 'person_linkedin', 'person_xing', 'person_position', 'person_funktion'].includes(key) })).join('')}` : ''}
          ${state.leadEditorContactId && state.leadEditorContactId !== NEUER_ANSPRECHPARTNER ? `<details class="leadgen-form-section-extra" ${PERSON_EDIT_FIELDS.some((key) => String(draft[`neu:${key}`] || '').trim()) ? 'open' : ''}>
            <summary class="leadgen-form-section-title">Weiteren Ansprechpartner anlegen (optional)</summary>
            ${PERSON_EDIT_FIELDS.map((key) => fieldInput(`neu:${key}`, REVIEW_FIELD_LABELS[key] || key, draft[`neu:${key}`], { wide: ['person_email', 'person_linkedin', 'person_xing', 'person_position', 'person_funktion'].includes(key) })).join('')}
          </details>` : ''}
        </div>
        <footer class="leadgen-dialog-actions">
          <button class="ctox-button" data-action="close-lead-editor">Abbrechen</button>
          <button class="ctox-button is-primary" data-action="save-lead-editor">Speichern</button>
        </footer>
      </section>
    </div>`;
}

async function handleClick(event) {
  const trigger = event.target.closest('[data-action]');
  if (!trigger) return;
  const action = trigger.dataset.action;
  const id = trigger.dataset.id || trigger.closest('[data-source-id]')?.dataset.sourceId || '';
  await prepareFullLeadAction(action, id, trigger.dataset.campaign || state.selectedCampaign);
  if (action === 'retry-sync') {
    await retryInitialSync();
    return;
  }
  if (action === 'adapter-settings') {
    await editSource(id);
    return;
  }
  if (action === 'view-adapter-script') {
    state.adapterInspectorSourceId = id;
    state.adapterInspectorScript = null;
    renderLeadEditor();
    void ladeAdapterSkript(id);
    return;
  }
  if (action === 'close-adapter-script' && (event.target === trigger || trigger.matches('button[data-action="close-adapter-script"]'))) {
    state.adapterInspectorSourceId = '';
    renderLeadEditor();
    return;
  }
  if (action === 'select-lead') {
    if (event.metaKey || event.ctrlKey) {
      toggleLeadSelection(id, !state.selectedLeadIds.has(id), event.shiftKey);
      renderCenter();
    } else {
      selectLeadAndRefreshEligibility(id);
    }
  }
  if (action === 'select-contact-tab') {
    const leadId = String(trigger.dataset.leadId || state.selectedLeadId || '').trim();
    const contactId = String(trigger.dataset.contactId || '').trim();
    const contactIdentity = String(trigger.dataset.contactIdentity || '').trim();
    const personKey = String(trigger.dataset.personKey || '').trim();
    if (leadId && contactId) {
      state.activeContactTabs.set(leadId, { id: contactId, identity: contactIdentity, personKey });
      renderDetail();
    }
    return;
  }
  if (action === 'select-detail-tab') {
    const leadId = String(trigger.dataset.leadId || state.selectedLeadId || '').trim();
    const tabId = String(trigger.dataset.detailTab || 'overview').trim();
    if (leadId && DETAIL_TAB_IDS.includes(tabId)) {
      setActiveDetailTab(tabId);
      renderDetail();
    }
    return;
  }
  if (action === 'select-campaign') {
    state.selectedCampaign = trigger.dataset.campaign || '';
    state.selectedLeadIds.clear();
    state.selectionAnchorId = '';
    const first = listLeads().find((lead) => leadKampagnen(lead).includes(state.selectedCampaign));
    renderCampaigns();
    selectLeadAndRefreshEligibility(first?.id || '');
  }
  if (action === 'open-sources') {
    // Beim Oeffnen einmal die Registry nachladen, damit die Liste nicht auf
    // App-Vermutungen sitzen bleibt.
    if (!state.registryStand || Date.now() - state.registryStand > 300_000) void ladeQuellenRegistry();
    if (!state.secretKatalogStand || Date.now() - state.secretKatalogStand > 60_000) void ladeSecretKatalog();
    state.sourcePanelView = 'sources';
    state.sourcePanelOpen = true;
    renderSourcePanel();
  }
  if (action === 'open-policy') {
    state.sourcePanelView = 'policy';
    state.sourcePanelOpen = true;
    state.researchPolicyDraft = state.researchPolicy;
    state.researchPolicyFollowupDraft = state.researchPolicyFollowup;
    renderSourcePanel();
  }
  if (action === 'source-view') {
    state.sourcePanelView = ['policy', 'digest', 'pflichtfelder'].includes(trigger.dataset.view) ? trigger.dataset.view : 'sources';
    state.sourceSearch = '';
    state.researchPolicyDraft = state.researchPolicy;
    state.researchPolicyFollowupDraft = state.researchPolicyFollowup;
    renderSourcePanel();
  }
  if (action === 'close-sources' && (event.target === trigger || trigger.matches('button[data-action="close-sources"]'))) {
    state.sourcePanelOpen = false; renderSourcePanel();
  }
  // Der Import-Knopf im mittleren Fenster fuegt Unternehmen zur AUSGEWAEHLTEN
  // Kampagne hinzu. Vorher rief er den Importer ohne Kampagnenname auf; der
  // Importer setzte seinen Standardtitel, und weil die Kampagne aus dem Titel
  // abgeleitet wird, entstand jedes Mal eine neue. Neue Kampagnen legt man
  // ausdruecklich ueber "Neue Kampagne" an.
  // Eine Datei landete per Vorbelegung in der gerade offenen Sellify-Kampagne
  // (8 AT-Firmen in "Unternehmen CH - Chemie", 25.09.2026). Sellify-Kampagnen
  // werden nicht mehr vorbelegt; der Nutzer benennt die Zielkampagne selbst.
  // Auch eine offene eigene Kampagne wird nicht mehr vorbelegt: der Kunde
  // importierte drei Dateien still in die Test-Kampagne "Abnahme 10.09.2026"
  // (28.09.2026). Ergaenzen einer Kampagne geht ueber die Warnung in der Vorschau.
  if (action === 'import-leads') {
    await openImporter(`Import ${new Date().toLocaleDateString('de-DE')}`);
  }
  if (action === 'import-sellify-campaign') {
    state.sellifyImportOpen = true;
    state.sellifyImportNotice = '';
    state.sellifyImportResults = [];
    renderLeadEditor();
    return;
  }
  if (action === 'close-sellify-import' && (trigger === event.target || trigger.tagName === 'BUTTON')) {
    // Waehrend des Imports nicht schliessen (Kopf-Knopf war aktiv, P1 #15).
    if (state.sellifyImportBusy) return;
    state.sellifyImportOpen = false;
    renderLeadEditor();
    return;
  }
  if (action === 'sellify-campaign-search') { await sucheSellifyKampagnen(); return; }
  if (action === 'sellify-campaign-pick') { await importiereSellifyKampagne(trigger.dataset.name || ''); return; }
  if (action === 'resume-import') { await setzeImportFort(trigger.dataset.importId || ''); return; }
  if (action === 'new-campaign') await createCampaign();
  if (action === 'rename-campaign') await renameCampaign(trigger.dataset.campaign || state.selectedCampaign);
  if (action === 'delete-campaign') await deleteCampaign(trigger.dataset.campaign || state.selectedCampaign);
  if (action === 'toggle-lead') {
    toggleLeadSelection(id, Boolean(trigger.checked), event.shiftKey);
    renderCenter();
  }
  if (action === 'toggle-visible-leads') {
    setVisibleLeadSelection(Boolean(trigger.checked));
    renderCenter();
  }
  if (action === 'clear-selection') {
    state.selectedLeadIds.clear();
    state.selectionAnchorId = '';
    renderCenter();
  }
  if (action === 'reset-selection-research') await resetSelectionResearch();
  if (action === 'research-selection') await startSelectionResearch('followup');
  if (action === 'move-selection-campaign') await verschiebeAuswahlInKampagne();
  if (action === 'research-selection-new') await startSelectionResearch('new');
  if (action === 'research-selection-followup') await startSelectionResearch('followup');
  if (action === 'recheck-sellify') {
    // Die Pruefung neu anstossen: Zwischenspeicher leeren, damit die naechste
    // Runde wirklich fragt statt den alten "nicht pruefbar"-Stand zu zeigen.
    const zuPruefen = campaignLeads(state.selectedCampaign);
    state.recipientEligibility.clear();
    state.recipientEligibilityReady.clear();
    state.recipientEligibilitySignatures.clear();
    state.recipientEligibilityTimedOut.clear();
    forgetRecipientEligibility(zuPruefen.map((lead) => lead.id));
    renderCenter();
    renderDetail();
    // Vier Pruefungen gleichzeitig statt eine nach der anderen: 19 Leads mit
    // bis zu 90 s je Lead dauerten sequenziell bis zu einer halben Stunde.
    const warteschlange = [...zuPruefen];
    const arbeiter = Array.from({ length: Math.min(4, warteschlange.length) }, async () => {
      while (warteschlange.length) {
        const lead = warteschlange.shift();
        try { await refreshLeadRecipientEligibility(lead, { force: true }); } catch (error) {
          console.warn('[outbound-lead-generation] Sperrvermerk-Pruefung fehlgeschlagen', error);
        }
        renderCenter();
      }
    });
    await Promise.all(arbeiter);
    renderCenter();
    renderDetail();
    return;
  }
  if (action === 'edit-lead') openLeadEditor(id);
  if (action === 'close-lead-editor' && (event.target === trigger || trigger.matches('button[data-action="close-lead-editor"]'))) {
    closeLeadEditor();
  }
  if (action === 'save-lead-editor') await saveLeadEditor();
  if (action === 'view-mode') {
    state.campaignViewMode = trigger.dataset.view === 'shards' ? 'shards' : 'table';
    renderCenter();
  }
  if (action === 'save-policy') await saveResearchPolicy();
  if (action === 'toggle-optional-field') {
    const key = String(trigger.dataset.field || '');
    state.optionalFieldsDraft = optionalKeysForRequiredCheckbox(optionalFieldsDraft(), key, trigger.checked);
    renderSourcePanel();
    return;
  }
  if (action === 'save-optional-fields') await saveOptionalFields();
  if (action === 'release-empty-field') await setEmptyFieldRelease(String(trigger.dataset.field || ''), true);
  if (action === 'unrelease-empty-field') await setEmptyFieldRelease(String(trigger.dataset.field || ''), false);
  if (action === 'make-field-optional') {
    const key = String(trigger.dataset.field || '');
    if (!key) return;
    state.optionalFieldsDraft = new Set([...optionalResearchFields(), key]);
    await saveOptionalFields();
    return;
  }
  if (action === 'digest-save') await saveUpdateDigest();
  if (action === 'digest-preview') await runUpdateDigest(true);
  if (action === 'digest-send') await runUpdateDigest(false);
  if (action === 'digest-preview-close') {
    state.digestPreview = null;
    renderSourcePanel();
  }
  if (action === 'reset-policy') {
    state.researchPolicyDraft = DEFAULT_RESEARCH_POLICY;
    state.researchPolicyFollowupDraft = '';
    renderSourcePanel();
  }
  if (action === 'toggle-source') await toggleSource(id);
  if (action === 'build-adapter') {
    state.adapterInspectorSourceId = '';
    renderLeadEditor();
    await runAdapterCommand(id, 'outbound.research_source.generate_adapter');
  }
  if (action === 'check-credential') await pruefeZugang(id);
  if (action === 'test-adapter') await runAdapterCommand(id, 'outbound.research_source.test');
  if (action === 'auth-source') await runAdapterCommand(id, 'outbound.research_source.auth_assist');
  if (action === 'delete-adapter') await deleteSourceAdapter(id);
  if (action === 'edit-source') await editSource(id);
  if (action === 'delete-source') await deleteSource(id);
  if (action === 'research-campaign') await startCampaignResearch(trigger.dataset.campaign || state.selectedCampaign);
  if (action === 'research-campaign-gaps') await schliesseKampagnenLuecken(trigger.dataset.campaign || state.selectedCampaign, trigger.dataset.leadIds);
  if (action === 'check-campaign-remarks') await pruefeKampagnenVermerke(trigger.dataset.campaign || state.selectedCampaign);
  if (action === 'track-task') await openCtoxTask(trigger.dataset.taskId || '', trigger.dataset.commandId || '');
  if (action === 'lead-tray-toggle') { state.leadTrayOpen = !state.leadTrayOpen; renderCenter(); return; }
  if (action === 'lead-sort-dir') { state.leadSortDir = state.leadSortDir === 'desc' ? 'asc' : 'desc'; renderCenter(); return; }
  if (action === 'lead-filter-reset') { state.leadStatusFilter.clear(); state.leadSortKey = 'name'; state.leadSortDir = 'asc'; state.search = ''; renderCenter(); return; }
  if (action === 'lead-status-chip') {
    const key = trigger.dataset.status || '';
    if (state.leadStatusFilter.has(key)) state.leadStatusFilter.delete(key); else state.leadStatusFilter.add(key);
    renderCenter(); return;
  }
  if (action === 'research-lead') await researchLead(id, { openChat: true });
  if (action === 'research-lead-new') await researchLead(id, { openChat: true, variant: 'new' });
  if (action === 'research-lead-followup') await researchLead(id, { openChat: true, variant: 'followup' });
  if (action === 'research-lead-gaps') {
    const lead = state.leads.find((entry) => entry.id === id);
    const felder = lead ? offeneRecherchefelder(lead) : [];
    if (felder.length) await researchLead(id, { openChat: true, nurFelder: felder });
  }
  if (action === 'cancel-research') await cancelResearch(id);
  if (action === 'validate-lead') await validateLead(id);
  if (action === 'approve-field') await approveResearchField(trigger.dataset.field || '');
  if (action === 'edit-lead' && !id && selectedLead()) {
    state.leadEditorFocusField = trigger.dataset.field || '';
    openLeadEditor(selectedLead().id);
    return;
  }
  // Der Zustand wird aus der gespeicherten Auswahl abgeleitet, nicht aus
  // `trigger.checked`: bei einem Klick auf das umgebende Label meldet die
  // Checkbox je nach Ereignisreihenfolge noch den alten Wert, und die Auswahl
  // fiel danach still auf 0 zurueck — der Grund, warum die Sellify-Uebergabe
  // nie ansprang.
  // Ein Klick auf die Empfaengerzeile erzeugt ZWEI Klickereignisse: eines vom
  // umgebenden Label, eines von der Checkbox, die der Browser daraufhin selbst
  // ausloest. Gemessen. Beide erreichten den Handler, der die Auswahl damit
  // ab- und sofort wieder anschaltete — die Auswahl bewegte sich nie. Das
  // zweite Ereignis desselben Kontakts wird deshalb verworfen; der Zielzustand
  // kommt aus der gespeicherten Auswahl, nicht aus `trigger.checked` (das bei
  // Label-Klicks je nach Reihenfolge noch den alten Wert meldet).
  if (action === 'toggle-contact-recipient') {
    const contactId = trigger.dataset.contactId || '';
    const stempel = `${id}|${contactId}`;
    const jetzt = Date.now();
    if (state.lastRecipientToggle?.key === stempel && jetzt - state.lastRecipientToggle.at < 400) return;
    state.lastRecipientToggle = { key: stempel, at: jetzt };
    const current = new Set((state.leads.find((entry) => entry.id === id)?.selected_contact_ids) || []);
    await setContactRecipientSelection(id, contactId, !current.has(contactId));
  }
  if (action === 'export-lead-xlsx') {
    const lead = state.leads.find((entry) => entry.id === id);
    if (lead) await exportResearchXlsx([lead], lead.name || lead.id);
  }
  if (action === 'export-campaign-xlsx') {
    const campaign = trigger.dataset.campaign || state.selectedCampaign;
    await exportResearchXlsx(campaignLeads(campaign), campaign);
  }
  if (action === 'export-selection-xlsx') {
    const selected = state.leads.filter((entry) => state.selectedLeadIds.has(entry.id));
    await exportResearchXlsx(selected, `${state.selectedCampaign || 'Auswahl'} Auswahl`);
  }
  if (action === 'sellify-update-only') await sendLeadToSellify(id, { includeCampaign: false });
  if (action === 'sellify-update-campaign') await sendLeadToSellify(id, { includeCampaign: true });
  if (action === 'add-source') await addSource();
}

function handleKeydown(event) {
  if (event.key === 'Enter' && event.target?.matches?.('[data-sellify-campaign-query]')) {
    event.preventDefault();
    void sucheSellifyKampagnen().catch((error) => meldeAktionsfehler(event, error));
    return;
  }
  if (event.key === 'Escape') {
    // Ein offener Dialog schliesst sich selbst; Escape darf nicht zusaetzlich
    // das Panel oder den Editor darunter schliessen (Klicktest-Befund P4 V9).
    if (state.ctx.host.querySelector('.business-dialog-layer')) return;
    // App-Dialoge im [data-app-dialog]: Escape schloss sie nicht (Klicktest P1
    // SIM-04b). Waehrend eines Sellify-Imports bleibt der Dialog offen.
    if (state.sellifyImportOpen) {
      if (!state.sellifyImportBusy) { state.sellifyImportOpen = false; renderLeadEditor(); }
      return;
    }
    if (state.adapterInspectorSourceId) {
      state.adapterInspectorSourceId = '';
      renderLeadEditor();
      return;
    }
    if (state.leadEditorOpen) closeLeadEditor();
    else if (state.sourcePanelOpen) {
      state.sourcePanelOpen = false;
      renderSourcePanel();
    }
    return;
  }
  const detailTab = event.target.closest('[data-action="select-detail-tab"]');
  if (detailTab && ['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) {
    const tabs = [...(detailTab.closest('[role="tablist"]')?.querySelectorAll('[role="tab"]') || [])];
    if (!tabs.length) return;
    event.preventDefault();
    const currentIndex = Math.max(0, tabs.indexOf(detailTab));
    const nextIndex = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : (currentIndex + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    const nextTab = tabs[nextIndex];
    const leadId = String(nextTab.dataset.leadId || state.selectedLeadId || '').trim();
    const tabId = String(nextTab.dataset.detailTab || 'overview').trim();
    if (leadId && DETAIL_TAB_IDS.includes(tabId)) {
      setActiveDetailTab(tabId);
      renderDetail();
      queueMicrotask(() => state.ctx.host.querySelector(`[data-action="select-detail-tab"][data-detail-tab="${tabId}"]`)?.focus());
    }
    return;
  }
  const contactTab = event.target.closest('[data-action="select-contact-tab"]');
  if (contactTab && ['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) {
    const tabs = [...(contactTab.closest('[role="tablist"]')?.querySelectorAll('[role="tab"]') || [])];
    if (!tabs.length) return;
    event.preventDefault();
    const currentIndex = Math.max(0, tabs.indexOf(contactTab));
    const nextIndex = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : (currentIndex + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    const nextTab = tabs[nextIndex];
    const leadId = String(nextTab.dataset.leadId || state.selectedLeadId || '').trim();
    const contactId = String(nextTab.dataset.contactId || '').trim();
    if (leadId && contactId) {
      // Gleiche Form wie im Klickpfad - eine blosse Zeichenkette verlor
      // Kennung und Personenschluessel und damit die Auswahl.
      state.activeContactTabs.set(leadId, {
        id: contactId,
        identity: String(nextTab.dataset.contactIdentity || '').trim(),
        personKey: String(nextTab.dataset.personKey || '').trim(),
      });
      renderDetail();
      queueMicrotask(() => {
        [...document.querySelectorAll('[data-action="select-contact-tab"]')]
          .find((entry) => entry.dataset.leadId === leadId && entry.dataset.contactId === contactId)
          ?.focus();
      });
    }
    return;
  }
  const row = event.target.closest('[data-action="select-lead"]');
  if (!row || !['Enter', ' '].includes(event.key)) return;
  event.preventDefault();
  const id = row.dataset.id || '';
  if (event.key === ' ' || event.metaKey || event.ctrlKey) {
    toggleLeadSelection(id, !state.selectedLeadIds.has(id), event.shiftKey);
    renderCenter();
    return;
  }
  selectLeadAndRefreshEligibility(id);
}

function selectLeadAndRefreshEligibility(id) {
  state.selectedLeadId = id || '';
  state.selectedDetailError = '';
  renderCenter();
  renderDetail();
  void loadSelectedLeadDetails();
}

// EINE Definition von "sichtbar" fuer Tabelle, Kopf-Checkbox und
// Umschalt-Bereichsauswahl. Vorher filterte die Tabelle zusaetzlich nach Status
// und sortierte, die Auswahl nicht, und die Suche verglich andere Felder: die
// Kopf-Checkbox waehlte bei 0 sichtbaren Zeilen 4 unsichtbare Leads, der
// Umschalt-Klick einen anderen Bereich als den angezeigten (Klicktest P2
// CEN-16c, CEN-18).
function angezeigteLeads() {
  const needle = state.search.trim().toLowerCase();
  const statusFilter = state.leadStatusFilter;
  const leads = listLeads().filter((lead) => {
    // Auch zusaetzliche Kampagnen-Mitglieder zeigen: die Liste zaehlte 55
    // Firmen, die Tabelle zeigte nur die 29 eigenen (26.09.2026).
    return (!state.selectedCampaign || leadKampagnen(lead).includes(state.selectedCampaign))
      && (!needle || `${lead.name} ${lead.domain} ${lead.website} ${lead.city}`.toLowerCase().includes(needle))
      && (!statusFilter.size || statusFilter.has(effektiverRechercheStatus(lead)));
  });
  const dir = state.leadSortDir === 'desc' ? -1 : 1;
  const sortVal = (lead) => state.leadSortKey === 'status' ? String(lead.research_status || '')
    : state.leadSortKey === 'ort' ? String(lead.city || '')
    : state.leadSortKey === 'aktualisiert' ? Number(lead.research_updated_at_ms || lead.updated_at_ms || 0)
    : String(lead.name || '');
  leads.sort((a, b) => {
    const va = sortVal(a); const vb = sortVal(b);
    return (typeof va === 'number' ? va - vb : String(va).localeCompare(String(vb), 'de')) * dir;
  });
  return leads;
}

function visibleCampaignLeads() {
  return angezeigteLeads();
}

function selectedVisibleLeadCount(leads = visibleCampaignLeads()) {
  return leads.filter((lead) => state.selectedLeadIds.has(lead.id)).length;
}

function toggleLeadSelection(id, selected, range = false) {
  if (!id) return;
  const visible = visibleCampaignLeads();
  if (range && state.selectionAnchorId) {
    const start = visible.findIndex((lead) => lead.id === state.selectionAnchorId);
    const end = visible.findIndex((lead) => lead.id === id);
    if (start >= 0 && end >= 0) {
      for (const lead of visible.slice(Math.min(start, end), Math.max(start, end) + 1)) {
        if (selected) state.selectedLeadIds.add(lead.id);
        else state.selectedLeadIds.delete(lead.id);
      }
      return;
    }
  }
  if (selected) state.selectedLeadIds.add(id);
  else state.selectedLeadIds.delete(id);
  state.selectionAnchorId = id;
}

function setVisibleLeadSelection(selected) {
  for (const lead of visibleCampaignLeads()) {
    toggleLeadSelection(lead.id, selected);
  }
  state.selectionAnchorId = '';
}

// Sichtbare Rueckmeldung fuer Sammelaktionen. state.notice wurde frueher zwar
// gesetzt, aber nirgends gerendert - der Knopf arbeitete stumm (B1).
let hinweisTimer = 0;
// Laufende Vorgaenge zeigen sofort und dann jede Sekunde, was passiert
// (Owner 25.09.2026: nie laenger als 1 s ohne sichtbare Rueckmeldung).
function laufanzeige(textFuerSekunden) {
  const start = Date.now();
  const zeige = () => {
    state.sellifyImportNotice = textFuerSekunden(Math.floor((Date.now() - start) / 1000));
    const ziel = state.ctx?.host?.querySelector?.('[data-sellify-import-notice]');
    if (ziel) ziel.textContent = state.sellifyImportNotice;
    else renderLeadEditor();
  };
  zeige();
  const timer = setInterval(zeige, 1000);
  return () => clearInterval(timer);
}

function zeigeHinweis(text, verweildauerMs = 12000) {
  state.notice = String(text || '');
  if (hinweisTimer) { clearTimeout(hinweisTimer); hinweisTimer = 0; }
  if (state.notice && verweildauerMs > 0) {
    hinweisTimer = setTimeout(() => {
      hinweisTimer = 0;
      state.notice = '';
      renderCenter();
    }, verweildauerMs);
  }
  renderCenter();
}

function sellifyBekanntOhneAbfrage(lead) {
  const payload = lead?.payload || {};
  return Boolean(
    payload.imported_row?.sellify_contact_id
    || payload.sellify_precheck?.known === true
    || payload.sellify_snapshot?.contact_id,
  );
}

async function startSelectionResearch(variant = 'followup') {
  await ensureFullLeads([...state.selectedLeadIds]);
  // Owner-Befund 04.09.2026: 19 Leads angehakt, Knopf meldet "Bitte mindestens
  // einen Lead auswaehlen". Ursache: die Auswahl wurde zusaetzlich gegen
  // `state.selectedCampaign` geschnitten. Nach einer Kampagnen-Umbenennung
  // passt der Name im Zustand nicht mehr zu den Leads - die Schnittmenge ist
  // leer, obwohl sichtbar alles angehakt ist.
  //
  // Die Auswahl IST der Umfang. Sie braucht keinen Kampagnenfilter.
  const leads = state.leads.filter((lead) => state.selectedLeadIds.has(lead.id));
  if (!leads.length) {
    await showBusinessAlert('Bitte mindestens einen Lead auswählen.');
    return null;
  }
  const bezeichnung = variant === 'new' ? 'Neurecherche' : 'Nachrecherche';
  // Ein zweiter Klick waehrend des Sellify-Vorabgleichs startete einen
  // zweiten Lauf; der Lead blieb als "Wartet" ohne Befehl und ohne
  // Abbruchmoeglichkeit stehen (Klicktest P2 SEL-03b / V2).
  if (state.auswahlStartLaeuft) {
    zeigeHinweis('Die Recherche für die Auswahl wird bereits gestartet …');
    return null;
  }
  // Die Owner-Weiche (Neue Recherche nur fuer Firmen, die Sellify nicht kennt)
  // bleibt hart. Aus Sellify importierte Leads sind aber per Definition
  // bekannt: statt jeden einzeln abzufragen und nach einer halben Stunde
  // "nicht gestartet" zu melden, fragt die App sofort, ob fuer sie die
  // Nachrecherche laufen soll (Euphrasie, 25.09.2026: 75 Leads).
  let neu = leads;
  let nach = [];
  if (variant === 'new') {
    const bekannt = leads.filter(sellifyBekanntOhneAbfrage);
    if (bekannt.length) {
      const umstellen = await showBusinessConfirm(
        `${bekannt.length} von ${leads.length} ausgewählten Firmen sind bereits in Sellify geführt (aus Sellify importiert). `
        + 'Für sie ist nur eine Nachrecherche möglich.'
        + (bekannt.length < leads.length ? ` Die übrigen ${leads.length - bekannt.length} werden neu recherchiert.` : ''),
        { title: 'Firmen bereits in Sellify', confirmLabel: `Nachrecherche für ${bekannt.length} starten`, cancelLabel: 'Abbrechen' },
      );
      if (!umstellen) return null;
      const bekannteIds = new Set(bekannt.map((lead) => lead.id));
      nach = bekannt;
      neu = leads.filter((lead) => !bekannteIds.has(lead.id));
    }
  }
  state.auswahlStartLaeuft = true;
  try {
    const kampagne = leads[0]?.campaign || state.selectedCampaign;
    let ergebnis = null;
    if (nach.length) {
      ergebnis = await startScopedResearch(kampagne, nach, {
        scope: 'selection',
        variant: 'followup',
        title: `Nachrecherche: ${nach.length} ausgewählte Leads`,
      });
    }
    if (neu.length) {
      ergebnis = await startScopedResearch(kampagne, neu, {
        scope: 'selection',
        variant,
        title: `${bezeichnung}: ${neu.length} ausgewählte Leads`,
      }) || ergebnis;
    }
    return ergebnis;
  } finally {
    state.auswahlStartLaeuft = false;
  }
}

function payloadWithoutResearchResults(payload = {}) {
  const cleaned = {};
  const exactResearchKeys = new Set([
    'browser_assist_tasks',
    'campaign_command_id',
    'campaign_run_id',
    'campaign_task_id',
    'operator_approved_field_keys',
    'operator_approved_person_fields',
    'researched_field_keys',
    'unverified_field_keys',
    'validated_at_ms',
    'verified_field_keys',
  ]);
  for (const [key, value] of Object.entries(payload || {})) {
    const normalizedKey = String(key).toLowerCase();
    if (exactResearchKeys.has(normalizedKey)) continue;
    if (normalizedKey.startsWith('research_')) continue;
    if (normalizedKey.startsWith('last_research_')) continue;
    if (normalizedKey.startsWith('observed_research_')) continue;
    if (normalizedKey.startsWith('reconciled_research_')) continue;
    cleaned[key] = value;
  }
  return cleaned;
}

function researchResetLeadPatch(lead) {
  // Der Dialog verspricht "Firmenstammdaten bleiben erhalten"; geloescht wurde
  // aber auch, was der Nutzer von Hand gepflegt hatte (z. B. Verantwortlicher)
  // — Klicktest P2 SEL-01d. Pflegefelder und von Hand geaenderte Felder
  // bleiben samt ihrem Nutzerbeleg; nur Rechercheergebnisse gehen.
  // Pflegefelder, die zugleich Recherchefelder sind, gehoeren zum
  // Rechercheergebnis und gehen mit (Review 3).
  const rechercheFelder = new Set(RESEARCH_FIELDS);
  const behalten = new Set([
    ...GOVERNANCE_FIELDS.filter((key) => !rechercheFelder.has(key)),
    ...(lead?.payload?.manually_edited_field_keys || []),
  ]);
  const data = Object.fromEntries(Object.entries(lead?.data || {}).filter(([key]) => behalten.has(key)));
  const evidence = (lead?.evidence || []).filter((entry) => String(entry?.source_id || '') === 'operator'
    && entry?.via === 'manual-edit'
    && behalten.has(String(entry?.field_key || entry?.field || '')));
  return {
    research_status: 'new',
    research_error: '',
    research_updated_at_ms: Date.now(),
    validation_status: 'pending',
    task_id: '',
    command_id: '',
    data,
    contacts: [],
    selected_contact_ids: [],
    evidence,
    // Der Feldstatus IST ein Rechercheergebnis. Gemessen am 09.09.2026: nach
    // "Ergebnisse löschen" waren Werte, Belege und Ansprechpartner weg, aber
    // alle 32 Feldstatus standen weiter da — die Freigabeansicht zeigte
    // "verifiziert" für Werte, die es nicht mehr gab. Der Zusage im Dialog
    // ("Rechercheergebnisse, Belege und Ansprechpartner werden dauerhaft
    // gelöscht") widerspricht das.
    field_status: {},
    research_phase: '',
    gap_task_id: '',
    payload: payloadWithoutResearchResults(lead?.payload || {}),
  };
}

async function resetSelectionResearch() {
  const campaign = String(state.selectedCampaign || '').trim();
  const leads = campaignLeads(campaign)
    .filter((lead) => state.selectedLeadIds.has(lead.id));
  if (!leads.length) {
    await showBusinessAlert('Bitte mindestens einen Lead auswählen.');
    return 0;
  }
  const running = leads.filter((lead) => (
    ['queued', 'running'].includes(String(lead.research_status || ''))
    && !staleCampaignParentPlaceholder(lead, state.campaignRuns.get(campaign))
  ));
  if (running.length) {
    await showBusinessAlert(`Die Ergebnisse können nicht gelöscht werden, solange ${running.length} Recherche-Lauf${running.length === 1 ? '' : 'e'} aktiv ${running.length === 1 ? 'ist' : 'sind'}.`);
    return 0;
  }
  const confirmed = await showBusinessConfirm(
    `Die bisherigen Rechercheergebnisse, Belege und Ansprechpartner von ${leads.length} Lead${leads.length === 1 ? '' : 's'} in „${campaign}“ werden dauerhaft gelöscht. Firmenstammdaten und Kampagnenzuordnung bleiben erhalten.`,
    {
      title: 'Rechercheergebnisse löschen',
      confirmLabel: 'Ergebnisse löschen',
      requireText: campaign,
      kind: 'danger',
    },
  );
  if (!confirmed) return 0;
  await Promise.all(leads.map((lead) => patchLead(lead.id, researchResetLeadPatch(lead))));
  state.campaignRuns.delete(campaign);
  await reload();
  render();
  await showBusinessAlert(`${leads.length === 1 ? 'Das Rechercheergebnis wurde' : `${leads.length} Rechercheergebnisse wurden`} gelöscht. Die Auswahl bleibt für den Neustart erhalten.`);
  return leads.length;
}

// Hat CTOX den Befehl angenommen? Liest nur den lokal replizierten
// business_commands-Datensatz; ein rein lokaler Entwurf zaehlt nicht.
async function befehlAmServer(commandId) {
  const id = String(commandId || '').trim();
  if (!id) return false;
  try {
    const doc = await state.ctx?.db?.collection?.('business_commands')?.findOne(id).exec();
    const befehl = doc?.toJSON?.() || doc;
    if (!befehl) return false;
    const phase = String(befehl.replication_phase || '').toLowerCase();
    const status = String(befehl.status || '').toLowerCase();
    return phase === 'native_observed' || ['accepted', 'queued', 'running', 'leased', 'completed'].includes(status);
  } catch {
    return false;
  }
}

async function startCampaignResearch(campaignName) {
  const campaign = String(campaignName || '').trim();
  await ensureFullLeads(campaignListLeads(campaign).map((lead) => lead.id));
  const leads = campaignLeads(campaign);
  return startScopedResearch(campaign, leads, {
    scope: 'campaign',
    title: `Kampagnenrecherche: ${campaign}`,
  });
}

async function startScopedResearch(campaign, leads, options = {}) {
  const configuration = await researchConfigurationReadiness();
  if (!configuration.ready) {
    if (configuration.reconcile) {
      void queueAdapterReconciliationAfterSourceChange('research_start_requires_current_adapters');
    }
    showBusinessAlert(configuration.error);
    return null;
  }
  const campaignRun = state.campaignRuns.get(campaign);
  const eligibleIds = options.scope === 'selection'
    ? new Set((leads || [])
      .filter((lead) => lead.validation_status !== 'validated')
      .filter((lead) => !['queued', 'running'].includes(lead.research_status)
        || staleCampaignParentPlaceholder(lead, campaignRun))
      .map((lead) => lead.id))
    : new Set(campaignResearchQueue(leads));
  const eligibleLeads = (leads || []).filter((lead) => eligibleIds.has(lead.id));
  const validation = validateCampaignResearchRequest({ campaign, leads: eligibleLeads });
  if (!validation.valid) {
    showBusinessAlert(validation.error);
    return null;
  }
  if (options.scope === 'campaign' && state.campaignRuns.get(campaign)?.status === 'running') {
    showBusinessAlert(tr('campaignAlreadyRunning', 'Für diese Kampagne läuft bereits eine Recherche.'));
    return null;
  }

  const runId = `leadgen_${options.scope || 'campaign'}_research_${crypto.randomUUID()}`;
  const runKey = options.scope === 'campaign' ? campaign : runId;
  const run = {
    id: runId,
    taskId: '',
    commandId: '',
    status: 'running',
    currentLeadId: '',
    currentLeadName: '',
    startedAtMs: Date.now(),
    finishedAtMs: 0,
    error: '',
  };
  state.campaignRuns.set(runKey, run);
  renderCenter();

  // Ein Kampagnenstart ist EIN Auftrag JE LEAD: derselbe Weg wie der einzelne
  // Nachrecherche-Knopf (ein Satz, der CTOX-Agent arbeitet nach Skill). Die
  // CTOX-Queue serialisiert die Auftraege und ueberlebt geschlossene Tabs; der
  // frueher hier erzeugte Orchestrator-Chat mit Riesen-Prompt entfaellt.
  const queued = [];
  const failed = [];
  // Owner-Befund 04.09.2026: "wenn ich auf Auswahl Recherchieren klicke,
  // passiert absolut gar nichts". Die Laeufe starteten sehr wohl - aber ohne
  // Chatfenster und ohne Hinweis war das nicht zu sehen. Der erste Lauf oeffnet
  // deshalb wieder ein Chatfenster, und die Leiste sagt sofort, was passiert.
  zeigeHinweis(`${eligibleLeads.length} Recherche${eligibleLeads.length === 1 ? ' wird' : 'n werden'} gestartet …`, 0);
  // Eine Uebergabe, die im 30-Sekunden-Fenster nicht bestaetigt wurde, ist NICHT
  // bewiesen fehlgeschlagen — sie ist unbestaetigt. thesen 09.09.2026: von drei
  // Firmen lief eine nicht an, ohne dass ein Befehl am Server ankam, waehrend
  // derselbe Lead einzeln sofort startete. Ein zweiter Versuch nach kurzer Pause
  // kostet nichts und rettet den Lead; erst danach zaehlt er als gescheitert.
  const istUnbestaetigt = (grund) => /nicht rechtzeitig|nicht best(ä|ae)tigt|R(ü|ue)ckmeldung steht aus|wartet noch auf die R(ü|ue)ckmeldung|timed out|timeout|Zeit(ü|ue)berschreitung/i.test(String(grund || ''));
  const starteEinen = async (lead, index) => {
    try {
      let outcome = await researchLead(lead.id, {
        openChat: index === 0,
        // Owner-Befund 07.09.2026: "Alle recherchieren" auf eine frische Kampagne
        // erzwang "Nachrecherche" fuer alle 19 Leads; 18 davon kennt Sellify
        // nicht, also brach jeder einzelne mit "nur Neue Recherche moeglich" ab.
        // Ohne Vorgabe entscheidet der Sellify-Vorabgleich je Lead (bekannt =
        // Nachrecherche, unbekannt = Neue Recherche). Die Owner-Weiche bleibt
        // fuer die ausdruecklichen Knoepfe "neu"/"nach" unveraendert hart.
        variant: options.variant || '',
        campaignRunId: runId,
        // Im Sammellauf oeffnet sonst JEDER blockierte Lead seinen eigenen
        // Dialog - bei 19 Firmen 19 Dialoge hintereinander. Die Gruende
        // sammelt der Lauf und nennt sie einmal am Ende.
        suppressAlerts: true,
      });
      const gestartet = (ergebnis) => ergebnis && ['queued', 'running', 'submitting'].includes(ergebnis.status);
      if (!gestartet(outcome) && istUnbestaetigt(outcome?.error || outcome?.status)) {
        await new Promise((resolve) => setTimeout(resolve, 3000));
        // Kam der erste Befehl doch an, kein zweiter Auftrag: sonst liefen zwei
        // Recherchen fuer denselben Lead, abbrechbar nur die letzte
        // (Klicktest-Befund P2 V16).
        if (await befehlAmServer(outcome?.commandId)) {
          queued.push({ id: lead.id, commandId: outcome.commandId });
          return;
        }
        outcome = await researchLead(lead.id, {
          openChat: false,
          variant: options.variant || '',
          campaignRunId: runId,
          suppressAlerts: true,
        });
      }
      if (gestartet(outcome)) {
        queued.push({ id: lead.id, commandId: outcome.commandId || '' });
      } else {
        failed.push({ id: lead.id, error: outcome?.error || outcome?.status || 'unbekannt' });
      }
    } catch (error) {
      const grund = String(error?.message || error);
      if (istUnbestaetigt(grund)) {
        try {
          await new Promise((resolve) => setTimeout(resolve, 3000));
          const zweiter = await researchLead(lead.id, {
            openChat: false,
            variant: options.variant || '',
            campaignRunId: runId,
            suppressAlerts: true,
          });
          if (zweiter && ['queued', 'running', 'submitting'].includes(zweiter.status)) {
            queued.push({ id: lead.id, commandId: zweiter.commandId || '' });
            return;
          }
          failed.push({ id: lead.id, error: zweiter?.error || zweiter?.status || grund });
          return;
        } catch (zweiterFehler) {
          failed.push({ id: lead.id, error: String(zweiterFehler?.message || zweiterFehler) });
          return;
        }
      }
      failed.push({ id: lead.id, error: grund });
    }
  };
  // Drei Starts nebeneinander statt strikt nacheinander: jeder Start wartet auf
  // den Sellify-Vorabgleich (10-20 s), 75 Leads hingen sonst eine halbe Stunde
  // auf "Wird gestartet" ohne sichtbaren Fortschritt (Euphrasie, 25.09.2026).
  let fertig = 0;
  let naechster = 0;
  const melde = () => zeigeHinweis(`${fertig} / ${eligibleLeads.length} Recherche${eligibleLeads.length === 1 ? '' : 'n'} gestartet oder geprüft …`, 0);
  const arbeiter = async () => {
    while (naechster < eligibleLeads.length) {
      const index = naechster;
      naechster += 1;
      await starteEinen(eligibleLeads[index], index);
      fertig += 1;
      melde();
    }
  };
  await Promise.all(Array.from({ length: Math.min(3, eligibleLeads.length) }, arbeiter));
  if (!queued.length) {
    const message = failed.length
      ? `Kein Lead konnte gestartet werden (${failed.length} Fehler): ${failed.slice(0, 3).map((entry) => entry.error).join(' \u00b7 ')}`
      : 'Kein Lead war fuer die Recherche offen.';
    zeigeHinweis(message);
    Object.assign(run, { status: 'failed', finishedAtMs: Date.now(), error: message });
    renderCenter();
    showBusinessAlert(message);
    return null;
  }
  if (failed.length) {
    const gruende = [...new Set(failed.map((eintrag) => String(eintrag.error || '').trim()).filter(Boolean))];
    zeigeHinweis(`${queued.length} gestartet, ${failed.length} nicht gestartet.`, 30000);
    await showBusinessAlert(
      `${queued.length} Recherche${queued.length === 1 ? '' : 'n'} gestartet. `
      + `${failed.length} Lead${failed.length === 1 ? '' : 's'} wurde${failed.length === 1 ? '' : 'n'} nicht gestartet:\n\n`
      + gruende.slice(0, 5).map((grund) => `\u2022 ${grund}`).join('\n')
      + (gruende.length > 5 ? `\n\u2026 und ${gruende.length - 5} weitere Gruende.` : ''),
    );
  } else {
    zeigeHinweis(`${queued.length} Recherche${queued.length === 1 ? '' : 'n'} gestartet.`);
  }
  // Jeder Lead bekam einen EIGENEN Befehl. Ihn mit der Befehls-ID des ersten
  // Leads zu stempeln, haengt die Statusverfolgung von Lead 2..n an einen
  // fremden Vorgang: gemessen am 09.09.2026 blieben Aeroxon, Beiersdorf und
  // Carbosulf nach einem sauberen Sammelstart auf ihrem alten Stand stehen,
  // obwohl drei Befehle angenommen waren.
  const commandIdByLead = new Map(queued.map((entry) => [entry.id, entry.commandId]));
  const submissionFor = (leadId) => ({ task_id: '', command_id: commandIdByLead.get(leadId) || '' });
  const submission = submissionFor(queued[0].id);
  Object.assign(run, {
    taskId: submission.task_id,
    commandId: submission.command_id,
    status: 'running',
    queuedCount: queued.length,
    failedCount: failed.length,
  });
  if (failed.length) {
    console.warn('[olg-trace] campaign start: leads not queued', failed.slice(0, 5));
  }
  const queuedAtMs = Date.now();
  // Nur die Leads, deren Befehl bestaetigt ist, wandern auf "Wartet". Ein Lead,
  // der nicht angelaufen ist, behaelt seinen Stand und seine Fehlermeldung.
  const gestarteteLeads = eligibleLeads.filter((lead) => commandIdByLead.has(lead.id));
  const aktuellerStand = (lead) => state.leads.find((entry) => entry.id === lead.id) || lead;
  for (const lead of gestarteteLeads) {
    const patch = campaignQueuedLeadPatch(aktuellerStand(lead), submissionFor(lead.id), runId, queuedAtMs);
    Object.assign(lead, patch);
  }
  renderCenter();
  try {
    await Promise.all(gestarteteLeads.map(async (lead) => {
      const doc = await state.collections.leads.findOne(lead.id).exec();
      const current = doc?.toJSON?.() || aktuellerStand(lead);
      return patchCampaignQueuedLeadIfCurrent(
        lead.id,
        campaignQueuedLeadPatch(current, submissionFor(lead.id), runId, queuedAtMs),
      );
    }));
  } catch (error) {
    // Der serverseitige Task ist bereits bestaetigt und darf nicht als
    // fehlgeschlagen bezeichnet werden. Seine eigenen Upserts und der
    // Command-Abgleich reparieren eine verzoegerte Browser-Projektion.
    run.error = `Task gestartet; Statusprojektion verzoegert: ${String(error?.message || error)}`;
  }
  await reload().catch(() => {});
  if (options.scope === 'selection') {
    state.selectedLeadIds.clear();
    state.selectionAnchorId = '';
    renderCenter();
  }
  return runId;
}

async function researchConfigurationReadiness() {
  // Die Recherche macht seit 1.0.65 der CTOX-Agent ueber den Web-Stack
  // (Skill `outbound-lead-generation-research`). Adapter sind dabei
  // Hilfswerkzeuge fuer die Stapelphase, keine Voraussetzung: ein fehlender
  // oder fehlgeschlagener Adapter ist eine Quelle weniger, kein Grund, den
  // Auftrag zu verweigern. Der frueher hier verankerte Abgleich-Zwang
  // (configuration_digest + reconciliation_status + Adapter-Status) gehoerte
  // zum nativen Adapter-Lauf und blockierte am 02.09. die komplette Kampagne,
  // obwohl jeder Einzel-Auftrag sauber lief. Er ist jetzt ein Hinweis.
  const sources = state.sources.filter((source) => !isInternalResearchSource(source) && source.enabled !== false);
  if (!sources.length) {
    return { ready: false, reconcile: false, error: 'Mindestens eine aktive Recherchequelle ist erforderlich.' };
  }
  const policy = state.researchPolicyRecord || researchPolicyRecord(null, state.researchPolicyDraft);
  const expectedDigest = await adapterConfigurationDigest(policy, sources);
  const digestChanged = String(policy.configuration_digest || '').trim() !== expectedDigest;
  // Gleiche Konfiguration heisst: nichts abzugleichen. Kein Hintergrundlauf,
  // keine Reparaturauftraege, keine belegten Worker.
  if (digestChanged) {
    // Abgleich im Hintergrund anstossen, aber den Lauf nicht aufhalten.
    void queueAdapterReconciliationAfterSourceChange('research_start_refreshes_adapters');
  }
  return { ready: true, reconcile: false, error: '', note: digestChanged ? 'Adapter werden im Hintergrund abgeglichen.' : '' };
}

// `lead` muss der AKTUELLE Stand sein: researchLead hat den Lead beim Start
// bereits auf "running" gesetzt und task_id/research_started_at_ms
// geschrieben. Mit dem Schnappschuss von vor dem Start stufte der Patch
// "Laeuft" auf "Wartet" zurueck, loeschte die Task-ID ("Task oeffnen" erschien
// nie) und den Startzeitpunkt (die 3-h-Rueckfallgrenze griff nie) —
// Klicktest-Befunde P2 V3/V7.
function campaignQueuedLeadPatch(lead, submission, runId, queuedAtMs = Date.now()) {
  const commandId = String(submission?.command_id || '').trim() || String(lead?.command_id || '').trim();
  // Eine alte Task-ID eines frueheren Laufs darf nicht an den neuen Befehl
  // (Review 6): nur uebernehmen, wenn der Lead denselben Befehl traegt.
  const taskId = String(submission?.task_id || '').trim()
    || (String(lead?.command_id || '').trim() === commandId ? String(lead?.task_id || '').trim() : '');
  return {
    research_status: lead?.research_status === 'running' ? 'running' : 'queued',
    research_error: '',
    research_updated_at_ms: queuedAtMs,
    task_id: taskId,
    command_id: commandId,
    payload: {
      ...(lead?.payload || {}),
      campaign_run_id: runId,
      campaign_task_id: taskId,
      campaign_command_id: commandId,
      research_queued_at_ms: queuedAtMs,
    },
  };
}

// Der Parent-Task kann seine ersten Child-Commands bereits abschliessen,
// waehrend der Browser noch die 19 queued-Platzhalter repliziert. Ein normaler
// patchLead wuerde dann den nativen Child-Writeback wieder mit der Parent-ID und
// leeren Daten ueberschreiben. Innerhalb desselben Kampagnenlaufs gewinnt daher
// immer ein bereits beobachteter Child-Command gegen den Parent-Platzhalter.
async function patchCampaignQueuedLeadIfCurrent(id, patch) {
  const doc = await state.collections.leads.findOne(id).exec();
  if (!doc) return false;
  const current = doc.toJSON?.() || doc;
  const currentRunId = String(current?.payload?.campaign_run_id || '').trim();
  const nextRunId = String(patch?.payload?.campaign_run_id || '').trim();
  const currentCommandId = String(current?.command_id || '').trim();
  const parentCommandId = String(patch?.command_id || '').trim();
  if (currentRunId && currentRunId === nextRunId
    && currentCommandId && currentCommandId !== parentCommandId) {
    return false;
  }
  await patchLead(id, patch);
  return true;
}

function staleCampaignParentPlaceholder(lead, run = null, nowMs = Date.now()) {
  if (['queued', 'running'].includes(String(run?.status || ''))) return false;
  if (String(lead?.research_status || '') !== 'queued') return false;
  const taskId = String(lead?.task_id || '').trim();
  const commandId = String(lead?.command_id || '').trim();
  const parentTaskId = String(lead?.payload?.campaign_task_id || '').trim();
  const parentCommandId = String(lead?.payload?.campaign_command_id || '').trim();
  const queuedAtMs = Number(lead?.payload?.research_queued_at_ms || 0);
  return Boolean(taskId
    && commandId
    && taskId === parentTaskId
    && commandId === parentCommandId
    && queuedAtMs > 0
    && nowMs - queuedAtMs >= CAMPAIGN_PARENT_PLACEHOLDER_STALE_MS);
}

function campaignTaskStatus(command) {
  const terminal = String(command?.terminal_status || '').trim().toLowerCase();
  const raw = terminal && terminal !== 'none'
    ? terminal
    : String(command?.task_status || command?.status || '').trim().toLowerCase();
  const error = String(command?.error || command?.result?.error || '').trim();
  // A queue task can already be terminal while an older business-command
  // projection still says accepted/queued. The terminal queue projection does
  // carry its failure note, so use that durable evidence after the normal
  // reconciliation grace period instead of trapping every lead in "Wartet".
  if (error
    && ['accepted', 'pending', 'queued', 'routing'].includes(raw)) {
    return 'failed';
  }
  if (['accepted', 'pending', 'queued', 'routing'].includes(raw)) return 'queued';
  if (['running', 'leased', 'working', 'retry_wait'].includes(raw)) return 'running';
  if (raw === 'completed') return 'completed';
  if (['failed', 'blocked', 'cancelled', 'canceled'].includes(raw)) return 'failed';
  return 'queued';
}

function campaignResearchCommandForCampaign(campaign, commands = []) {
  const recordId = `campaign:${String(campaign || '').trim()}`;
  return commands
    .filter((command) => command?.command_type === 'business_os.chat.task')
    .filter((command) => String(command?.source_module || command?.module || '').trim() === 'outbound-lead-generation')
    .filter((command) => String(command?.record_id || '').trim() === recordId)
    .sort((left, right) => Number(
      right?.created_at_ms || right?.updated_at_ms || 0,
    ) - Number(left?.created_at_ms || left?.updated_at_ms || 0))[0] || null;
}

function campaignRunFromCommand(campaign, command) {
  if (!command) return null;
  const commandId = String(command.command_id || command.id || '').trim();
  const taskId = String(command.task_id || command.taskId || '').trim();
  if (!commandId || !taskId) return null;
  const status = campaignTaskStatus(command);
  return {
    id: commandId,
    taskId,
    commandId,
    status,
    currentLeadId: '',
    currentLeadName: '',
    startedAtMs: Number(command.created_at_ms || 0),
    finishedAtMs: ['completed', 'failed'].includes(status) ? Number(command.updated_at_ms || 0) : 0,
    error: status === 'failed' ? String(command?.error || command?.result?.error || '') : '',
    campaign: String(campaign || '').trim(),
  };
}

function terminalCampaignQueuedLeadPatch(lead, command, nowMs = Date.now()) {
  const status = campaignTaskStatus(command);
  const taskId = String(command?.task_id || command?.taskId || '').trim();
  const commandId = String(command?.command_id || command?.id || '').trim();
  const terminalAtMs = Number(command?.updated_at_ms || command?.created_at_ms || 0);
  if (!['completed', 'failed'].includes(status)
    || !taskId
    || String(lead?.research_status || '') !== 'queued'
    || String(lead?.task_id || '') !== taskId
    || !terminalAtMs
    || nowMs - terminalAtMs < CAMPAIGN_TERMINAL_REPAIR_GRACE_MS) {
    return null;
  }
  return {
    research_status: 'failed',
    task_id: '',
    command_id: '',
    research_error: 'Der Kampagnen-Task ist beendet, bevor für diesen Lead ein Recherche-Command gestartet wurde. Der Lead kann erneut eingeplant werden.',
    research_updated_at_ms: nowMs,
    payload: {
      ...(lead?.payload || {}),
      campaign_parent_task_id: taskId,
      campaign_parent_command_id: commandId,
      campaign_parent_terminal_status: status,
      campaign_parent_reconciled_at_ms: nowMs,
    },
  };
}

async function reconcileCampaignResearchRuns({ authoritative = false } = {}) {
  if (state.reconcilingCampaignRuns) return false;
  const campaigns = campaignRows().map((campaign) => campaign.name).filter(Boolean);
  if (!campaigns.length) return false;
  state.reconcilingCampaignRuns = true;
  let changed = false;
  try {
    const commandIds = [...new Set([
      ...state.campaignRuns.values().map((run) => String(run?.commandId || '').trim()),
      ...state.leads
        .filter((lead) => ['queued', 'running'].includes(String(lead?.research_status || '')))
        .map((lead) => String(lead?.payload?.campaign_command_id || '').trim()),
    ].filter(Boolean))];
    const commands = await loadCommandStatuses(commandIds, { authoritative, quelle: 'kampagne' });
    const repairs = [];
    for (const campaign of campaigns) {
      const command = campaignResearchCommandForCampaign(campaign, commands);
      const next = campaignRunFromCommand(campaign, command);
      if (!next) continue;
      for (const lead of campaignLeads(campaign)) {
        const patch = terminalCampaignQueuedLeadPatch(lead, command);
        if (!patch) continue;
        Object.assign(lead, patch);
        repairs.push(patchLead(lead.id, patch));
        changed = true;
      }
      const current = state.campaignRuns.get(campaign);
      const before = current ? JSON.stringify(current) : '';
      if (before === JSON.stringify(next)) continue;
      state.campaignRuns.set(campaign, next);
      changed = true;
    }
    if (repairs.length) {
      const outcomes = await Promise.allSettled(repairs);
      for (const outcome of outcomes) {
        if (outcome.status === 'rejected') {
          console.warn('[outbound-lead-generation] Verwaisten Kampagnenstatus konnte nicht repariert werden', outcome.reason);
        }
      }
    }
  } finally {
    state.reconcilingCampaignRuns = false;
  }
  return changed;
}

function campaignLeads(campaign) {
  return state.leads.filter((lead) => leadKampagnen(lead).includes(campaign));
}
function heimatLeads(campaign) {
  return state.leads.filter((lead) => heimatKampagne(lead) === campaign);
}

function campaignResearchQueue(leads) {
  return (leads || [])
    // Kampagnenweit nur neue und explizit fehlgeschlagene Leads starten.
    // needs_review/completed sind Ergebnisse fuer die menschliche Pruefung;
    // queued/running gehoeren bereits zu einem dauerhaften Lauf. Eine bewusst
    // gewaehlte Auswahl darf diese Leads weiterhin einzeln nachrecherchieren.
    .filter((lead) => lead.validation_status !== 'validated')
    .filter((lead) => ['new', 'failed'].includes(String(lead.research_status || 'new')))
    .map((lead) => lead.id);
}

function validateCampaignResearchRequest({ campaign, leads }) {
  if (!String(campaign || '').trim()) {
    return { valid: false, error: tr('selectCampaignFirst', 'Bitte zuerst eine Kampagne auswählen.') };
  }
  if (!Array.isArray(leads) || leads.length === 0) {
    return { valid: false, error: tr('campaignHasNoLeads', 'Diese Kampagne enthält keine Leads.') };
  }
  if (leads.some((lead) => !String(lead?.id || '').trim() || !String(lead?.name || '').trim())) {
    return { valid: false, error: tr('campaignHasInvalidLeads', 'Die Kampagne enthält unvollständige Lead-Datensätze.') };
  }
  return { valid: true, error: '' };
}

function campaignResearchPrompt(campaign, leads, runId, options = {}) {
  const leadList = leads.map((lead, index) => `${index + 1}. ${lead.name} [${lead.id}]`).join('\n');
  const includePrivate = enabledPrivateResearchSources();
  return [
    'Nutze den CTOX Skill outbound-lead-generation-research und den CTOX Web Stack.',
    '',
    `Kampagne: ${campaign}`,
    `Umfang: ${options.scope === 'selection' ? `Auswahl mit ${leads.length} Leads` : `gesamte Kampagne mit ${leads.length} Leads`}`,
    `Workflow-ID: ${runId}`,
    `Leads: ${leads.length}`,
    '',
    'Aufgabe:',
    'Du bist der alleinige dauerhafte Orchestrator dieser Kampagnenrecherche. Die Ausführung darf nicht von einem geöffneten Browser-Tab abhängen.',
    'Verbindlicher Rechercheablauf:',
    state.researchPolicy,
    '',
    '--- RECHERCHEABLAUF (verbindlich fuer jeden Lead) ---',
    researchPolicyInstructions(state.researchPolicyRecord) || state.researchPolicy || DEFAULT_RESEARCH_POLICY,
    '--- ENDE RECHERCHEABLAUF ---',
    'Arbeite in zwei dauerhaften Phasen. Phase 1: Lies die Datensätze über business_os.query_records aus outbound_lead_generation_leads und starte für JEDEN Lead exakt eine business_os.execute_action mit module_id outbound-lead-generation, action_id web_stack.person_research und der jeweiligen Lead-ID sowohl als record_id als auch als payload.operation_id. Sende zuerst alle Actions ab; warte zwischen den Starts nicht auf deren terminalen Abschluss. Jeder angenommene Child-Command ist selbst dauerhaft und darf parallel oder durch den nativen Worker seriell abgearbeitet werden.',
    `Der Payload ist strikt: {"operation_id":"<Lead-ID>","company":"<Lead-Name>","country":"DE|AT|CH","mode":"new_record|update_firm","fields":${JSON.stringify(activeResearchFields())},"include_private":${JSON.stringify(includePrivate)},"person_priorities":${JSON.stringify(PERSON_RESEARCH_PRIORITIES)},"auto_browser_capture":true}. Verwende company, niemals company_name. Sende keine Felder research_scope, min_independent_sources oder campaign im Recherche-Payload.`,
    'Phase 2 beginnt erst, wenn für alle Lead-IDs ein Child-Command angenommen oder bereits terminal zurückgegeben wurde. Prüfe dann jeden Child-Command über business_os.get_command_status bis completed, failed, blocked oder cancelled. Ein anfänglicher running-Status ist ausdrücklich kein Erfolg. Beende oder berichte den Kampagnenlauf niemals nach einer bloßen Teilmenge; nenne offen jede noch nicht angenommene Operation. Der native Recherchebefehl schreibt den typisierten Lead-Zustand selbst zurück; prüfe ihn danach über business_os.get_record. Verwende keinen direkten Upsert.',
    '',
    'Validierungsregeln:',
    '- Für jedes Feld genügt EINE passende belegte Quelle (URL + wörtliches Zitat, das den konkreten Wert nennt). Weitere unabhängige Quellen stärken den Wert, sind aber keine Pflicht. Sellify allein belegt nichts; eine Spanne belegt keinen Einzelwert.',
    `- Selbstauskünfte (${[...SELF_REPORTED_FIELDS].join(', ')}) belegst du von der Unternehmensseite bzw. dem Profil selbst und trägst den Wert ein, statt ihn zu verwerfen. Eine persönliche E-Mail, die genau so auf der offiziellen Unternehmensseite steht, ist belegt, auch ohne SMTP-Prüfung.`,
    '- Widersprechen sich Quellen, das Feld NICHT still auflösen: action_required mit beiden Werten und ihren Quellen.',
    '- Einen gefundenen Wert nie stillschweigend fallen lassen: jeder Beleg wird erfasst, auch wenn er allein steht.',
    '- Listen als reine JSON-Listen senden: "sources": [ {…} ]. Niemals ein Trägerobjekt wie {"item": [ … ]} — solche Belege gehen verloren.',
    '- Nutze konfigurierte Playwright-Adapter; bei Zugriffshürden den Web-Stack-Unlocking- und Browser-Anmeldeprozess.',
    '- Terminal, Shell, curl, direkte HTTP-Aufrufe oder eigene Browserautomation sind kein Ersatz für business_os.execute_action.',
    '- Keine stillen Fehler: Fehler je Lead protokollieren und mit dem nächsten Lead fortfahren.',
    '- Setze je Lead queued -> running -> completed, needs_review oder failed und erhalte campaign_run_id sowie alle bestehenden Felder.',
    '- Keine direkte Übergabe an Sellify und keine SQL-Schreiboperation in diesem Lauf.',
    '- Ergebnisse ausschließlich in den zugehörigen outbound_lead_generation_leads zurückschreiben.',
    '',
    'Lead-Liste:',
    leadList,
  ].join('\n');
}

async function openCampaignResearchChat({
  campaign,
  leads,
  runId,
  prompt,
  title: requestedTitle = '',
  scope = 'campaign',
  submitTask = state.ctx?.businessChat?.submitTask,
}) {
  const validation = validateCampaignResearchRequest({ campaign, leads });
  if (!validation.valid) throw new Error(validation.error);
  if (!String(prompt || '').trim() || !String(runId || '').trim()) {
    throw new Error(tr('campaignTaskInvalid', 'Der Recherche-Task ist unvollständig und wurde nicht gestartet.'));
  }
  if (typeof submitTask !== 'function') {
    throw new Error(tr('chatUnavailable', 'CTOX Chat ist nicht verfügbar. Die Recherche wurde nicht gestartet.'));
  }
  const title = String(requestedTitle || `${tr('campaignResearchTitle', 'Kampagnenrecherche')}: ${campaign}`).trim();
  const detail = {
    text: prompt,
    module: 'outbound-lead-generation',
    source_module: 'outbound-lead-generation',
    source_title: tr('title', 'Outbound Lead Generation'),
    action: 'context-chat',
    reuseActive: false,
    open: true,
    command_id: runId,
    command_type: 'business_os.chat.task',
    record_id: `campaign:${campaign}`,
    title,
    command_title: title,
    instruction: prompt,
    mode: 'data',
    target: 'data',
    required_skills: ['outbound-lead-generation-research', 'universal-scraping', 'web-unlock'],
    writeback_contract: {
      collection: 'outbound_lead_generation_leads',
      allowed_collections: ['outbound_lead_generation_leads'],
      command_type: 'web_stack.person_research',
      record_ids: leads.map((lead) => lead.id),
      allowed_actions: [{
        module_id: 'outbound-lead-generation',
        action_id: 'web_stack.person_research',
        operation_ids: leads.map((lead) => lead.id),
      }],
      min_independent_sources: 1,
    },
    payload: {
      campaign,
      scope,
      lead_ids: leads.map((lead) => lead.id),
      lead_count: leads.length,
      prompt,
      response_channel: 'business_os_chat',
      thread_key: `business-os/outbound-lead-generation/campaign/${runId}`,
    },
    client_context: {
      action: 'context-chat',
      source: 'outbound-lead-generation-campaign-research',
      module: 'outbound-lead-generation',
      campaign,
      scope,
      workflow_id: runId,
      response_channel: 'business_os_chat',
    },
  };
  const submission = requireTrackedSubmission(await submitTask(detail));
  return { ...submission, detail };
}

function requireTrackedSubmission(submission, { allowTerminalCommand = false, allowControlCommand = false } = {}) {
  const taskId = String(submission?.task_id || submission?.taskId || '').trim();
  const commandId = String(submission?.command_id || submission?.commandId || '').trim();
  const status = String(submission?.terminal_status || submission?.status || '').trim().toLowerCase();
  const terminalCommand = allowTerminalCommand
    && ['completed', 'failed', 'blocked', 'cancelled', 'canceled'].includes(status);
  const trackedControlCommand = allowControlCommand && Boolean(commandId) && Boolean(status);
  if (!commandId || (!taskId && !terminalCommand && !trackedControlCommand)) {
    throw new Error(tr('taskNotConfirmed', 'CTOX hat keinen verfolgbaren Task bestätigt. Die Automatisierung wurde nicht gestartet.'));
  }
  return { ...submission, task_id: taskId, command_id: commandId };
}

function normalizeProtectionText(value) {
  return String(value || '')
    .normalize('NFKC')
    .toLocaleLowerCase('de-DE')
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim()
    .replace(/\s+/g, ' ');
}

function personDisplayName(person) {
  return String(person?.display_name || person?.name || [
    person?.first_name || person?.person_vorname,
    person?.last_name || person?.person_nachname,
  ].filter(Boolean).join(' ')).trim();
}

function personProtectionFields(person) {
  return [
    ['note_text', String(person?.note_text || '')],
    ['title', String(person?.title || '')],
  ].filter(([, value]) => value.trim());
}

function freePersonEligibility() {
  return {
    status: 'free',
    label: 'frei',
    reason: '',
    originalRemark: '',
    sourceField: '',
    sourceRecordId: '',
  };
}

function personEligibilityDecision(status, match, originalRemark, sourceField, sourceRecordId = '') {
  return {
    status,
    label: status === 'blocked' ? match.label : 'zu prüfen',
    reason: match.label,
    originalRemark: String(originalRemark || ''),
    sourceField,
    sourceRecordId: String(sourceRecordId || ''),
  };
}

// Sellify fuehrt ausgeschiedene Personen mit PERSON.retired. Seit die
// Sellify-Personen einer Firma als Kontakte am Lead stehen (Kern 12509a3ad),
// waeren sie sonst als Empfaenger waehlbar.
function istAusgeschieden(person) {
  const value = person?.retired;
  if (value === true || value === 1 || value === '1' || String(value).toLowerCase() === 'true') return true;
  return /\(ausgeschieden\)/i.test(String(person?.person_funktion || person?.funktion || person?.position || person?.function || person?.role || ''));
}

// Sellify fuehrt zwei Sperrmerkmale als Ja/Nein-Felder: nomailing und
// blockEmarketing. Der Serienbrief-Export der Sellify-App filtert beide seit
// 15.08.2026; hier galten sie bis 23.09.2026 nicht — ein Kontakt stand in
// Outbound als "frei" und fiel spaeter still aus dem Serienbrief (gemessen:
// 51.415 von 62.837 Sellify-Personen mit nomailing, 2 mit blockEmarketing).
// Gleiche Wahrheitswerte wie sellify/core/campaign-export.mjs.
const SELLIFY_SPERRE_WAHR = new Set([1, '1', true, 'true', 'J', 'j', 'Y', 'y']);

// Owner-Entscheidung 27.09.2026: "nomailing ist ein interner flag von sellify
// und keine echte sperre". nomailing wird nur noch als Hinweis gezeigt;
// Seit 1.0.265 sperrt auch blockEmarketing/ausgeschieden nicht mehr (nur Freitext).
function sellifySperrmerkmal(record) {
  for (const quelle of [record, record?.payload?.sql]) {
    if (!quelle || typeof quelle !== 'object') continue;
    if (SELLIFY_SPERRE_WAHR.has(quelle.blockEmarketing)) return { label: 'E-Marketing gesperrt', field: 'blockEmarketing' };
  }
  return null;
}

function sellifyNomailingHinweis(record) {
  return [record, record?.payload?.sql].some((quelle) => quelle && typeof quelle === 'object' && SELLIFY_SPERRE_WAHR.has(quelle.nomailing));
}

// Owner-Vorgabe (24./25.09.2026, wie Sellify 0.4.93): Eine Kontaktsperre
// ergibt sich AUSSCHLIESSLICH aus dem Freitext (Bemerkung) der Firma oder
// Person; Datenbankmerkmale wie nomailing, blockEmarketing, xstop oder
// "ausgeschieden" sperren nicht. Sie bleiben als Hinweis am Kontakt sichtbar.
// Freitexte wertet der CTOX-Agent aus (sellifyFreitexte / freitextUrteil).
function sellifyMerkmalHinweise(person) {
  const hinweise = [];
  if (istAusgeschieden(person)) hinweise.push('ausgeschieden');
  const merkmal = sellifySperrmerkmal(person);
  if (merkmal) hinweise.push(merkmal.field);
  if (sellifyNomailingHinweis(person)) hinweise.push('nomailing');
  return hinweise;
}

function classifySellifyPerson(person) {
  const hinweise = sellifyMerkmalHinweise(person);
  if (hinweise.length) {
    return { ...freePersonEligibility(), reason: `Sellify-Hinweis: ${hinweise.join(', ')} (keine Sperre)`, hinweis: hinweise.join(',') };
  }
  return freePersonEligibility();
}

function strongerPersonEligibility(left, right) {
  const rank = { free: 0, review: 1, blocked: 2 };
  if (!left) return right || freePersonEligibility();
  if (!right) return left;
  if (rank[right.status] === rank[left.status] && right.hinweis && !left.hinweis) return right;
  return rank[right.status] > rank[left.status] ? right : left;
}

function normalizedPersonId(value) {
  const raw = String(value || '').trim();
  // Nur echte Sellify-Kennungen: rein numerisch oder "…sellify-person-<n>".
  // Vorher zaehlten beliebige Endziffern — die UUID eines von Hand angelegten
  // Kontakts ("contact_manual_…e03e48862") wurde zur Sellify-Person 48862 und
  // erbte deren Sperre "Ausgeschieden" (Nachtest F, NF-1).
  const match = raw.match(/^(\d+)$/) || raw.match(/sellify-person-(\d+)$/);
  return match ? Number(match[1]) : 0;
}

function personMatchesContact(person, contact) {
  const explicitIds = [
    contact?.sellify_person_id,
    contact?.person_id,
    contact?.id,
  ].map(normalizedPersonId).filter(Boolean);
  if (explicitIds.length && explicitIds.includes(Number(person?.person_id))) return true;
  const contactEmail = String(contact?.email || contact?.person_email || '').trim().toLowerCase();
  const personEmail = String(person?.email || '').trim().toLowerCase();
  if (contactEmail && personEmail && contactEmail === personEmail) return true;
  const contactName = normalizeProtectionText(personDisplayName(contact));
  const sellifyName = normalizeProtectionText(personDisplayName(person));
  return contactName.split(' ').length >= 2 && contactName === sellifyName;
}

// Sellify fuehrt Sperren an der Firma auch als Ja/Nein-Merkmal; das gilt
// fuer alle Kontakte der Firma.
function firmenSperrmerkmalEntscheidung(company) {
  // Firmenmerkmale sperren ebenfalls nicht (Owner-Vorgabe, siehe oben).
  const hinweise = [];
  const merkmal = sellifySperrmerkmal(company);
  if (merkmal) hinweise.push(`Firma: ${merkmal.field}`);
  if (sellifyNomailingHinweis(company)) hinweise.push('Firma: nomailing');
  if (!hinweise.length) return null;
  return { ...freePersonEligibility(), reason: `Sellify-Hinweis: ${hinweise.join(', ')} (keine Sperre)`, hinweis: hinweise.join(',') };
}

// Sellify-Freitexte (note_text und title an Firma und Person) wertet seit
// 27.09.2026 der CTOX-Agent semantisch aus. Owner: "der ctox agent soll die
// freitext felder in sellify einfach semantisch auswerten." Die frueheren
// Phrasenlisten kannten "nicht anschreiben" oder "nicht mehr kontaktieren"
// nicht und schlugen bei "Rente" auch in "Rentenversicherung" an. Die
// strukturierten Merkmale (retired, nomailing, blockEmarketing) sind seit 1.0.265 nur Hinweise
// hart und ohne Agent. Solange kein Urteil zur aktuellen Textlage vorliegt,
// ist ein Kontakt "wird geprueft" - nie frei.
const FREITEXT_URTEIL_VERSION = 1;
const FREITEXT_BATCH_GROESSE = 12;
// Anteil der Payload-Vorschau (18.000 Zeichen, eingerueckt) fuer die Leads;
// der Rest bleibt fuer user_message, lead_ids und die Einrueckung im Payload.
const FREITEXT_PAKET_ZEICHEN = 11000;
// "high" schiebt einen Queue-Auftrag nur 1 h vor (queue_sort_at); hinter einem
// Recherche-Sammelstart von 130 Leads warteten die minutenkurzen Pruefungen
// so stundenlang (29.09.2026). "urgent" = 24 h.
const FREITEXT_PRIORITAET = 'urgent';
const FREITEXT_TEXT_MAX = 4000;
const FREITEXT_ANTWORT_FRIST_MS = 3 * 60 * 60 * 1000;
const FREITEXT_FEHLER_PAUSE_MS = 30 * 60 * 1000;
const FREITEXT_STATUS = Object.freeze({ gesperrt: 'blocked', pruefen: 'review', frei: 'free' });

function sellifyFreitexte(lead, { people = [], companies = [] } = {}) {
  const firma = [];
  for (const company of companies) {
    for (const [feld, text] of personProtectionFields(company)) {
      firma.push({ firma_id: String(company?.contact_id || company?.id || ''), feld, text: text.slice(0, FREITEXT_TEXT_MAX) });
    }
  }
  const personen = new Map();
  const aufnehmen = (quelle) => {
    const personId = normalizedPersonId(quelle?.person_id || quelle?.sellify_person_id || quelle?.id);
    if (!personId) return;
    const texte = personProtectionFields(quelle).map(([feld, text]) => ({ feld, text: text.slice(0, FREITEXT_TEXT_MAX) }));
    const bisher = personen.get(personId);
    if (bisher) {
      for (const eintrag of texte) {
        if (!bisher.vermerke.some((alt) => alt.feld === eintrag.feld && alt.text === eintrag.text)) bisher.vermerke.push(eintrag);
      }
      return;
    }
    personen.set(personId, {
      person_id: personId,
      name: personDisplayName(quelle),
      funktion: String(quelle?.function || quelle?.position || quelle?.person_funktion || quelle?.role || '').trim(),
      vermerke: texte,
    });
  };
  for (const person of people) aufnehmen(person);
  for (const contact of lead?.contacts || []) {
    if (normalizedPersonId(contact?.sellify_person_id || contact?.person_id)) aufnehmen({ ...contact, person_id: contact?.sellify_person_id || contact?.person_id });
  }
  const liste = [...personen.values()].sort((left, right) => left.person_id - right.person_id);
  firma.sort((left, right) => `${left.firma_id}|${left.feld}|${left.text}`.localeCompare(`${right.firma_id}|${right.feld}|${right.text}`));
  return { firma, personen: liste, hatText: firma.length > 0 || liste.some((person) => person.vermerke.length > 0) };
}

function freitextSignatur(texte) {
  return recipientSignatureHash(JSON.stringify({ v: FREITEXT_URTEIL_VERSION, firma: texte.firma, personen: texte.personen }));
}

function freitextUrteil(lead, signatur) {
  const urteil = lead?.payload?.sellify_freitext_urteil;
  return urteil && urteil.version === FREITEXT_URTEIL_VERSION && urteil.signatur === signatur ? urteil : null;
}

function freitextEntscheidung(eintrag, quelleFeld, quelleId) {
  const status = FREITEXT_STATUS[String(eintrag?.status || '').trim()];
  if (!status || status === 'free') return null;
  const grund = String(eintrag?.grund || '').trim() || (status === 'blocked' ? 'nicht anschreiben' : 'klären');
  return personEligibilityDecision(status, { label: `Sellify-Vermerk: ${grund}` }, String(eintrag?.zitat || ''), quelleFeld, quelleId);
}

function deriveLeadRecipientEligibility(lead, { people = [], companies = [], contextAvailable = true } = {}) {
  const normalizedLead = normalizeLeadRecipientShape(lead || {});
  const decisions = new Map();
  const texte = sellifyFreitexte(normalizedLead, { people, companies });
  const signatur = texte.hatText ? freitextSignatur(texte) : '';
  const urteil = texte.hatText ? freitextUrteil(normalizedLead, signatur) : null;
  if (contextAvailable && normalizedLead.id) {
    if (!(state.freitextLage instanceof Map)) state.freitextLage = new Map();
    state.freitextLage.set(normalizedLead.id, !texte.hatText ? 'ohne' : urteil ? 'geprueft' : 'offen');
  }
  if (texte.hatText && !urteil && contextAvailable) merkeFreitextBedarf(normalizedLead, texte, signatur);
  const firmaId = String(companies[0]?.contact_id || companies[0]?.id || '');
  for (const contact of normalizedLead.contacts) {
    let decision = classifySellifyPerson(contact);
    const matchedPeople = people.filter((person) => personMatchesContact(person, contact));
    for (const person of matchedPeople) {
      decision = strongerPersonEligibility(decision, classifySellifyPerson(person));
    }
    for (const company of companies) {
      decision = strongerPersonEligibility(decision, firmenSperrmerkmalEntscheidung(company));
    }
    if (texte.hatText && !urteil) {
      decision = strongerPersonEligibility(decision, {
        // Nur „wird geprüft“, wenn wirklich ein Prüfauftrag läuft. Vorher stand
        // das bei allen Kontakten, auch wenn nie eine Prüfung gestartet wurde
        // (Rundgang 28.09.2026: 19 Leads, 0 Urteile, 0 Aufträge).
        ...personEligibilityDecision('review', {
          label: freitextPruefungLaeuft(normalizedLead, signatur)
            ? 'Sellify-Vermerk wird von CTOX geprüft'
            : 'Sellify-Vermerk noch nicht geprüft',
        }, '', '', ''),
        pending: true,
      });
    } else if (urteil) {
      const personIds = [
        ...matchedPeople.map((person) => normalizedPersonId(person?.person_id || person?.id)),
        normalizedPersonId(contact?.sellify_person_id || contact?.person_id),
      ].filter(Boolean);
      const eigenes = personIds.map((personId) => urteil.personen?.[personId]).find(Boolean)
        || urteil.namen?.[normalizeProtectionText(personDisplayName(contact))]
        || urteil.firma;
      decision = strongerPersonEligibility(decision, freitextEntscheidung(eigenes, 'note_text', personIds[0] || firmaId));
      if (urteil.firma?.status === 'gesperrt') {
        decision = strongerPersonEligibility(decision, freitextEntscheidung(urteil.firma, 'note_text', firmaId));
      }
    }
    if (!contextAvailable && decision.status === 'free') {
      decision = {
        ...personEligibilityDecision('review', { label: 'Sellify-Sperrvermerk nicht prüfbar' }, '', '', ''),
        pending: true,
      };
    }
    decisions.set(contact.id, decision);
  }
  return decisions;
}

function merkeFreitextBedarf(lead, texte, signatur) {
  if (!lead?.id || !signatur) return;
  if (!(state.freitextBedarf instanceof Map)) state.freitextBedarf = new Map();
  const weitere = (lead.contacts || [])
    .filter((contact) => !normalizedPersonId(contact?.sellify_person_id || contact?.person_id))
    .map((contact) => personDisplayName(contact))
    .filter(Boolean);
  state.freitextBedarf.set(lead.id, { leadId: lead.id, name: String(lead.name || ''), texte, signatur, weitere });
  if (!state.freitextPlan) state.freitextPlan = globalThis.setTimeout(() => { void sendeFreitextPruefung(); }, 3_000);
}

function freitextPruefungLaeuft(lead, signatur) {
  const pruefung = lead?.payload?.sellify_freitext_pruefung;
  if (!pruefung || pruefung.signatur !== signatur) return false;
  const alter = Date.now() - Number(pruefung.at_ms || 0);
  return pruefung.fehler ? alter < FREITEXT_FEHLER_PAUSE_MS : alter < FREITEXT_ANTWORT_FRIST_MS;
}

function freitextLeadDaten(eintrag) {
  return {
    lead_id: eintrag.leadId,
    firma: eintrag.name,
    firmenvermerke: eintrag.texte.firma.map(({ feld, text }) => ({ feld, text })),
    personen: eintrag.texte.personen,
    weitere_kontakte_ohne_sellify_id: eintrag.weitere,
  };
}

// CTOX kuerzt die Anweisung eines Queue-Auftrags auf 8.000 Zeichen und zeigt
// das Payload als JSON-Vorschau bis 18.000 Zeichen (store.rs command_prompt).
// Die Leads stecken deshalb im Payload, und ein Paket bleibt unter dem Budget:
// 12 Leads mit bis zu 34.000 Zeichen Anweisung kamen abgeschnitten an, der
// Agent suchte den Rest selbst oder urteilte unvollstaendig (29.09.2026).
function freitextPakete(offen) {
  const pakete = [];
  let teil = [];
  let groesse = 0;
  for (const eintrag of offen) {
    const eigene = JSON.stringify(freitextLeadDaten(eintrag), null, 2).length;
    if (teil.length && (teil.length >= FREITEXT_BATCH_GROESSE || groesse + eigene > FREITEXT_PAKET_ZEICHEN)) {
      pakete.push(teil);
      teil = [];
      groesse = 0;
    }
    teil.push(eintrag);
    groesse += eigene;
  }
  if (teil.length) pakete.push(teil);
  return pakete;
}

function freitextBestaetigungAusstehend(error) {
  return /R(ü|ue)ckmeldung steht (noch )?aus|nicht rechtzeitig|nicht best(ä|ae)tigt|timed? ?out|Zeit(ü|ue)berschreitung/i.test(String(error?.message || error || ''));
}

function freitextPruefungPrompt() {
  return [
    'Werte die Freitextvermerke aus dem CRM Sellify semantisch aus: Darf das Unternehmen bzw. die Person angeschrieben werden (Serienbrief, E-Mail, Anruf)?',
    'Urteile nach dem Sinn des Vermerks, nicht nach Stichworten. Vermerke sind oft Gesprächsnotizen mit Datum; der jüngste Stand zählt.',
    '- "gesperrt": Der Vermerk besagt, dass nicht (mehr) angeschrieben oder kontaktiert werden soll, oder dass die Person nicht mehr erreichbar bzw. nicht mehr im Unternehmen ist (ausgeschieden, Ruhestand, verstorben).',
    '- "pruefen": Der Vermerk wirft eine Frage auf, die ein Mensch vor der Ansprache klären sollte (z. B. angekündigter Wechsel oder Nachfolge, unklare Zuständigkeit, widersprüchliche Angaben).',
    '- "frei": Der Vermerk enthält nichts, was gegen eine Ansprache spricht.',
    '"firma" gilt für das ganze Unternehmen und für Kontakte ohne eigenes Urteil. Setze "firma" nur auf "gesperrt", wenn der Vermerk das ganze Unternehmen betrifft; betrifft er nur eine Person, bewerte diese Person.',
    '"personen": ein Urteil für JEDE aufgeführte person_id, auch ohne eigenen Vermerk (dann das, was der Firmenvermerk für diese Person bedeutet).',
    '"namen": optional, für Personen, die ein Vermerk namentlich nennt, die aber keine person_id haben (z. B. ein genannter Nachfolger), Schlüssel "Vorname Nachname".',
    'Zu jedem Urteil außer "frei" gehören "grund" (kurz, deutsch) und "zitat" (die wörtliche Stelle aus dem Vermerk).',
    // Der Harness verlangt fuer Queue-Auftraege einen Plan: "keine Werkzeuge"
    // liess ihn bei 0/1 Schritten stehen, die Finalisierung scheiterte und der
    // Auftrag startete endlos neu (6 Auftraege, bis 135 Versuche, 28.09.2026).
    // Als Auftragsbestandteil formuliert pruefte der Reviewer den Plan aber in
    // fremden Tabellen (planned_steps), fand nichts und verwarf 60 richtige
    // Urteile (30.09.2026). Deshalb als Ablaufhinweis, nicht als Ergebnis.
    'Ablaufhinweis, kein Teil des Ergebnisses: Führe den vom System verlangten Plan mit genau einem Schritt „Vermerke beurteilen“ und setze ihn auf erledigt, bevor du antwortest. Sonst benutze keine Werkzeuge, recherchiere nicht und ändere keine Datensätze.',
    'Das Ergebnis ist ausschließlich genau ein JSON-Block in diesem Format; geprüft wird nur, ob die Urteile zu den Vermerken passen:',
    '```json',
    '{"leads":{"<lead_id>":{"firma":{"status":"frei|pruefen|gesperrt","grund":"","zitat":""},"personen":{"<person_id>":{"status":"frei|pruefen|gesperrt","grund":"","zitat":""}},"namen":{}}}}',
    '```',
    '',
    'Die Leads mit ihren Vermerken stehen vollständig unten im Payload JSON unter "freitext_leads". Urteile genau über diese Leads.',
  ].join('\n');
}

async function sendeFreitextPruefung() {
  state.freitextPlan = null;
  const submitTask = state.ctx?.businessChat?.submitTask;
  if (typeof submitTask !== 'function' || state.freitextSendet) return 0;
  const bedarf = [...(state.freitextBedarf?.values?.() || [])];
  state.freitextBedarf?.clear?.();
  const offen = bedarf.filter((eintrag) => {
    const lead = state.leads.find((entry) => entry.id === eintrag.leadId);
    return lead && !freitextUrteil(lead, eintrag.signatur) && !freitextPruefungLaeuft(lead, eintrag.signatur);
  });
  if (!offen.length) return 0;
  state.freitextSendet = true;
  let gesendet = 0;
  let uebergabeFehler = false;
  const pakete = freitextPakete(offen);
  // Gescheiterte Uebergaben gehen zurueck in den Bedarf und werden spaeter
  // erneut geschickt. Vorher waren sie verloren: nach dem Kampagnenlauf
  // (139 Sellify-Abfragen) lief die Uebergabe in RATE_LIMITED, und 82 Leads
  // blieben ohne Pruefauftrag, bis jemand den Lauf von Hand wiederholte
  // (30.09.2026).
  const zurueckInDenBedarf = (eintraege) => {
    if (!(state.freitextBedarf instanceof Map)) state.freitextBedarf = new Map();
    for (const eintrag of eintraege) if (!state.freitextBedarf.has(eintrag.leadId)) state.freitextBedarf.set(eintrag.leadId, eintrag);
  };
  try {
    for (const [index, teil] of pakete.entries()) {
      const commandId = `leadgen-sperrvermerk-${crypto.randomUUID()}`;
      const prompt = freitextPruefungPrompt();
      const title = teil.length === 1 ? `Sellify-Vermerk prüfen: ${teil[0].name}` : `Sellify-Vermerke prüfen: ${teil.length} Leads`;
      try {
        requireTrackedSubmission(await mitKanalHeilung(() => submitTask({
          text: prompt,
          module: 'outbound-lead-generation',
          source_module: 'outbound-lead-generation',
          source_title: tr('title', 'Outbound Lead Generation'),
          action: 'context-chat',
          reuseActive: false,
          open: false,
          command_id: commandId,
          command_type: 'business_os.chat.task',
          // Nie die Lead-ID: der Server projiziert einen Befehl mit
          // record_id=Lead als Recherche-Auftrag auf den Lead (command_id,
          // research_status "queued"), und "Alle recherchieren" uebersprang
          // den Lead danach als laufend (30.09.2026: 3 Kundenleads).
          record_id: `sperrvermerk:${commandId}`,
          title,
          command_title: title,
          instruction: prompt,
          prompt,
          mode: 'chat',
          target: 'chat',
          priority: FREITEXT_PRIORITAET,
          required_skills: [],
          payload: {
            prompt,
            lead_ids: teil.map((eintrag) => eintrag.leadId),
            freitext_leads: teil.map(freitextLeadDaten),
            priority: FREITEXT_PRIORITAET,
            response_channel: 'business_os_chat',
            thread_key: `business-os/outbound-lead-generation/sperrvermerk/${commandId}`,
          },
          client_context: {
            action: 'context-chat',
            source: 'outbound-lead-generation-sperrvermerk',
            module: 'outbound-lead-generation',
            response_channel: 'business_os_chat',
          },
        }), 'sellify-vermerkpruefung'));
      } catch (error) {
        // Der Befehl liegt schon dauerhaft in der lokalen Datenbank und wird
        // nachgereicht; nur die Bestaetigung kam zu spaet. Ohne Vermerk am
        // Lead schickte der naechste Lauf denselben Lead erneut (29.09.2026:
        // doppelte Pruefauftraege bei "Die Rückmeldung steht noch aus").
        if (!freitextBestaetigungAusstehend(error)) {
          console.warn('[outbound-lead-generation] Sellify-Vermerkprüfung konnte nicht übergeben werden', error);
          state.freitextLetzterFehler = String(error?.message || error);
          uebergabeFehler = true;
          if (/RATE_LIMITED|rate limit/i.test(state.freitextLetzterFehler)) {
            // Weitere Pakete scheitern am selben Limit: alles Restliche zurueck.
            zurueckInDenBedarf(pakete.slice(index).flat());
            break;
          }
          zurueckInDenBedarf(teil);
          continue;
        }
      }
      gesendet += teil.length;
      for (const eintrag of teil) {
        const pruefpersonen = eintrag.texte.personen.map((person) => person.person_id);
        await patchLead(eintrag.leadId, {
          payload: { sellify_freitext_pruefung: { command_id: commandId, signatur: eintrag.signatur, at_ms: Date.now(), pruefpersonen } },
        }, { payloadMerge: true }).catch((error) => console.warn('[outbound-lead-generation] Prüfauftrag nicht am Lead vermerkt', error));
      }
    }
  } finally {
    state.freitextSendet = false;
    // Bedarf, der waehrend dieses Versands hinzukam, blieb sonst liegen: der
    // Aufruf dafuer brach ab, weil ein Versand lief, und plante nichts nach
    // (30.09.2026: 13 CH-Leads trotz Kampagnenlauf nie uebergeben).
    if (uebergabeFehler) state.freitextUebergabeFehler = (state.freitextUebergabeFehler || 0) + 1;
    else if (gesendet) state.freitextUebergabeFehler = 0;
    if (state.freitextBedarf?.size && !state.freitextPlan) {
      const warten = uebergabeFehler
        ? Math.min(5 * 60_000, 20_000 * 2 ** Math.min((state.freitextUebergabeFehler || 1) - 1, 4))
        : 3_000;
      state.freitextPlan = globalThis.setTimeout(() => { void sendeFreitextPruefung(); }, warten);
    }
  }
  return gesendet;
}

function freitextAntwortText(befehl) {
  return [befehl?.answer, befehl?.response, befehl?.result?.outbound_text, befehl?.outbound_text]
    .find((text) => typeof text === 'string' && text.includes('{')) || '';
}

function leseFreitextAntwort(text) {
  const quelle = String(text || '');
  const kandidaten = [...quelle.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)].map((treffer) => treffer[1]).reverse();
  const anfang = quelle.indexOf('{');
  const ende = quelle.lastIndexOf('}');
  if (anfang >= 0 && ende > anfang) kandidaten.push(quelle.slice(anfang, ende + 1));
  for (const kandidat of kandidaten) {
    // Das erste vollstaendige Objekt zaehlt: eine ueberzaehlige schliessende
    // Klammer am Ende verwarf sonst die ganze Antwort als "ohne vollstaendiges
    // Urteil" (leadgen-sperrvermerk-9365bf16…, 29.09.2026).
    for (const text of [kandidat, ersterJsonBlock(kandidat)]) {
      if (!text) continue;
      try {
        const wert = JSON.parse(text);
        if (wert?.leads && typeof wert.leads === 'object') return wert.leads;
      } catch { /* naechster Versuch */ }
    }
  }
  return null;
}

function ersterJsonBlock(text) {
  const quelle = String(text || '');
  const anfang = quelle.indexOf('{');
  if (anfang < 0) return '';
  let tiefe = 0;
  let inText = false;
  for (let index = anfang; index < quelle.length; index += 1) {
    const zeichen = quelle[index];
    if (inText) {
      if (zeichen === '\\') index += 1;
      else if (zeichen === '"') inText = false;
    } else if (zeichen === '"') {
      inText = true;
    } else if (zeichen === '{') {
      tiefe += 1;
    } else if (zeichen === '}') {
      tiefe -= 1;
      if (tiefe === 0) return quelle.slice(anfang, index + 1);
    }
  }
  return '';
}

function freitextEintrag(roh) {
  const status = String(roh?.status || '').trim().toLowerCase().replace('prüfen', 'pruefen');
  if (!FREITEXT_STATUS[status]) return null;
  return { status, grund: String(roh?.grund || '').trim().slice(0, 300), zitat: String(roh?.zitat || '').trim().slice(0, 600) };
}

// Ein Urteil gilt nur vollstaendig: Firma und jede geprüfte Person. Fehlt eine
// Person, faellt sie nicht still auf das Firmenurteil zurueck.
function baueFreitextUrteil(roh, pruefung, commandId) {
  const firma = freitextEintrag(roh?.firma);
  if (!firma) return null;
  const personen = {};
  for (const personId of pruefung?.pruefpersonen || []) {
    const eintrag = freitextEintrag(roh?.personen?.[personId] ?? roh?.personen?.[String(personId)]);
    if (!eintrag) return null;
    personen[personId] = eintrag;
  }
  const namen = {};
  for (const [name, wert] of Object.entries(roh?.namen || {})) {
    const eintrag = freitextEintrag(wert);
    const schluessel = normalizeProtectionText(name);
    if (eintrag && schluessel.split(' ').length >= 2) namen[schluessel] = eintrag;
  }
  return { version: FREITEXT_URTEIL_VERSION, signatur: pruefung.signatur, command_id: commandId, geprueft_at_ms: Date.now(), firma, personen, namen };
}

async function verarbeiteFreitextAntworten() {
  if (state.freitextVerarbeitung) return;
  const collection = state.ctx?.db?.collection?.('business_commands');
  if (!collection?.findOne) return;
  state.freitextVerarbeitung = true;
  const geaendert = [];
  try {
    const nachBefehl = new Map();
    for (const lead of state.leads) {
      const pruefung = lead?.payload?.sellify_freitext_pruefung;
      if (!pruefung?.command_id || pruefung.fehler) continue;
      if (freitextUrteil(lead, pruefung.signatur)) continue;
      if (!nachBefehl.has(pruefung.command_id)) nachBefehl.set(pruefung.command_id, []);
      nachBefehl.get(pruefung.command_id).push(lead);
    }
    // Eine Abfrage je 50 Befehle: jede Einzelabfrage dauerte am Kunden-Knoten
    // 9-16 s, ein Durchgang ueber 60 Pruefauftraege also ~10 min, und fertige
    // Urteile kamen stundenlang nicht am Lead an (29.09.2026).
    const befehle = new Map();
    const ids = [...nachBefehl.keys()];
    for (let start = 0; start < ids.length; start += 50) {
      const teil = ids.slice(start, start + 50);
      try {
        const docs = typeof collection.find === 'function'
          ? await collection.find({ selector: { id: { $in: teil } }, limit: teil.length }).exec()
          : await Promise.all(teil.map((id) => collection.findOne(id).exec()));
        for (const doc of docs || []) {
          const befehl = doc?.toJSON?.() || doc;
          if (befehl?.id) befehle.set(befehl.id, befehl);
        }
      } catch { /* naechster Takt versucht es erneut */ }
    }
    for (const [commandId, leads] of nachBefehl) {
      const befehl = befehle.get(commandId) || null;
      if (!befehl) continue;
      const status = String(befehl?.terminal_status || befehl?.status || '').toLowerCase();
      const fehlschlag = ['failed', 'blocked', 'cancelled', 'canceled'].includes(status);
      if (!fehlschlag && status !== 'completed') continue;
      const antwort = fehlschlag ? null : leseFreitextAntwort(freitextAntwortText(befehl));
      for (const lead of leads) {
        const pruefung = lead.payload.sellify_freitext_pruefung;
        const urteil = antwort ? baueFreitextUrteil(antwort[lead.id], pruefung, commandId) : null;
        const patch = urteil
          ? { sellify_freitext_urteil: urteil }
          : { sellify_freitext_pruefung: { ...pruefung, at_ms: Date.now(), fehler: fehlschlag ? (befehl?.error_message || status) : 'Antwort ohne vollständiges Urteil' } };
        await patchLead(lead.id, { payload: patch }, { payloadMerge: true }).catch((error) => console.warn('[outbound-lead-generation] Vermerk-Urteil nicht gespeichert', error));
        geaendert.push(lead.id);
      }
    }
  } finally {
    state.freitextVerarbeitung = false;
  }
  if (!geaendert.length) return;
  for (const leadId of geaendert) state.recipientEligibilityReady.delete(leadId);
  const offen = state.leads.find((lead) => lead.id === state.selectedLeadId && geaendert.includes(lead.id));
  if (offen) void refreshLeadRecipientEligibility(offen, { force: true }).catch(() => {});
  renderCenter();
}

// Kampagnenweit: Sellify-Kontext je Lead laden und fehlende Urteile beim
// CTOX-Agenten anfordern. Seriell, damit die Sellify-Abfragen die App nicht
// verstopfen (siehe refreshAllRecipientEligibility).
async function pruefeKampagnenVermerke(campaign) {
  if (state.kampagnenVermerkLauf) return;
  await ensureFullLeads(campaignListLeads(campaign).map((lead) => lead.id));
  const leads = campaignLeads(campaign);
  if (!leads.length || state.kampagnenVermerkLauf) return;
  state.kampagnenVermerkLauf = { campaign, done: 0, total: leads.length };
  if (!(state.freitextLage instanceof Map)) state.freitextLage = new Map();
  renderCenter();
  const zaehler = { ohne: 0, geprueft: 0, offen: 0, fehler: 0 };
  let gesendet = 0;
  state.freitextLetzterFehler = '';
  try {
    for (const lead of leads) {
      state.freitextLage.delete(lead.id);
      try {
        await refreshLeadRecipientEligibility(lead, { force: true });
      } catch (error) {
        console.warn('[outbound-lead-generation] Vermerkprüfung: Sellify-Kontext nicht geladen', lead.id, error);
      }
      zaehler[state.freitextLage.get(lead.id) || 'fehler'] += 1;
      state.kampagnenVermerkLauf.done += 1;
      if (state.kampagnenVermerkLauf.done % 5 === 0) renderCenter();
    }
    if (state.freitextPlan) { globalThis.clearTimeout(state.freitextPlan); state.freitextPlan = null; }
    gesendet = await sendeFreitextPruefung();
  } finally {
    state.kampagnenVermerkLauf = null;
    renderCenter();
  }
  console.info('[outbound-lead-generation] Vermerkprüfung', campaign, JSON.stringify({ ...zaehler, gesendet }));
  await showBusinessAlert([
    `Sellify-Vermerke in „${campaign}“: ${leads.length} Leads.`,
    `${zaehler.ohne} ohne Freitextvermerk, ${zaehler.geprueft} bereits vom CTOX-Agenten beurteilt, ${zaehler.offen} mit Vermerk ohne Urteil.`,
    gesendet ? `${gesendet} davon an den CTOX-Agenten übergeben; die Urteile erscheinen, sobald er antwortet.`
      : (zaehler.offen && state.freitextLetzterFehler ? `Übergabe an den CTOX-Agenten fehlgeschlagen: ${state.freitextLetzterFehler}`
        : (zaehler.offen ? 'Die offenen Vermerke sind bereits beim CTOX-Agenten in Prüfung.' : '')),
    zaehler.fehler ? `${zaehler.fehler} Leads konnten nicht geprüft werden (Sellify-Kontext nicht geladen); bitte erneut starten.` : '',
  ].filter(Boolean).join('\n'));
}

function recipientEligibilityKey(leadId, contactId) {
  return `${String(leadId || '')}|${String(contactId || '')}`;
}

function currentContactEligibility(lead, contact) {
  const local = classifySellifyPerson(contact);
  if (local.status !== 'free') return local;
  const cached = state.recipientEligibility.get(recipientEligibilityKey(lead?.id, contact?.id));
  if (cached) return cached;
  return {
    ...personEligibilityDecision('review', { label: 'Sellify-Sperrvermerk wird geprüft' }, '', '', ''),
    pending: true,
  };
}

function buildCampaignRecipientList(lead, decisions = null) {
  const normalized = normalizeLeadRecipientShape(lead || {});
  const selectedIds = new Set(normalized.selected_contact_ids);
  const recipients = [];
  const excluded = [];
  for (const contact of normalized.contacts) {
    if (!selectedIds.has(contact.id)) continue;
    let decision = decisions?.get?.(contact.id)
      || decisions?.[contact.id]
      || currentContactEligibility(normalized, contact);
    const identitaet = kontaktIdentitaet(contact);
    if (identitaet.status === 'widerspruch' && decision?.status === 'free') {
      decision = personEligibilityDecision('review', { label: `Name widerspricht Quelle („${identitaet.quelleName}“)` }, '', '', '');
    }
    if (decision?.status === 'free') recipients.push(contact);
    else excluded.push({ contact, decision: decision || personEligibilityDecision('review', { label: 'Sperrstatus unbekannt' }, '', '', '') });
  }
  return { recipients, excluded, selectedCount: selectedIds.size };
}

function recipientEligibilitySignature(lead) {
  return JSON.stringify({
    // Regelstand: 2 = Sellify-Merkmale nomailing/blockEmarketing und
    // Firmen-Kontaktsperren zaehlen (23.09.2026). Aeltere Urteile verfallen.
    regeln: 5,
    freitext_urteil: lead?.payload?.sellify_freitext_urteil?.signatur || '',
    freitext_urteil_befehl: lead?.payload?.sellify_freitext_urteil?.command_id || '',
    name: lead?.name || '',
    sellify_contact_id: lead?.payload?.sellify_contact_id || '',
    contacts: (lead?.contacts || []).map((contact) => ({
      id: contact?.id || '',
      person_id: contact?.person_id || contact?.sellify_person_id || '',
      name: personDisplayName(contact),
      email: contact?.email || contact?.person_email || '',
      note_text: contact?.note_text || '',
      title: contact?.title || '',
    })),
  });
}

// Das Urteil einer Sperrvermerk-Pruefung lebte nur im Speicher der Sitzung.
// Nach jedem Neuladen galten alle ausgewaehlten Kontakte wieder als
// ungeprueft, und die Leiste meldete "hat fuer 68 Kontakte nicht geantwortet"
// (gemessen 10.09.2026), obwohl Sellify jede Abfrage in 1-2 s beantwortet
// hatte. Ein Urteil gilt jetzt zwoelf Stunden in diesem Browser, solange sich
// Firma und Kontakte nicht aendern (gleiche Signatur). Die Uebergabe an Sellify
// prueft ohnehin immer frisch (force).
const RECIPIENT_ELIGIBILITY_CACHE_MS = 12 * 60 * 60 * 1000;
function recipientEligibilityStorageKey(leadId) {
  return `ctox.olg.sperrvermerk.v2.${leadId}`;
}
// Das Urteil liegt zusaetzlich am Lead (recipient_eligibility) und gilt damit
// fuer jeden Browser und jeden Nutzer. Bis 10.09.2026 lag es nur im Speicher
// des einen Browsers: jede neue Sitzung pruefte 74 Kontakte von vorn, das
// rote Banner stand minutenlang, und alle Empfaenger waren gelb.
function recipientSignatureHash(signature) {
  let hash = 0x811c9dc5;
  for (let index = 0; index < signature.length; index += 1) {
    hash ^= signature.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}
// Erledigt ist die Vermerkpruefung eines Leads, wenn der CTOX-Agent ein
// Urteil geliefert hat ODER die Sperrpruefung ohne offenen Vermerk
// abgeschlossen und am Lead gespeichert wurde (nur endgueltige Urteile werden
// gespeichert, s. persistRecipientEligibility). Vorher zaehlte nur das
// Agenten-Urteil: Leads ganz ohne Sellify-Freitext blieben ewig „offen“, und
// die Excel-Kampagne zeigte 4/139 nach vollstaendiger Pruefung (30.09.2026).
function vermerkPruefungErledigt(lead) {
  if (lead?.payload?.sellify_freitext_urteil) return true;
  const gespeichert = lead?.recipient_eligibility;
  return Boolean(gespeichert?.signature)
    && gespeichert.signature === recipientSignatureHash(recipientEligibilitySignature(lead))
    && Array.isArray(gespeichert.decisions)
    && !gespeichert.decisions.some((paar) => paar?.[1]?.pending === true);
}

function persistRecipientEligibility(lead, decisions) {
  // Vorlaeufiges wird nicht gespeichert, auch nicht am Lead (s. restore).
  if ([...decisions.values()].some((decision) => decision?.pending === true)) return;
  const record = {
    signature: recipientSignatureHash(recipientEligibilitySignature(lead)),
    at: Date.now(),
    decisions: [...decisions.entries()],
  };
  try {
    globalThis.localStorage?.setItem(recipientEligibilityStorageKey(lead.id), JSON.stringify(record));
  } catch { /* Ein voller oder gesperrter Speicher darf die Pruefung nicht kippen. */ }
  const shared = lead?.recipient_eligibility;
  // Nicht bei jeder Pruefung schreiben: nur wenn sich die Kontakte geaendert
  // haben oder das gemeinsame Urteil aelter als eine Stunde ist.
  if (shared?.signature === record.signature && Date.now() - Number(shared?.at || 0) < 60 * 60 * 1000) return;
  if (state.startLaeuft) return;
  lead.recipient_eligibility = record;
  patchLead(lead.id, { recipient_eligibility: record }).catch((error) => {
    console.warn('[outbound-lead-generation] Sperrvermerk-Urteil konnte nicht am Lead gespeichert werden', error);
  });
}
function restoreRecipientEligibility(lead, signature, { maxAgeMs = RECIPIENT_ELIGIBILITY_CACHE_MS } = {}) {
  const hash = recipientSignatureHash(signature);
  const candidates = [];
  try {
    const raw = globalThis.localStorage?.getItem(recipientEligibilityStorageKey(lead.id));
    if (raw) candidates.push(JSON.parse(raw));
  } catch { /* Browserspeicher nicht lesbar: dann gilt das Urteil am Lead. */ }
  if (lead?.recipient_eligibility) candidates.push(lead.recipient_eligibility);
  // Vorlaeufige Urteile („wird geprueft“, „nicht pruefbar“) sind kein Stand,
  // den man wiederherstellt: sie hielten nach jedem Neuladen ein Etikett fest,
  // obwohl gar keine Pruefung lief (Rundgang 28.09.2026, AKEMI).
  const stored = candidates
    .filter((entry) => entry?.signature === hash
      && Date.now() - Number(entry?.at || 0) < maxAgeMs
      && Array.isArray(entry?.decisions)
      && !entry.decisions.some((paar) => paar?.[1]?.pending === true))
    .sort((left, right) => Number(right.at || 0) - Number(left.at || 0))[0];
  if (!stored) return false;
  for (const [contactId, decision] of stored.decisions) {
    state.recipientEligibility.set(recipientEligibilityKey(lead.id, contactId), decision);
  }
  state.recipientEligibilityReady.add(lead.id);
  return true;
}
function forgetRecipientEligibility(leadIds) {
  try {
    for (const leadId of leadIds) globalThis.localStorage?.removeItem(recipientEligibilityStorageKey(leadId));
  } catch { /* nichts zu tun */ }
}
function invalidateChangedRecipientEligibility(leads) {
  const liveLeadIds = new Set((leads || []).map((lead) => lead.id));
  for (const lead of leads || []) {
    const signature = recipientEligibilitySignature(lead);
    if (state.recipientEligibilitySignatures.get(lead.id) === signature) continue;
    state.recipientEligibilitySignatures.set(lead.id, signature);
    state.recipientEligibilityReady.delete(lead.id);
    for (const key of state.recipientEligibility.keys()) {
      if (key.startsWith(`${lead.id}|`)) state.recipientEligibility.delete(key);
    }
    restoreRecipientEligibility(lead, signature);
  }
  for (const leadId of [...state.recipientEligibilitySignatures.keys()]) {
    if (liveLeadIds.has(leadId)) continue;
    state.recipientEligibilitySignatures.delete(leadId);
    state.recipientEligibilityReady.delete(leadId);
    state.recipientRemovalNotices.delete(leadId);
  }
}

function docJson(doc) {
  return doc?.toJSON?.() || doc;
}

function uniqueSellifyRecords(records) {
  const unique = new Map();
  for (const record of records || []) {
    const value = docJson(record);
    const key = String(value?.id || value?.person_id || value?.contact_id || JSON.stringify(value));
    if (!unique.has(key)) unique.set(key, value);
  }
  return [...unique.values()];
}

async function findSellifyRecords(collection, selector) {
  if (!collection?.find) return [];
  const docs = await collection.find({ selector }).exec();
  return (docs || []).map(docJson).filter((record) => !record?.is_deleted);
}

async function loadSellifyRecipientContext(lead) {
  // Die Sperrvermerkspruefung darf nicht daran scheitern, dass zufaellig noch
  // niemand sellifyReadCollection() gerufen hat. state.sellifyPeople ist ein
  // Zwischenspeicher, der erst beim ersten Zugriff gefuellt wird — beim Rendern
  // der Empfaengerliste passierte das nie. Ergebnis am 11.08.2026: jeder Kontakt
  // stand auf "Sellify-Sperrvermerk nicht pruefbar", das Haekchen war
  // deaktiviert, und damit endete die Kette vor der Uebergabe. Das System hat
  // dabei richtig gehandelt — ohne Pruefung der Kontaktsperre darf niemand
  // angeschrieben werden; es konnte nur nicht pruefen.
  if (!state.sellifyPeople?.find || !state.sellifyCompanies?.find) {
    try {
      sellifyReadCollection('company');
      sellifyReadCollection('person');
    } catch (error) {
      console.warn('[outbound-lead-generation] Sellify-Projektion fuer die Sperrvermerkspruefung nicht erreichbar', error);
    }
  }
  if (!state.sellifyPeople?.find || !state.sellifyCompanies?.find) {
    merkeSellifyProjektion(false, lead, null);
    return { people: [], companies: [], contextAvailable: false };
  }
  try {
    // Unabhaengige Abfragen laufen nebenlaeufig. Am 12.08.2026 brauchten die
    // seriellen await-Schleifen 50360 ms fuer 1 Firma und 3 Personen — jede
    // contact_id, person_id, email, display_name und jede Namensvariante war
    // ein eigener Round-Trip. Promise.all + $in halten denselben Schutzzweck
    // und senken die Dauer unter 3 s.
    const linkedContactId = Number(lead?.payload?.sellify_contact_id) || 0;
    const leadName = String(lead?.name || '').trim();
    const companyNameValues = leadName
      ? [...new Set([leadName, ...firmenNamensvarianten(leadName)])]
      : [];

    // Ein einziger typisierter Read-Command pro Entität. Frühere Fassungen
    // starteten für ID, Namen, E-Mail und Anzeigenamen jeweils einen eigenen
    // Command. Das konnte fünf parallele Demand-Queries pro Lead erzeugen und
    // das Browser-Budget sprengen, obwohl der Native-Handler längst fertig war.
    const activeCompaniesPromise = linkedContactId || companyNameValues.length
      ? state.sellifyCompanies.lookup({
        ids: linkedContactId ? [`sellify-company-${linkedContactId}`] : [],
        selector: companyNameValues.length ? { name: { $in: companyNameValues } } : {},
        limit: 50,
      })
      : Promise.resolve([]);

    const personIds = new Set();
    const emails = new Set();
    const displayNames = new Set();
    for (const contact of lead?.contacts || []) {
      const personId = normalizedPersonId(contact?.sellify_person_id || contact?.person_id || contact?.id);
      if (personId) personIds.add(personId);
      const email = String(contact?.email || contact?.person_email || '').trim();
      if (email) emails.add(email);
      const displayName = personDisplayName(contact);
      if (normalizeProtectionText(displayName).split(' ').length >= 2) {
        displayNames.add(displayName);
      }
    }

    const personSelector = {};
    if (emails.size) personSelector.email = { $in: [...emails] };
    if (displayNames.size) personSelector.display_name = { $in: [...displayNames] };
    // Ohne Kennung, E-Mail und Namen lief die Personensuche ganz ohne Filter
    // ("selectors":[],"ids":[]) und lieferte beliebige 100 Personen; die
    // gezielte Suche ueber die Firma entfiel dann (Klicktest P2 BG-14 / V8).
    const personenFilter = personIds.size || Object.keys(personSelector).length;
    const peoplePromise = personenFilter
      ? state.sellifyPeople.lookup({
        ids: [...personIds].map((personId) => `sellify-person-${personId}`),
        selector: personSelector,
        limit: 100,
      })
      : Promise.resolve([]);
    const [activeCompaniesRaw, initialPeople] = await Promise.all([
      activeCompaniesPromise,
      peoplePromise,
    ]);
    const activeCompanies = uniqueSellifyRecords(activeCompaniesRaw);
    const contactIds = [...new Set(
      activeCompanies
        .map((company) => Number(company?.contact_id) || 0)
        .filter(Boolean),
    )];
    const people = initialPeople.length || !contactIds.length
      ? initialPeople
      : await state.sellifyPeople.lookup({
        selector: { contact_id: { $in: contactIds } },
        limit: 100,
      });
    merkeSellifyProjektion(true, lead, activeCompanies[0] || null);
    return { people: uniqueSellifyRecords(people), companies: activeCompanies, contextAvailable: true };
  } catch (error) {
    // Dieser Zweig war ein stummes catch. Die Folge: die Sperrpruefung meldete
    // "nicht pruefbar", der Kontrollkasten blieb gesperrt, und WARUM war
    // nirgends zu sehen — am 12.08.2026 stand Roger Wintzen auf "nicht
    // pruefbar", waehrend dieselbe Abfrage aus der Konsole ihn sofort fand
    // (note_text leer, also frei). Ein Fehler ohne Spur ist nicht
    // diagnostizierbar; er sieht aus wie eine Eigenschaft der Daten.
    console.error('[outbound-lead-generation] Sperrvermerkspruefung fehlgeschlagen', error);
    state.lastEligibilityError = String(error?.message || error);
    merkeSellifyProjektion(false, lead, null);
    return { people: [], companies: [], contextAvailable: false };
  }
}

// Was im CRM gepflegt ist, wird uebernommen — nicht neu erraten.
//
// Am 11.08.2026 stand im gesamten Lead-Bestand KEINE einzige E-Mail-Adresse an
// einem Ansprechpartner. Damit war die Serien-E-Mail fachlich unmoeglich: der
// Knopf war aktiv, filterte aber auf gueltige Adressen und behielt null
// Empfaenger. Gleichzeitig lagen im CRM 60.021 von 60.640 Personen MIT Adresse,
// fuer denselben Roger Wintzen von CHEMOFAST woertlich
// roger.wintzen@chemofast.com. Die Recherche suchte im offenen Netz nach etwas,
// das zwei Handgriffe entfernt gepflegt bereitlag.
//
// Uebernommen wird nur, was FEHLT. Ein am Lead vorhandener Wert bleibt stehen —
// er kann aus der Recherche stammen und aktueller sein als das CRM. Und es wird
// nichts erfunden: jeder Wert kommt aus einem echten CRM-Datensatz.
function crmSchluessel(text) {
  return String(text || '').trim().toLowerCase().replace(/\s+/g, ' ');
}

function sellifyCrmBeleg(fieldKey, value, { contactId = '', personId = '' } = {}) {
  const kennung = [
    contactId ? `contact_id ${contactId}` : '',
    personId ? `person_id ${personId}` : '',
  ].filter(Boolean).join(' · ');
  return {
    field_key: fieldKey,
    field: fieldKey,
    value,
    confidence: 'crm',
    source_id: SELLIFY_SOURCE_ID,
    source_url: '',
    tier: 'I',
    via: 'sellify-crm',
    label: SELLIFY_SOURCE_LABEL,
    note: kennung ? `${kennung} · eigene gepflegte Angabe` : 'eigene gepflegte Angabe',
    contact_id: contactId || '',
    person_id: personId || '',
  };
}

async function uebernehmeCrmKontaktdaten(lead, context) {
  if (!context?.contextAvailable || !Array.isArray(context.people) || !context.people.length) return false;
  // Bei mehreren CRM-Datensaetzen zum selben Namen gewinnt der VOLLSTAENDIGERE.
  // CHEMOFAST wird im CRM unter zwei contact_ids gefuehrt (17714 und 18255);
  // Roger Wintzen steht in beiden, seine Adresse roger.wintzen@chemofast.com aber
  // nur in 17714. Wer einfach den ersten Treffer nimmt, erwischt in der Haelfte
  // der Faelle den leeren Datensatz und uebernimmt nichts — genau das war am
  // 12.08.2026 der Fall.
  const inhalt = (person) => [person?.email, person?.phone || person?.telephone,
    person?.position || person?.function].filter((wert) => String(wert || '').trim()).length;
  const nachName = new Map();
  for (const person of context.people) {
    const schluessel = crmSchluessel(personDisplayName(person));
    if (!schluessel) continue;
    const bisher = nachName.get(schluessel);
    if (!bisher || inhalt(person) > inhalt(bisher)) nachName.set(schluessel, person);
  }
  const kontakte = Array.isArray(lead?.contacts) ? lead.contacts : [];
  const belege = [];
  let geaendert = false;
  const neueKontakte = kontakte.map((kontakt) => {
    const person = nachName.get(crmSchluessel(personDisplayName(kontakt)));
    if (!person) return kontakt;
    const ergaenzt = { ...kontakt };
    const contactId = person.contact_id || context.companies?.[0]?.contact_id || '';
    const personId = person.person_id || '';
    const leer = (...felder) => !felder.some((feld) => String(ergaenzt[feld] || '').trim());
    const uebernehmen = (feld, wert) => {
      const vorhanden = String(ergaenzt[feld] || '').trim();
      const neu = String(wert || '').trim();
      if (vorhanden || !neu) return '';
      ergaenzt[feld] = neu;
      geaendert = true;
      return neu;
    };
    const emailWarLeer = leer('email', 'person_email');
    const telefonWarLeer = leer('phone', 'person_telefon');
    const positionWarLeer = leer('position', 'person_position');
    const email = uebernehmen('email', person.email) || uebernehmen('person_email', person.email);
    uebernehmen('person_email', person.email);
    const telefon = uebernehmen('phone', person.phone || person.telephone)
      || uebernehmen('person_telefon', person.phone || person.telephone);
    uebernehmen('person_telefon', person.phone || person.telephone);
    const position = uebernehmen('position', person.position || person.function)
      || uebernehmen('person_position', person.position || person.function);
    uebernehmen('person_position', person.position || person.function);
    if (emailWarLeer && email) belege.push(sellifyCrmBeleg('person_email', email, { contactId, personId }));
    if (telefonWarLeer && telefon) belege.push(sellifyCrmBeleg('person_telefon', telefon, { contactId, personId }));
    if (positionWarLeer && position) belege.push(sellifyCrmBeleg('person_position', position, { contactId, personId }));
    if (!ergaenzt.sellify_person_id && person.person_id) {
      ergaenzt.sellify_person_id = person.person_id;
      geaendert = true;
    }
    return ergaenzt;
  });
  if (!geaendert) return false;
  // Waehrend des Starts nichts zurueckschreiben (alter Stand, siehe startLaeuft).
  if (state.startLaeuft) return false;
  lead.contacts = neueKontakte;
  const evidence = belege.length
    ? deduplicateEvidence([...(lead.evidence || []), ...belege])
    : lead.evidence;
  if (belege.length) lead.evidence = evidence;
  try {
    await patchLead(lead.id, belege.length
      ? { contacts: neueKontakte, evidence }
      : { contacts: neueKontakte });
  } catch (error) {
    console.warn('[outbound-lead-generation] CRM-Kontaktdaten konnten nicht gespeichert werden', error);
  }
  return true;
}

function planeSperrpruefungNeu(leadId) {
  if (!(state.sperrpruefungNeu instanceof Map)) state.sperrpruefungNeu = new Map();
  // Nur fuer den geoeffneten Lead: bei vielen Leads mit Auswahl haette ein
  // nicht erreichbares Sellify sonst im 20-s-Takt Dutzende Abfragen erzeugt.
  if (state.sperrpruefungNeu.has(leadId) || leadId !== state.selectedLeadId) return;
  state.sperrpruefungNeu.set(leadId, globalThis.setTimeout(() => {
    state.sperrpruefungNeu.delete(leadId);
    const aktuell = state.leads.find((entry) => entry.id === leadId);
    if (!aktuell || leadId !== state.selectedLeadId || !state.recipientEligibilityTimedOut.has(leadId)) return;
    void refreshLeadRecipientEligibility(aktuell, { force: true })
      .then(() => { if (state.selectedLeadId === leadId) renderDetail(); })
      .catch(() => {});
  }, wartungAktiv() ? 30_000 : 20_000));
}

function wartungAktiv() {
  return Date.now() - Number(state.wartungGesehenAt || 0) < 120_000;
}

async function refreshLeadRecipientEligibility(lead, { force = false } = {}) {
  if (!lead?.id) return new Map();
  if (!force && state.recipientEligibilityReady.has(lead.id)) {
    return new Map((lead.contacts || []).map((contact) => [
      contact.id,
      currentContactEligibility(lead, contact),
    ]));
  }
  // Die Sperrvermerkspruefung fragt die CRM-Projektion ab — in diesem Mandanten
  // 17.520 Firmen und 60.639 Personen, die per Bedarfsabfrage erst geladen
  // werden muessen. Bleibt diese Abfrage haengen, haengt der ganze Aufrufer mit:
  // am 11.08.2026 fuehrte der Klick "Zu Sellify (nur aktualisieren)" zu gar
  // nichts. Kein Vorgang, kein Fehler, keine Meldung — sendLeadToSellify stand
  // in genau diesem await und erreichte die Zeile nie, die sellify_status auf
  // "queued" setzt. Fuer den Nutzer war der Knopf kaputt.
  //
  // Nach der Frist gilt der Kontext als NICHT verfuegbar. Das ist die sichere
  // Richtung: nicht pruefbar heisst gesperrt, niemand wird versehentlich
  // angeschrieben — aber der Aufrufer bekommt eine Antwort und kann es sagen.
  const t0 = Date.now();
  state.recipientEligibilityBusy.add(lead.id);
  const context = await Promise.race([
    loadSellifyRecipientContext(lead).then((wert) => {
      console.info(`[outbound-lead-generation] Sperrvermerkspruefung ${lead.id}: ${Date.now() - t0} ms, `
        + `${(wert?.companies || []).length} Firmen, ${(wert?.people || []).length} Personen`);
      return wert;
    }),
    new Promise((resolve) => setTimeout(
      () => resolve({ people: [], companies: [], contextAvailable: false, timedOut: true }),
      RECIPIENT_ELIGIBILITY_TIMEOUT_MS,
    )),
  ]);
  if (context.timedOut) {
    console.warn('[outbound-lead-generation] Sperrvermerkspruefung hat die Frist ueberschritten', lead.id);
    // Ein BEREITS geprueftes Ergebnis darf durch einen Fristablauf nicht
    // verfallen. Der Serienbrief und die Sellify-Uebergabe rufen mit force auf;
    // laeuft diese zweite Pruefung in die Frist, fiel der eben noch freigegebene
    // Kontakt zurueck auf "gesperrt oder zu pruefen" — am 12.08.2026 endete die
    // Kette genau dort, obwohl der Sperrvermerk Sekunden zuvor sauber geprueft
    // worden war. Wir behalten dann das alte Urteil: ein freier Kontakt bleibt
    // frei, ein gesperrter bleibt gesperrt. Neu bewertet wird erst, wenn die
    // Pruefung wieder durchlaeuft.
    if (state.recipientEligibilityReady.has(lead.id)) {
      state.recipientEligibilityBusy.delete(lead.id);
      return new Map((lead.contacts || []).map((contact) => [
        contact.id,
        currentContactEligibility(lead, contact),
      ]));
    }
  }
  state.recipientEligibilityBusy.delete(lead.id);
  await uebernehmeCrmKontaktdaten(lead, context);
  const decisions = deriveLeadRecipientEligibility(lead, context);
  for (const [contactId, decision] of decisions) {
    if (typeof location !== 'undefined' && new URLSearchParams(location.search).has('urteilsfalle')) {
      const vorher = state.recipientEligibility.get(recipientEligibilityKey(lead.id, contactId));
      if (vorher && vorher.status !== decision.status) {
        console.warn('[URTEILSFALLE]', lead.id, contactId,
          `${vorher.status} -> ${decision.status}`, `force=${force}`,
          `timedOut=${!!context?.timedOut}`, `personen=${(context?.people || []).length}`,
          '\n', new Error('Aufrufspur').stack);
      }
    }
    state.recipientEligibility.set(recipientEligibilityKey(lead.id, contactId), decision);
  }
  state.recipientEligibilityReady.add(lead.id);
  // Nicht pruefbar (Frist, CTOX-Update, kein Sellify-Zugriff) ist kein Urteil:
  // nichts speichern und von selbst erneut pruefen. Vorher blieb der Lead bis
  // zum Neuladen auf "gesperrt oder zu pruefen" - nach jedem Update (die
  // Wartungssperre gilt schon waehrend des Baus, 25.09.2026: 13:22-14:05).
  if (context?.timedOut || context?.contextAvailable === false) {
    state.recipientEligibilityTimedOut.add(lead.id);
    planeSperrpruefungNeu(lead.id);
  } else {
    state.recipientEligibilityTimedOut.delete(lead.id);
    persistRecipientEligibility(lead, decisions);
  }
  // Ohne diesen Neuaufbau blieb das Banner auf dem Stand von vor der Pruefung
  // stehen, bis irgendein anderer Vorgang gerendert hat.
  if (state.selectedLeadId === lead.id) { try { renderDetail(); } catch { /* Anzeige darf die Pruefung nicht kippen */ } }
  return decisions;
}

// Geprueft wird, wo es RECHTLICH zaehlt — nicht der ganze Bestand.
//
// Diese Schleife lief ueber JEDEN Lead, sequenziell. Bei 21 Leads und gemessenen
// 5 bis 23 Sekunden je Pruefung sind das 100 bis 480 Sekunden am Stueck. Jeder
// Einzelaufruf hat 40 Sekunden Frist; die spaeteren rissen sie zwangslaeufig,
// fielen auf contextAvailable=false und erzeugten "Sellify-Sperrvermerk nicht
// pruefbar". Damit war jeder Kontakt gesperrt und die Kette endete vor der
// Uebergabe — an einer Warteschlange, nicht an einem Sperrvermerk.
//
// Der Schutzzweck bleibt vollstaendig erhalten: geprueft wird jeder Lead, bei
// dem jemand Empfaenger AUSGEWAEHLT hat, denn nur dort kann eine Sperre verletzt
// werden. Ein Lead ohne Auswahl kann niemanden anschreiben; seine Pruefung
// passiert spaeter beim Oeffnen oder beim Auswaehlen, wo sie allein laeuft und
// die Frist muehelos haelt.
async function refreshAllRecipientEligibility() {
  // Die Pruefung laeuft NICHT ueber alle Leads — das stellte 21 Vorgaenge in die
  // Schlange und blockierte die App. Sie laeuft fuer Leads mit Auswahl UND fuer
  // den gerade geoeffneten Lead.
  //
  // Der geoeffnete Lead ist der Teil, der am 12.08.2026 gefehlt hat, und sein
  // Fehlen war eine VERKLEMMUNG, kein Schoenheitsfehler:
  //   keine Auswahl -> keine Pruefung -> Kontrollkasten bleibt "wird geprueft"
  //   -> gesperrt -> keine Auswahl moeglich.
  // Auf CHEMOFAST gemessen: Roger Wintzen dauerhaft "zu pruefen", alle vier
  // Uebergabe-Schaltflaechen gesperrt, auch die Serien-E-Mail, die vorher lief.
  // Wer einen Lead ansieht, braucht sein Urteil; wer ihn nicht ansieht, nicht.
  const relevant = state.leads.filter((lead) => (
    lead?.id === state.selectedLeadId
    || (Array.isArray(lead?.selected_contact_ids) && lead.selected_contact_ids.length > 0)
  ));
  for (const lead of relevant) await refreshLeadRecipientEligibility(lead);
  return relevant.length;
}

// Fuenf der zwoelf Pflegefelder sind keine Entscheidungen, sondern Tatsachen,
// die die App selbst kennt. Sie blieben bei allen 26 Leads leer und hielten
// jeden einzelnen vor der Freigabe auf (gemessen 10.09.2026). Sie werden jetzt
// abgeleitet — nur wenn sie leer sind oder frueher schon abgeleitet wurden;
// was ein Mensch eingetragen hat, bleibt unangetastet. Die sieben echten
// Entscheidungen (Eignung, Ausschluss, Aufnahme, Begruendung, Verantwortlicher,
// Initialen, Listenstatus) bleiben beim Menschen.
const DERIVED_MAINTENANCE_FIELDS = Object.freeze([
  'herkunft_import',
  'statistische_kampagne',
  'aenderungsart',
  'adressquelle',
  'firma_email_domain_konflikt',
]);
const IMPORT_SOURCE_LABELS = Object.freeze({
  text: 'Freitext', excel: 'Excel', url: 'URL', document: 'Dokument', sellify_campaign: 'Sellify-Kampagne',
});
function derivedMaintenanceValues(lead, imports = state.imports || []) {
  const values = {};
  const job = (imports || []).find((entry) => entry?.id === lead?.import_id);
  if (job) {
    const art = IMPORT_SOURCE_LABELS[job.source_type] || String(job.source_type || 'Import');
    const datum = Number(job.created_at_ms) ? new Date(Number(job.created_at_ms)).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }) : '';
    values.herkunft_import = `${art}-Import „${job.title || lead?.campaign || ''}“${datum ? ` vom ${datum}` : ''}`;
  } else if (String(lead?.import_id || '').trim()) {
    values.herkunft_import = `Import ${lead.import_id}`;
  }
  const kampagne = String(lead?.campaign || '').trim();
  if (kampagne) values.statistische_kampagne = kampagne;
  const sellifyId = lead?.payload?.imported_row?.sellify_contact_id
    || lead?.payload?.sellify_contact_id
    || lead?.payload?.sellify_precheck?.contact_id;
  if (sellifyId) values.aenderungsart = `Aktualisierung (Sellify ${sellifyId})`;
  else if (lead?.payload?.sellify_precheck?.known === false) values.aenderungsart = 'Neuanlage';
  const adressFelder = new Set(['firma_anschrift', 'firma_besucheranschrift', 'firma_postanschrift', 'firma_plz', 'firma_ort']);
  const adressHosts = [...new Set((lead?.evidence || [])
    .filter((entry) => adressFelder.has(entry?.field_key || entry?.field))
    .map((entry) => normalizedDomain(entry?.source_url || entry?.url || '') || String(entry?.source_id || '').trim())
    .filter((host) => host && !['operator', SELLIFY_SOURCE_ID].includes(host)))];
  if (adressHosts.length) values.adressquelle = adressHosts.slice(0, 3).join(', ');
  const domain = normalizedDomain(researchFieldValue(lead, 'firma_domain') || lead?.domain || lead?.website || '');
  const adressen = [researchFieldValue(lead, 'firma_email'), ...(lead?.contacts || []).map((contact) => contact?.person_email || contact?.email)]
    .map((wert) => String(wert || '').trim().toLowerCase())
    .filter((wert) => wert.includes('@'));
  if (domain && adressen.length) {
    // beiersdorf.com neben beiersdorf.de ist dasselbe Haus, kein Konflikt.
    const haus = evidenceSourceProvider(domain);
    const fremd = [...new Set(adressen
      .map((wert) => wert.split('@')[1] || '')
      .filter((host) => host && evidenceSourceProvider(host) !== haus))];
    values.firma_email_domain_konflikt = fremd.length
      ? `Abweichung: ${fremd.join(', ')} statt ${domain}`
      : `konsistent (${domain})`;
  }
  return values;
}
// Leads mit bekannter Sellify-Nummer bekommen ihren Stammsatz in EINER
// Abfrage ueber die Datensatz-IDs. Die unscharfe Namenssuche je Lead brauchte
// unter Last 20 s und mehr und erreichte in einer Sitzung kaum einen Lead.
const SELLIFY_PRECHECK_VERSION = 2;
// Gescheiterte Vorabgleiche je Browser merken: vorher lief jeder App-Start in
// jedem Browser dieselben ~10 Leads erneut an (je 2 Sellify-Abfragen), weil nur
// ein Erfolg gespeichert wird (25.09.2026).
const VORABGLEICH_SPEICHER = 'ctox.olg.vorabgleich-versuche.v1';
function vorabgleichVersuchGespeichert(leadId) {
  try {
    const wert = JSON.parse(globalThis.localStorage?.getItem(VORABGLEICH_SPEICHER) || '{}')[leadId];
    return typeof wert === 'number' ? wert : undefined;
  } catch { return undefined; }
}
function merkeVorabgleichVersuch(leadId) {
  try {
    const alle = JSON.parse(globalThis.localStorage?.getItem(VORABGLEICH_SPEICHER) || '{}');
    const grenze = Date.now() - 24 * 3600_000;
    for (const [id, t] of Object.entries(alle)) if (!(t > grenze)) delete alle[id];
    alle[leadId] = Date.now();
    globalThis.localStorage?.setItem(VORABGLEICH_SPEICHER, JSON.stringify(alle));
  } catch { /* nur Komfort */ }
}
async function nachgeladeneSellifyStammsaetze() {
  const offen = (state.leads || []).filter((lead) => !lead?.payload?.sellify_snapshot
    && sellifyContactIdOfLead(lead)
    && !state.sellifyPrecheckTried.has(`snapshot:${lead.id}`));
  if (!offen.length) return new Map();
  for (const lead of offen) state.sellifyPrecheckTried.set(`snapshot:${lead.id}`, Date.now());
  const ids = [...new Set(offen.map((lead) => `sellify-company-${sellifyContactIdOfLead(lead)}`))];
  const firmen = await withTimeout(
    sellifyReadCollection('company').lookup({ ids, limit: ids.length }),
    'Sellify-Stammsaetze zu langsam',
    60000,
  );
  const nachId = new Map((firmen || [])
    .filter((firma) => firma && !firma.is_deleted)
    .map((firma) => [String(firma.contact_id || '').trim(), firma]));
  const ergebnis = new Map();
  for (const lead of offen) {
    const firma = nachId.get(sellifyContactIdOfLead(lead));
    if (firma) ergebnis.set(lead.id, sellifySnapshotAusVorwissen(sellifyVorwissenAusFirma(firma)));
  }
  return ergebnis;
}
function sellifyContactIdOfLead(lead) {
  return String(lead?.payload?.imported_row?.sellify_contact_id
    || lead?.payload?.sellify_contact_id
    || lead?.payload?.sellify_precheck?.contact_id
    || '').trim();
}
async function repairDerivedMaintenanceFields() {
  let geaendert = 0;
  let stammsaetze = new Map();
  try {
    stammsaetze = await nachgeladeneSellifyStammsaetze();
  } catch (fehler) {
    console.warn('[outbound-lead-generation] Sellify-Stammsaetze nicht nachgeladen', fehler);
  }
  // Vorabgleiche laufen zu dritt nebeneinander statt einzeln mit je bis zu
  // 20 s: ein neuer Lead wartete sonst bis zu 22 min (Klicktest P2 BG-09).
  // Ein gescheiterter Versuch darf nach 5 min erneut laufen.
  const braucheVorabgleich = (lead) => {
    const ohne = !lead?.payload?.sellify_precheck
      && !lead?.payload?.imported_row?.sellify_contact_id
      && !lead?.payload?.sellify_contact_id;
    const veraltet = lead?.payload?.sellify_precheck?.known === false
      && Number(lead?.payload?.sellify_precheck?.version || 1) < SELLIFY_PRECHECK_VERSION;
    const versuch = state.sellifyPrecheckTried.get(lead.id) ?? vorabgleichVersuchGespeichert(lead.id);
    const gesperrt = typeof versuch === 'number' && Date.now() - versuch < 30 * 60_000;
    return (ohne || veraltet) && !gesperrt;
  };
  const vorabgleiche = new Map();
  const warteschlange = (state.leads || []).filter(braucheVorabgleich);
  const arbeiter = async () => {
    for (let lead = warteschlange.shift(); lead; lead = warteschlange.shift()) {
      // Nutzeraktionen (Uebergabe, Sperrvermerkspruefung) gehen vor: der
      // Vorabgleich konkurrierte beim Start um dasselbe Abfragekontingent.
      for (let w = 0; w < 60 && (state.sellifyUebergabeVorbereitung?.size || state.recipientEligibilityBusy?.size); w += 1) {
        await new Promise((resolve) => globalThis.setTimeout(resolve, 1000));
      }
      state.sellifyPrecheckTried.set(lead.id, Date.now());
      merkeVorabgleichVersuch(lead.id);
      try {
        vorabgleiche.set(lead.id, { firma: await withTimeout(sellifyVorwissen(lead), 'Sellify-Vorabgleich zu langsam', 20000) });
      } catch (fehler) {
        console.warn('[outbound-lead-generation] Vorabgleich fuer Aenderungsart fehlgeschlagen', lead.id, fehler);
      }
    }
  };
  // Ein Arbeiter statt drei: Hintergrundpflege, kein Nutzerpfad.
  await arbeiter();
  for (const lead of state.leads || []) {
    const stammsatz = stammsaetze.get(lead.id);
    if (stammsatz) lead.payload = { ...(lead.payload || {}), sellify_snapshot: stammsatz };
    // Leads aus der Zeit vor dem gespeicherten Vorabgleich: einmal nachholen,
    // damit die Aenderungsart (Neuanlage/Aktualisierung) bestimmbar wird.
    let vorabgleichNeu = false;
    if (vorabgleiche.has(lead.id)) {
      {
        const { firma } = vorabgleiche.get(lead.id);
        lead.payload = {
          ...(lead.payload || {}),
          sellify_precheck: {
            known: Boolean(firma),
            contact_id: firma?.contact_id ? String(firma.contact_id) : '',
            name: String(firma?.name || ''),
            checked_at_ms: Date.now(),
            version: SELLIFY_PRECHECK_VERSION,
          },
          ...(firma ? { sellify_snapshot: sellifySnapshotAusVorwissen(firma) } : {}),
        };
        vorabgleichNeu = true;
      }
    }
    const abgeleitet = derivedMaintenanceValues(lead);
    const bisher = new Set(lead?.payload?.derived_field_keys || []);
    const data = { ...(lead?.data || {}) };
    const dataAenderungen = {};
    const neueSchluessel = new Set(bisher);
    let aenderung = false;
    for (const key of DERIVED_MAINTENANCE_FIELDS) {
      const wert = abgeleitet[key];
      if (!wert) continue;
      const aktuell = String(researchFieldValue(lead, key) || '').trim();
      const vonHand = (lead?.evidence || []).some((entry) => (entry?.field_key || entry?.field) === key && entry?.source_id === 'operator');
      if (vonHand) { neueSchluessel.delete(key); continue; }
      if (aktuell && !bisher.has(key)) continue;
      if (aktuell === wert) { neueSchluessel.add(key); continue; }
      data[key] = wert;
      dataAenderungen[key] = wert;
      neueSchluessel.add(key);
      aenderung = true;
    }
    if (!aenderung && !vorabgleichNeu && !stammsatz) continue;
    const payload = { ...(lead.payload || {}), derived_field_keys: [...neueSchluessel].sort() };
    lead.data = data;
    lead.payload = payload;
    // Nur die eigenen Schluessel schreiben, den Rest aus dem aktuellen
    // Datensatz: dazwischen lag bis zu 20 s Vorabgleich (Klicktest P2 V15).
    const payloadAenderungen = {
      derived_field_keys: payload.derived_field_keys,
      ...(vorabgleichNeu ? { sellify_precheck: payload.sellify_precheck } : {}),
      ...((vorabgleichNeu || stammsatz) && payload.sellify_snapshot ? { sellify_snapshot: payload.sellify_snapshot } : {}),
    };
    try {
      await patchLead(lead.id, { data: dataAenderungen, payload: payloadAenderungen }, { payloadMerge: true, dataMerge: true });
      geaendert += 1;
    } catch (fehler) {
      console.warn('[outbound-lead-generation] Pflegefelder nicht gespeichert', lead.id, fehler);
    }
  }
  return geaendert;
}

// Jede Person einer importierten Sellify-Kampagne gehoert in den Lead ihrer
// Firma. Die App gibt dem Agenten alle Sellify-Personen als Vorwissen mit,
// ob sie im Ergebnis bleiben, hing aber am Agenten: bei TROX SE blieb von drei
// Kampagnenpersonen nur eine, ueber alle fertigen Leads fehlte ein Drittel bis
// die Haelfte (26.09.2026). Nach jeder fertigen Recherche ergaenzt die App die
// fehlenden Kampagnenpersonen als Sellify-Kontakte; vorhandene Kontakte werden
// nur verknuepft, recherchierte Werte bleiben unangetastet.
const PERSONEN_ABGLEICH_VERSION = 2;
function braucheKampagnenPersonen(lead) {
  if (!['needs_review', 'completed'].includes(String(lead?.research_status || ''))) return false;
  if (!sellifyContactIdOfLead(lead)) return false;
  if (!leadKampagnen(lead).some((name) => name.startsWith('Sellify: '))) return false;
  const abgleich = lead.payload?.sellify_personen_abgleich || {};
  if (Number(abgleich.version || 0) < PERSONEN_ABGLEICH_VERSION) return true;
  // Gap closing and follow-up writebacks rewrite the contacts after the first
  // finish; the later of both times decides (Singulus, Diessner, Zschimmer &
  // Schwarz lost campaign persons after 26.09. because only the finish counted).
  const zuletzt = Math.max(
    Number(lead.payload?.research_finished_at_ms || 0),
    Number(lead.research_updated_at_ms || 0),
  );
  return Number(abgleich.at_ms || 0) < zuletzt;
}
async function ergaenzeKampagnenPersonen(lead) {
  const contactId = sellifyContactIdOfLead(lead);
  const kampagnen = leadKampagnen(lead).filter((name) => name.startsWith('Sellify: ')).map((name) => name.slice(9).trim());
  const mitglieder = await sellifyNativeLookup({
    entity: 'campaign',
    selectors: [{ field: 'contact_id', value: contactId }],
    fields: ['name', 'person_id', 'contact_id', 'is_deleted'],
    limit: 2000,
  });
  // A lookup that hits its limit may be cut off: never mark such a lead as
  // reconciled (the marker would otherwise claim every expected person).
  if ((mitglieder?.records || []).length >= 2000) throw new Error('Kampagnenpersonen: Sellify-Kampagnenliste unvollständig (Limit 2000)');
  const personIds = new Set((mitglieder?.records || [])
    .filter((row) => row && !row.is_deleted && kampagnen.includes(String(row.name || '').trim()))
    .map((row) => String(row.person_id || '').trim())
    .filter((id) => id && id !== '0'));
  let personen = [];
  if (personIds.size) {
    const antwort = await sellifyNativeLookup({
      entity: 'person',
      selectors: [{ field: 'contact_id', value: contactId }],
      limit: 100,
    });
    if ((antwort?.records || []).length >= 100) throw new Error('Kampagnenpersonen: Sellify-Personenliste unvollständig (Limit 100)');
    personen = (antwort?.records || []).filter((person) => person && !person.is_deleted
      && personIds.has(String(person.person_id || '').trim()));
    // Deleted Sellify persons count as found (they are deliberately not added).
    const gefunden = new Set((antwort?.records || []).filter(Boolean).map((person) => String(person.person_id || '').trim()));
    const fehlend = [...personIds].filter((id) => !gefunden.has(id));
    if (fehlend.length) throw new Error(`Kampagnenpersonen: ${fehlend.length} Sellify-Person(en) nicht gefunden`);
  }
  const aktuell = state.leads.find((entry) => entry.id === lead.id) || lead;
  const kontakte = Array.isArray(aktuell.contacts) ? aktuell.contacts.map((kontakt) => ({ ...kontakt })) : [];
  const norm = (value) => String(value || '').trim().toLowerCase();
  let ergaenzt = 0;
  let verknuepft = 0;
  for (const person of personen) {
    const key = `sellify-person-${String(person.person_id).trim()}`;
    const vorname = String(person.first_name || '').trim();
    const nachname = String(person.last_name || '').trim();
    const email = String(person.email || '').trim();
    const funktion = sellifyDeutsch(person.position || person.function || '');
    // E-mail/name fallback only for contacts without a different Sellify ID:
    // a contact already bound to another Sellify person (shared address,
    // same name) must not swallow this one (Singulus 8235, Zschimmer 41921,
    // Diessner 42117 stayed missing that way).
    const frei = (kontakt) => !kontakt.sellify_person_id || kontakt.sellify_person_id === key;
    const vorhanden = kontakte.find((kontakt) => kontakt.sellify_person_id === key || kontakt.person_key === key)
      || (email && kontakte.find((kontakt) => frei(kontakt) && norm(kontakt.person_email || kontakt.email) === norm(email)))
      || kontakte.find((kontakt) => frei(kontakt) && norm(kontakt.person_nachname) === norm(nachname)
        && norm(kontakt.person_vorname) === norm(vorname) && nachname);
    if (vorhanden) {
      if (!vorhanden.sellify_person_id) {
        vorhanden.sellify_person_id = key;
        vorhanden.crm_known = true;
        verknuepft += 1;
      }
      continue;
    }
    const name = [vorname, nachname].filter(Boolean).join(' ');
    if (!name) continue;
    kontakte.push({
      id: `contact_sellify_${key}`,
      person_vorname: vorname,
      person_nachname: nachname,
      person_funktion: funktion,
      person_email: email,
      person_telefon: String(person.phone || person.telephone || '').trim(),
      source: 'sellify',
      crm_known: true,
      sellify_person_id: key,
      person_key: key,
      sellify_kampagnenperson: true,
      name,
      role: funktion,
      position: funktion,
      email,
      phone: String(person.phone || person.telephone || '').trim(),
      conflicts: [],
    });
    ergaenzt += 1;
  }
  const patch = {
    payload: {
      ...(aktuell.payload || {}),
      sellify_personen_abgleich: {
        version: PERSONEN_ABGLEICH_VERSION,
        at_ms: Date.now(),
        kampagnen_personen: personIds.size,
        ergaenzt,
        verknuepft,
      },
    },
  };
  if (ergaenzt || verknuepft) patch.contacts = kontakte;
  await patchLead(lead.id, patch);
  // Lokal sofort nachziehen, sonst gilt der Lead bis zum naechsten Laden
  // weiter als offen und wird erneut abgeglichen.
  const lokal = state.leads.find((entry) => entry.id === lead.id);
  if (lokal) Object.assign(lokal, patch);
  return ergaenzt + verknuepft;
}
async function kampagnenPersonenPflege() {
  if (state.kampagnenPersonenLaeuft) return;
  state.kampagnenPersonenLaeuft = true;
  try {
    const offen = (state.leads || []).filter(braucheKampagnenPersonen)
      .filter((lead) => !(state.kampagnenPersonenFehler?.get(lead.id) > Date.now() - 30 * 60_000));
    for (const lead of offen.slice(0, 5)) {
      if (state.sellifyUebergabeVorbereitung?.size || state.recipientEligibilityBusy?.size || state.hintergrundImport) break;
      try {
        await withTimeout(ergaenzeKampagnenPersonen(lead), 'Kampagnenpersonen: Sellify zu langsam', 30000);
      } catch (fehler) {
        if (!state.kampagnenPersonenFehler) state.kampagnenPersonenFehler = new Map();
        state.kampagnenPersonenFehler.set(lead.id, Date.now());
        console.warn('[olg] Kampagnenpersonen nicht ergaenzt', lead.id, fehler);
      }
    }
  } finally {
    state.kampagnenPersonenLaeuft = false;
  }
}

async function enforceRecipientEligibility() {
  if (state.reconcilingRecipientEligibility) return 0;
  state.reconcilingRecipientEligibility = true;
  try {
    try { await repairDerivedMaintenanceFields(); } catch (fehler) {
      console.warn('[outbound-lead-generation] Pflegefelder konnten nicht abgeleitet werden', fehler);
    }
    await refreshAllRecipientEligibility();
    const repaired = await repairLeadRecipientSelections();
    if (repaired) await reload();
    render();
    return repaired;
  } finally {
    state.reconcilingRecipientEligibility = false;
  }
}

function stableContactIdentity(contact, index = 0) {
  const parts = [
    contact?.name,
    contact?.first_name,
    contact?.last_name,
    contact?.person_vorname,
    contact?.person_nachname,
    contact?.email,
    contact?.person_email,
    contact?.phone,
    contact?.person_telefon,
    contact?.linkedin,
    contact?.xing,
  ].map((value) => String(value || '').trim().toLowerCase());
  return parts.some(Boolean) ? parts.join('|') : `position-${index}`;
}

// Zwei Zeilen mit derselben Identitaet sind eine Person, keine zweite.
// Gemessen am 09.09.2026: CHEMOFAST trug 38 Kontakte, davon 35 Kopien; die
// alte Fassung hat die Kopien mit _2, _3, _4 umbenannt und damit aus einer
// Person fuenf gemacht, statt sie zusammenzufuehren. Danach hat
// repairLeadRecipientSelections die erfundenen Kennungen auch noch
// zurueckgeschrieben. Jetzt gewinnt die erste Zeile, und spaetere Zeilen
// steuern nur bei, was ihr fehlt.
// Ein Name, der nur aus Platzhaltern besteht, ist kein Ansprechpartner. Der
// native Teil weist solche Datensaetze seit dem 09.09.2026 ab; auf Carbosulf
// standen "test test" und "n/a n/a" aber noch aus einem alten Probelauf in der
// Liste und damit in der Freigabeansicht. Getroffen wird nur, wer ausser dem
// Platzhalternamen NICHTS mitbringt: keine Mail, kein Telefon, kein Profil.
const KONTAKT_PLATZHALTER = new Set([
  'test', 'tests', 'testing', 'na', 'n/a', 'nn', 'xx', 'xxx', 'unbekannt', 'unknown',
  'dummy', 'muster', 'mustermann', 'placeholder', 'platzhalter', 'tbd', 'todo', '-', '.',
]);
function contactIsPlaceholder(contact) {
  const tokens = String(contact?.name || '').split(/\s+/).map((t) => t.replace(/[.,]/g, '').toLowerCase()).filter(Boolean);
  if (!tokens.length || !tokens.every((t) => KONTAKT_PLATZHALTER.has(t))) return false;
  const traegt = ['person_email', 'email', 'person_telefon', 'phone', 'person_linkedin', 'person_xing', 'linkedin', 'xing']
    .some((feld) => String(contact?.[feld] || '').trim());
  return !traegt;
}
function contactIdentityKeys(contact, leadId, index) {
  const keys = [];
  const explicitId = String(contact?.id || '').trim();
  if (explicitId) keys.push(`id:${explicitId.toLowerCase()}`);
  const personKey = String(contact?.person_key || '').trim();
  if (personKey) keys.push(`pk:${personKey.toLowerCase()}`);
  const identity = stableContactIdentity(contact, index);
  if (!identity.startsWith('position-')) keys.push(`fp:${fingerprint(`${leadId}|${identity}`)}`);
  return keys;
}
function withStableContactIds(leadId, contacts = []) {
  const byKey = new Map();
  const kept = [];
  (Array.isArray(contacts) ? contacts : []).forEach((value, index) => {
    const contact = value && typeof value === 'object' ? value : {};
    if (contactIsPlaceholder(contact)) return;
    const keys = contactIdentityKeys(contact, leadId, index);
    const treffer = keys.map((key) => byKey.get(key)).find((entry) => entry !== undefined);
    if (treffer !== undefined) {
      for (const [feld, wert] of Object.entries(contact)) {
        const vorhanden = kept[treffer][feld];
        const leer = vorhanden === undefined || vorhanden === null || vorhanden === '';
        if (leer && wert !== undefined && wert !== null && wert !== '') kept[treffer][feld] = wert;
      }
      for (const key of keys) if (!byKey.has(key)) byKey.set(key, treffer);
      return;
    }
    const explicitId = String(contact.id || '').trim();
    const id = explicitId || `contact_${fingerprint(`${leadId}|${stableContactIdentity(contact, index)}`)}`;
    const position = kept.length;
    kept.push(contact.id === id ? { ...contact } : { ...contact, id });
    for (const key of [...keys, `id:${id.toLowerCase()}`]) if (!byKey.has(key)) byKey.set(key, position);
  });
  return kept;
}

function normalizeLeadRecipientShape(lead) {
  const contacts = withStableContactIds(lead?.id || 'lead', lead?.contacts || []);
  const contactIds = new Set(contacts.map((contact) => contact.id));
  const selected = Array.isArray(lead?.selected_contact_ids) ? lead.selected_contact_ids : [];
  const selected_contact_ids = [...new Set(selected
    .map((id) => String(id || '').trim())
    .filter((id) => contactIds.has(id)))];
  return { ...lead, contacts, selected_contact_ids };
}

async function repairLeadRecipientSelections() {
  const docs = await alleDokumente(state.collections.leads);
  let repaired = 0;
  for (const doc of docs) {
    const current = doc.toJSON?.() || doc;
    const normalized = normalizeLeadRecipientShape(current);
    const decisions = new Map(normalized.contacts.map((contact) => [
      contact.id,
      currentContactEligibility(normalized, contact),
    ]));
    const plan = buildCampaignRecipientList(normalized, decisions);
    // Eine nur ausstehende oder gescheiterte Sperrvermerk-Pruefung ist keine
    // Entscheidung: solche Empfaenger bleiben gespeichert. Vorher entfernte
    // ein Zeitablauf beim App-Start die Auswahl dauerhaft (Klicktest P3 #15).
    const ausstehend = new Set(plan.excluded
      .filter(({ decision }) => decision?.pending === true)
      .map(({ contact }) => contact.id));
    const selected_contact_ids = normalized.selected_contact_ids
      .filter((id) => plan.recipients.some((contact) => contact.id === id) || ausstehend.has(id));
    const contactsChanged = JSON.stringify(current.contacts || []) !== JSON.stringify(normalized.contacts);
    const selectionChanged = JSON.stringify(current.selected_contact_ids || []) !== JSON.stringify(selected_contact_ids);
    if (plan.excluded.length) {
      state.recipientRemovalNotices.set(normalized.id, plan.excluded);
    } else {
      const stillRestricted = (state.recipientRemovalNotices.get(normalized.id) || []).filter(({ contact }) => (
        currentContactEligibility(normalized, contact).status !== 'free'
      ));
      if (stillRestricted.length) state.recipientRemovalNotices.set(normalized.id, stillRestricted);
      else state.recipientRemovalNotices.delete(normalized.id);
    }
    if (!contactsChanged && !selectionChanged && Array.isArray(current.selected_contact_ids)) continue;
    await doc.incrementalPatch({
      contacts: normalized.contacts,
      selected_contact_ids,
      updated_at_ms: Date.now(),
    });
    repaired += 1;
  }
  return repaired;
}

// Eine Sellify-Vermerkpruefung ist kein Recherche-Auftrag. Zeigt ein Lead auf
// "queued"/"running" mit einer solchen command_id, war der Status gekapert.
function istVermerkBefehl(commandId) {
  return String(commandId || '').startsWith('leadgen-sperrvermerk-');
}

async function repairUntrackedResearchStatuses() {
  const invalid = state.leads.filter((lead) => (
    ['queued', 'running'].includes(lead.research_status)
    && (!String(lead.command_id || '').trim() || istVermerkBefehl(lead.command_id))
  ));
  if (!invalid.length) return 0;
  await Promise.all(invalid.map((lead) => patchLead(lead.id, {
    research_status: istVermerkBefehl(lead.command_id) && hatRechercheErgebnis(lead) ? 'needs_review' : 'new',
    task_id: '',
    command_id: '',
    payload: {
      ...(lead.payload || {}),
      research_recovery_reason: 'untracked_automation_reset',
      research_recovered_at_ms: Date.now(),
    },
  })));
  return invalid.length;
}

async function toggleSource(id) {
  // Der Schalter springt sofort um und ist bis zum Speichern gesperrt. Vorher
  // blieb er sekundenlang unveraendert, bis der Abgleich antwortete; ein
  // zweiter Klick schaltete die Quelle dann unbemerkt zurueck (Rundgang
  // 25.09.2026, CompanyHouse). Ziel ist der Zustand, den der Nutzer gesehen
  // hat, nicht der moeglicherweise veraltete lokale Datensatz.
  if (state.sourceTogglePending.has(id)) return;
  const angezeigt = state.sources.find((source) => source.id === id);
  if (!angezeigt) return;
  const enabled = angezeigt.enabled === false;
  state.sourceTogglePending.add(id);
  state.sourceToggleIntent.set(id, { enabled, bis: Date.now() + 120_000 });
  state.sources = state.sources.map((source) => source.id === id ? { ...source, enabled } : source);
  renderSourcePanel();
  try {
    const doc = await mitKanalHeilung(() => state.collections.sources.findOne(id).exec(), 'toggle-source');
    if (!doc) {
      state.sourceToggleIntent.delete(id);
      state.sources = state.sources.map((source) => source.id === id ? { ...source, enabled: !enabled } : source);
      await showBusinessAlert(`Die Quelle „${id}“ ist lokal nicht auffindbar – bitte neu laden.`);
      return;
    }
    const now = Date.now();
    await doc.incrementalPatch({ enabled, updated_at_ms: now });
    // Der Adapter folgt der Quelle: vorher blieb er enabled=1, auch nach dem
    // Neuladen (Nachtest F, T52) — der Abgleich beim Start sah nichts.
    const adapter = state.adapters.find((entry) => entry.source_id === id);
    const adapterDoc = adapter ? await state.collections.adapters.findOne(adapter.id).exec() : null;
    if (adapterDoc) {
      await adapterDoc.incrementalPatch({ ...adapterPflichtfelder({ ...doc.toJSON(), enabled }), enabled, updated_at_ms: now });
      state.adapters = state.adapters.map((entry) => entry.id === adapter.id ? { ...entry, enabled, updated_at_ms: now } : entry);
    }
    state.sources = state.sources.map((source) => source.id === id ? { ...source, enabled } : source);
    state.sourceTogglePending.delete(id);
    renderSourcePanel();
    await queueAdapterReconciliationAfterSourceChange('source_activation_changed');
  } catch (error) {
    state.sourceToggleIntent.delete(id);
    state.sources = state.sources.map((source) => source.id === id ? { ...source, enabled: !enabled } : source);
    await showBusinessAlert(`Umschalten fehlgeschlagen: ${String(error?.message || error)}`);
  } finally {
    if (state.sourceTogglePending.delete(id)) renderSourcePanel();
  }
}

function sourceCredentialSecretName(sourceId) {
  const suffix = String(sourceId || '')
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 44);
  return `OUTBOUND_${suffix || 'SOURCE'}_LOGIN`;
}

function showSourceCredentialDialog({ existingSecretName = '', onSave = null } = {}) {
  return new Promise((resolve) => {
    const layer = document.createElement('div');
    layer.className = 'business-dialog-layer is-info';
    layer.innerHTML = `
      <section class="business-dialog" role="dialog" aria-modal="true" aria-labelledby="leadgenCredentialTitle">
        <div class="business-dialog-copy">
          <h2 id="leadgenCredentialTitle">Zugang sicher speichern</h2>
          <p>E-Mail/Benutzername und Passwort werden als ein verschlüsseltes Credential im CTOX Secret Store gespeichert. In App-Daten, Agent-Prompts und Logs steht nur die Referenz.</p>
          ${existingSecretName ? `<p>Vorhandene Referenz: <code>${escapeHtml(existingSecretName)}</code>. „Beibehalten“ ändert sie nicht.</p>` : ''}
        </div>
        <label class="leadgen-form-field is-wide"><span>E-Mail oder Benutzername</span><input data-credential-username autocomplete="off" autocapitalize="off" autocorrect="off" spellcheck="false" data-1p-ignore data-lpignore="true" data-bwignore></label>
        <label class="leadgen-form-field is-wide"><span>Passwort</span><input data-credential-password type="password" autocomplete="new-password" autocapitalize="off" autocorrect="off" spellcheck="false" data-1p-ignore data-lpignore="true" data-bwignore></label>
        <div class="business-dialog-actions">
          <button class="business-dialog-secondary" type="button" data-credential-cancel>Abbrechen</button>
          <button class="business-dialog-secondary" type="button" data-credential-none>${existingSecretName ? 'Zugang entfernen' : 'Ohne Zugang'}</button>
          ${existingSecretName ? '<button class="business-dialog-secondary" type="button" data-credential-keep>Beibehalten</button>' : ''}
          <button class="business-dialog-primary" type="button" data-credential-save>Sicher speichern</button>
        </div>
      </section>`;
    // Hausregel: nichts ausserhalb der App. Auf document.body gestapelte
    // Vollbild-Schichten schluckten ALLE Klicks der Shell, sobald eine nicht
    // sauber schloss. Der Dialog lebt im App-Host, ist einzeln (alte Schicht
    // wird ersetzt) und schliesst per Escape und Backdrop-Klick.
    const dialogHost = state.ctx?.host || document.body;
    dialogHost.querySelectorAll(':scope > .business-dialog-layer').forEach((alt) => alt.remove());
    const escapeHandler = (event) => {
      if (event.key !== 'Escape' || !layer.isConnected) return;
      // Nur die oberste Ebene reagiert (liegt ein Hinweis darueber, ist er dran).
      const ebenen = layer.parentElement?.querySelectorAll(':scope > .business-dialog-layer') || [];
      if (ebenen.length && ebenen[ebenen.length - 1] !== layer) return;
      event.preventDefault();
      event.stopPropagation();
      layer.querySelector('[data-ss-cancel], [data-credential-cancel]')?.click();
    };
    layer.addEventListener('click', (event) => { if (event.target === layer) layer.querySelector('[data-ss-cancel], [data-credential-cancel]')?.click(); });
    // Es gibt kein DOM-Ereignis 'remove': der Handler blieb fuer immer haengen
    // (Klicktest-Befund P4 V9). Abgemeldet wird jetzt in close().
    document.addEventListener('keydown', escapeHandler, true);
    dialogHost.append(layer);
    // Ohne `is-open` bleibt die Ebene per CSS auf `opacity: 0` UND
    // `pointer-events: none`: der Dialog ist unsichtbar und alle Klicks und
    // Tastatureingaben gehen an das Fenster darunter. Genau das sah wie
    // "Dialog oeffnet unter dem Dialog" aus. Die Shell-Dialoge setzen die
    // Klasse im naechsten Frame; diese app-eigenen Dialoge taten es nie.
    // `requestAnimationFrame` feuert NICHT, solange das Fenster verdeckt oder
    // der Tab im Hintergrund ist — der Dialog bliebe dann fuer immer auf
    // opacity 0 und pointer-events none. Deshalb im Timer setzen (laeuft auch
    // verdeckt) und den Frame nur noch als Zusatz nehmen.
    const oeffnen = () => {
      layer.classList.add('is-open');
      // Nur fokussieren, solange noch nichts im Dialog den Fokus hat: der
      // spaete Frame (verdecktes Fenster) zog ihn sonst mitten im Tippen
      // ins erste Feld — das Passwort landete in der URL (Klicktest P0b).
      if (!layer.contains(document.activeElement)) layer.querySelector('input, textarea, select, button')?.focus?.();
    };
    window.setTimeout(oeffnen, 0);
    window.requestAnimationFrame(oeffnen);
    const username = layer.querySelector('[data-credential-username]');
    const password = layer.querySelector('[data-credential-password]');
    const close = (value) => {
      document.removeEventListener('keydown', escapeHandler, true);
      layer.classList.add('is-closing');
      window.setTimeout(() => { layer.remove(); resolve(value); }, 120);
    };
    layer.querySelector('[data-credential-cancel]').addEventListener('click', () => close(null));
    layer.querySelector('[data-credential-none]').addEventListener('click', () => close({ mode: 'none' }));
    layer.querySelector('[data-credential-keep]')?.addEventListener('click', () => close({ mode: 'keep' }));
    const zeigeHinweis = (text, rolle) => {
      let hinweis = layer.querySelector('[data-credential-hinweis]');
      if (!hinweis) {
        hinweis = document.createElement('p');
        hinweis.className = 'leadgen-import-notice';
        hinweis.dataset.credentialHinweis = '';
        layer.querySelector('.business-dialog-actions')?.before(hinweis);
      }
      hinweis.setAttribute('role', rolle);
      hinweis.textContent = text;
    };
    layer.querySelector('[data-credential-save]').addEventListener('click', async () => {
      const nextUsername = String(username.value || '').trim();
      const nextPassword = String(password.value || '');
      if (!nextUsername || !nextPassword) {
        zeigeHinweis('E-Mail/Benutzername und Passwort sind beide erforderlich.', 'alert');
        return;
      }
      // Wie im Quellen-Dialog: speichern, solange der Dialog offen ist, und
      // erst nach der Serverbestaetigung schliessen (Owner-Befund 11.09.2026).
      if (typeof onSave === 'function') {
        const knoepfe = [...layer.querySelectorAll('button')];
        const speichern = layer.querySelector('[data-credential-save]');
        // Abbrechen bleibt IMMER bedienbar (Owner-Befund: Dialog sperrte die App).
        knoepfe.forEach((knopf) => { if (!knopf.matches('[data-ss-cancel], [data-credential-cancel]')) knopf.disabled = true; });
        username.readOnly = true;
        password.readOnly = true;
        speichern.textContent = 'Wird gespeichert …';
        zeigeHinweis('Wird gespeichert …', 'status');
        try {
          await onSave({ username: nextUsername, password: nextPassword });
        } catch (error) {
          knoepfe.forEach((knopf) => { knopf.disabled = false; });
          username.readOnly = false;
          password.readOnly = false;
          speichern.textContent = 'Sicher speichern';
          zeigeHinweis(error?.message || String(error), 'alert');
          return;
        }
        password.value = '';
        close({ mode: 'save', saved: true });
        return;
      }
      password.value = '';
      close({ mode: 'save', username: nextUsername, password: nextPassword });
    });
    username.focus();
  });
}

// Jeder Befehl mit harter Frist. Owner-Befund 11.09.2026 abends: im Tab des
// Owners hing der Speicherbefehl VOR dem Einfuegen endlos ("eine Minute" ohne
// Ende, Dialog gesperrt), waehrend die App "Zugaenge" im selben Tab Befehle
// durchbrachte. Zwei Unterschiede zu ihr werden hier geschlossen:
// 1. `sync_queue_tasks: false` wirkt in der Shell nur am BEFEHL
//    (prepareCommandSync liest command.sync_queue_tasks); als Aufrufoption
//    wurde es ignoriert und jeder Befehl wartete zuerst auf die
//    Warteschlangen-Sammlung.
// 2. Die Befehlssammlung wird vorher gestartet (wie "Zugaenge").
// Und: nichts wartet unbegrenzt — nach Ablauf der Frist kommt ein Fehler.
// Letzter Lebenszyklus-Schritt je Befehl (die Shell meldet ihn als Ereignis):
// haengt ein Befehl, nennt die Meldung den Schritt statt nur "timeout".
const befehlsSchritte = new Map();
globalThis.addEventListener?.('ctox-business-command-lifecycle', (event) => {
  const id = String(event?.detail?.command_id || '');
  if (!id.startsWith('cmd_outbound') && !id.startsWith('cmd_leadgen')) return;
  befehlsSchritte.set(id, String(event.detail.phase || ''));
  if (befehlsSchritte.size > 200) befehlsSchritte.delete(befehlsSchritte.keys().next().value);
});
function befehlsSchritt(commandId) {
  return befehlsSchritte.get(String(commandId || '')) || 'vor dispatch_started';
}

async function sendeBefehl(command, options = {}) {
  const bus = state.ctx?.commandBus;
  if (typeof bus?.dispatch !== 'function') throw new Error('Der CTOX-Befehlskanal ist in dieser Sitzung nicht verfügbar.');
  const befehl = options.sync_queue_tasks === false ? { ...command, sync_queue_tasks: false } : command;
  const warte = (ms) => new Promise((resolve) => { globalThis.setTimeout(resolve, ms); });
  try {
    await Promise.race([Promise.resolve(state.ctx?.sync?.startCollection?.('business_commands')), warte(5_000)]);
  } catch { /* laeuft ggf. schon */ }
  const fristMs = Number(options.timeoutMs || 45_000) + 10_000;
  let timer = null;
  const t0 = Date.now();
  try {
    const antwort = await Promise.race([
      bus.dispatch(befehl, options),
      new Promise((_, reject) => {
        timer = globalThis.setTimeout(() => reject(new Error(`CTOX hat nicht innerhalb von ${Math.round(fristMs / 1000)} s geantwortet (timeout, letzter Schritt: ${befehlsSchritt(command?.id)})`)), fristMs);
      }),
    ]);
    befehlsDiagnose(command, `ok ${Date.now() - t0} ms`);
    return antwort;
  } catch (error) {
    befehlsDiagnose(command, `FEHLER ${Date.now() - t0} ms: ${String(error?.message || error).slice(0, 160)}`);
    throw error;
  } finally {
    if (timer) globalThis.clearTimeout(timer);
  }
}

// Letzter Ausgang je Befehlsart als Seitenattribut: die Konsole ist in der
// Automationsumgebung nicht lesbar, und "Befehl kam nie an" war am 25.09.2026
// von aussen nicht von "Befehl haengt im Browser" zu unterscheiden.
function befehlsDiagnose(command, text) {
  if (/wird aktualisiert|schreibgesch|read-?only|MAINTENANCE/i.test(text)) state.wartungGesehenAt = Date.now();
  else if (/^ok /.test(text)) state.wartungGesehenAt = 0;
  try {
    const bisher = JSON.parse(document.documentElement.dataset.olgBefehle || '{}');
    bisher[String(command?.command_type || '?')] = `${new Date().toISOString().slice(11, 19)} ${text} · ${befehlsSchritt(command?.id)}`;
    document.documentElement.dataset.olgBefehle = JSON.stringify(bisher);
  } catch { /* nur Diagnose */ }
}

// Zugang speichern = sofort lokal in RxDB schreiben, der Dialog wartet NICHT
// auf den Server (Owner-Vorgabe 11.09.2026 abends: "zwei Strings speichern,
// instant, synchronisiert ueber RxDB"). Die Uebertragung laeuft im
// Hintergrund; die Quellenzeile zeigt "wird uebertragen …", danach
// "hinterlegt (Datum)" oder den Grund, warum nichts ankam.
function zugangUebertragung(secretName) {
  return secretName ? state.zugangUebertragung?.get(secretName) || null : null;
}

function setzeZugangUebertragung(secretName, eintrag) {
  if (!(state.zugangUebertragung instanceof Map)) state.zugangUebertragung = new Map();
  if (eintrag) state.zugangUebertragung.set(secretName, eintrag);
  else state.zugangUebertragung.delete(secretName);
  if (state.sourcePanelOpen) renderSourcePanel();
}

function zugangsFehlerText(roh) {
  // Toter Befehlskanal des Tabs (Sync-Befund B1) oder keine Bestaetigung:
  // der Nutzer braucht die Handlung, nicht den Sync-Engine-Text.
  if (/peer-not-open|webrtc|rc_push|timed? ?out|timeout|zeit(ü|ue)berschreitung|nicht innerhalb/i.test(roh)) {
    return `Dieser Browser-Tab erreicht CTOX gerade nicht (${roh}). Bitte die Seite neu laden (⌘R bzw. F5) und den Zugang erneut eingeben.`;
  }
  return roh;
}

async function verfolgeZugangUebertragung(receipt, commandId, secretName) {
  try {
    const ergebnis = await Promise.race([
      receipt?.tracking?.waitForTerminal
        ? receipt.tracking.waitForTerminal({ timeoutMs: 120_000, sync_queue_tasks: false })
        : Promise.reject(new Error('keine Verfolgung')),
      new Promise((_, reject) => { globalThis.setTimeout(() => reject(new Error(`nicht innerhalb von 130 s beim Server angekommen (letzter Schritt: ${befehlsSchritt(commandId)})`)), 130_000); }),
    ]);
    if (ergebnis?.ok === false || ergebnis?.status === 'failed' || ergebnis?.terminal_status === 'failed') {
      throw new Error(ergebnis?.error || ergebnis?.result?.error || 'CTOX hat den Speicherbefehl abgelehnt.');
    }
    await entferneLokalenZugangswert(commandId, secretName);
    // Server hat bestaetigt: sofort als hinterlegt fuehren, der Katalog
    // bestaetigt im Hintergrund (sonst blitzte kurz "Zugang fehlt" auf).
    if (!(state.secretKatalog instanceof Map)) state.secretKatalog = new Map();
    state.secretKatalog.set(secretName, new Date().toISOString());
    state.secretKatalogStand = Date.now();
    setzeZugangUebertragung(secretName, null);
    void ladeSecretKatalog();
  } catch (error) {
    console.warn('[olg] Zugang nicht uebertragen', secretName, befehlsSchritt(commandId), String(error?.message || error));
    setzeZugangUebertragung(secretName, { commandId, fehler: zugangsFehlerText(String(error?.message || error)) });
  }
}

// Ein API-Schluessel ist EIN Wert, kein Benutzer-Passwort-Paar: er wird roh
// abgelegt, damit das Skript ihn unveraendert verwenden kann.
function istSchluesselZugang(secretName) {
  return /_(TOKEN|API_KEY)$/.test(String(secretName || ''));
}

async function putSourceCredential(secretName, username, password) {
  if (typeof state.ctx?.commandBus?.dispatch !== 'function') {
    throw new Error('Der CTOX Secret Store ist in dieser Sitzung nicht verfügbar.');
  }
  const commandId = `cmd_outbound_secret_put_${crypto.randomUUID()}`;
  const value = istSchluesselZugang(secretName) ? String(password || '') : JSON.stringify({ username, password });
  let receipt;
  try {
    receipt = await sendeBefehl({
      id: commandId,
      command_id: commandId,
      module: 'outbound-lead-generation',
      command_type: 'ctox.secret.put',
      record_id: secretName,
      inbound_channel: 'business_os.outbound_lead_generation',
      payload: { name: secretName, value },
      client_context: {
        source: 'outbound-lead-generation.source-credentials',
        source_module: 'outbound-lead-generation',
        actor: { id: state.ctx?.session?.user?.id || state.ctx?.session?.userId || '' },
      },
      // Lokal einfuegen, ohne vorher auf den Peer zu warten; die Replikation
      // liefert, sobald der Kanal steht (wie der Fehlerberichter der Shell).
      allow_local_intent_without_peer: true,
    }, { until: 'local', timeoutMs: 15_000, sync_queue_tasks: false });
  } catch (error) {
    throw new Error(`Der Zugang wurde NICHT gespeichert: ${zugangsFehlerText(String(error?.message || error))}`);
  }
  setzeZugangUebertragung(secretName, { commandId, seit: Date.now() });
  void verfolgeZugangUebertragung(receipt, commandId, secretName);
  return secretName;
}

// Der lokale Befehlsdatensatz traegt den Wert, bis der Server-Stand ihn
// ersetzt; eine lokale Folgeaenderung schob ihn erneut hoch (UI-Test
// 11.09.2026: Klartext Sekunden nach "gespeichert" im replizierten Dokument).
// Wie das Zugaenge-Modul: Wert lokal entfernen, sobald der Befehl durch ist.
async function entferneLokalenZugangswert(commandId, secretName) {
  try {
    const collection = state.ctx?.db?.collection?.('business_commands');
    const doc = collection ? await collection.findOne(commandId).exec() : null;
    if (doc?.payload && Object.prototype.hasOwnProperty.call(doc.payload, 'value')) {
      await doc.incrementalPatch({ payload: { name: secretName } });
    }
  } catch {
    // Der Server entfernt den Wert ohnehin aus seinem Stand.
  }
}

async function deleteSourceCredential(secretName) {
  if (!secretName || typeof state.ctx?.commandBus?.dispatch !== 'function') return;
  const commandId = `cmd_outbound_secret_delete_${crypto.randomUUID()}`;
  const result = await sendeBefehl({
    id: commandId,
    command_id: commandId,
    module: 'outbound-lead-generation',
    command_type: 'ctox.secret.delete',
    record_id: secretName,
    inbound_channel: 'business_os.outbound_lead_generation',
    payload: { name: secretName },
    client_context: {
      source: 'outbound-lead-generation.source-credentials',
      source_module: 'outbound-lead-generation',
    },
  }, { until: 'terminal', timeoutMs: 60_000, sync_queue_tasks: false });
  if (result?.ok === false || result?.status === 'failed' || result?.terminal_status === 'failed') {
    throw new Error(`Der Zugang wurde NICHT entfernt: ${result?.error || result?.result?.error || 'CTOX hat den Befehl abgelehnt.'}`);
  }
  void ladeSecretKatalog();
}

// Owner-Befund 11.09.2026: "Zugang · hinterlegt" stand da, sobald eine Quelle
// auf einen Secret-Namen VERWEIST. Ob dort ein Wert liegt und seit wann, wusste
// die App nicht (XING: Verweis gesetzt, Wert vom 21.07.). Sie fragt jetzt den
// Secret Store — nur Namen und Zeitstempel, nie Werte.
async function ladeSecretKatalog() {
  if (typeof state.ctx?.commandBus?.dispatch !== 'function') return;
  // Laeuft schon eine Abfrage, kann sie den gerade gespeicherten Zugang noch
  // nicht kennen: danach genau einmal nachladen statt still zu ueberspringen.
  if (state.secretKatalogLaeuft) { state.secretKatalogNochmal = true; return; }
  state.secretKatalogLaeuft = true;
  state.secretKatalogNochmal = false;
  const commandId = `cmd_outbound_secret_list_${crypto.randomUUID()}`;
  try {
    const receipt = await sendeBefehl({
      id: commandId,
      command_id: commandId,
      module: 'outbound-lead-generation',
      command_type: 'ctox.secret.list',
      record_id: 'credentials',
      inbound_channel: 'business_os.outbound_lead_generation',
      payload: {},
      client_context: {
        source: 'outbound-lead-generation.source-credentials',
        source_module: 'outbound-lead-generation',
        business_chat_auto_focus: false,
      },
    }, { until: 'terminal', timeoutMs: 30_000, sync_queue_tasks: false });
    const outcome = receipt?.result?.result || receipt?.result || receipt || {};
    const eintraege = [...(outcome.catalog || []), ...(outcome.extra || [])];
    state.secretKatalog = new Map(eintraege
      .filter((eintrag) => eintrag?.is_set)
      .map((eintrag) => [String(eintrag.name || ''), String(eintrag.updated_at || '')]));
    state.secretKatalogStand = Date.now();
    if (state.sourcePanelOpen) renderSourcePanel();
  } catch (error) {
    // Ohne Antwort bleibt der Stand unbekannt; nichts wird als hinterlegt behauptet.
    console.warn('[olg] Secret-Katalog nicht lesbar', String(error?.message || error));
    // Frische Tabs verloren den ersten Befehl, solange der Peer nicht
    // angemeldet war (Klicktest P1 B1). Einmal nach 15 s nachfassen.
    if (!state.secretKatalogNachgefasst) {
      state.secretKatalogNachgefasst = true;
      globalThis.setTimeout(() => { void ladeSecretKatalog(); }, 15_000);
    }
  } finally {
    state.secretKatalogLaeuft = false;
    if (state.secretKatalogNochmal) void ladeSecretKatalog();
  }
}

// null = unbekannt (Katalog nicht geladen), sonst { vorhanden, datum }.
function secretStand(secretName) {
  if (!secretName || !(state.secretKatalog instanceof Map)) return null;
  if (!state.secretKatalog.has(secretName)) return { vorhanden: false, datum: '' };
  const at = Date.parse(state.secretKatalog.get(secretName) || '');
  const datum = Number.isFinite(at)
    ? new Date(at).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' })
    : '';
  return { vorhanden: true, datum };
}


// Owner-Befund 04.09.2026: 53 wartende Aufgaben in der Warteschlange, keine
// davon eine Firmenrecherche - alles "repair scrape target ..." plus ein
// dauerhaft laufender Playwright-Abgleich (15 Minuten je Durchlauf). Die
// Recherchen standen dahinter und verhungerten.
//
// Ursache: JEDER Recherchestart prueft die Adapter-Konfiguration und stoesst
// bei abweichender Pruefsumme einen Abgleich an. Bei 17 ausgewaehlten Leads
// sind das bis zu 17 Abgleiche, jeder mit 21 Reparaturauftraegen.
//
// Ein Abgleich je Konfigurationsstand genuegt. Diese Sperre haelt das fest -
// im Speicher, damit sie auch ohne Serverantwort greift.
const ABGLEICH_SPERRE_MS = 30 * 60 * 1000;
let letzterAbgleichStand = '';
let letzterAbgleichAtMs = 0;

async function queueAdapterReconciliationAfterSourceChange(reason) {
  const stand = String(state.researchPolicyRecord?.configuration_digest || '')
    || `${state.sources.length}:${state.researchFieldKeys.length}`;
  const zuFrueh = Date.now() - letzterAbgleichAtMs < ABGLEICH_SPERRE_MS;
  if (stand === letzterAbgleichStand && zuFrueh) {
    console.info('[olg] Adapterabgleich uebersprungen (bereits angestossen)', { reason, stand });
    return;
  }
  letzterAbgleichStand = stand;
  letzterAbgleichAtMs = Date.now();
  try {
    await queueAdapterReconciliation(reason, state.researchPolicyRecord);
  } catch (error) {
    const message = String(error?.message || error);
    // Ein Service-/Replikationsneustart reisst den Command-Kanal kurz ab
    // ("collection ... was cancelled"). Das ist transient: einmal kurz warten
    // und neu versuchen, bevor der Nutzer eine Fehlermeldung sieht.
    if (/cancelled|abgebrochen/i.test(message)) {
      await recoverCommandChannel('adapter-reconcile');
      await new Promise((resolve) => setTimeout(resolve, 4000));
      try {
        await queueAdapterReconciliation(reason, state.researchPolicyRecord);
        return;
      } catch (retryError) {
        showBusinessAlert(`Die Quelle wurde gespeichert, aber der Adapter-Abgleich konnte nicht gestartet werden: ${String(retryError?.message || retryError)}`);
        return;
      }
    }
    showBusinessAlert(`Die Quelle wurde gespeichert, aber der Adapter-Abgleich konnte nicht gestartet werden: ${message}`);
  }
}

async function addSource() {
  const url = String(await showBusinessPrompt('Vollständige Adresse der Recherchequelle', {
    title: 'Quelle hinzufügen',
    defaultValue: 'https://',
    confirmLabel: 'Weiter',
    cancelLabel: 'Abbrechen',
  }) || '').trim();
  if (!url) return;
  let parsed;
  try { parsed = new URL(url); } catch { showBusinessAlert('Bitte eine gültige URL eingeben.'); return; }
  if (!['http:', 'https:'].includes(parsed.protocol) || !parsed.hostname) {
    showBusinessAlert('Recherchequellen müssen eine vollständige HTTP- oder HTTPS-Adresse verwenden.');
    return;
  }
  const id = parsed.hostname.replace(/^www\./, '').toLowerCase();
  // Host UND Anbieter pruefen: der Anbieter von "x.invalid" ist "x" und fiel
  // durch, obwohl die Meldung .invalid/.test nennt (Klicktest-Befund P4 V3).
  if (isDocumentationSourceKey(id) || isDocumentationSourceKey(evidenceSourceKey({ source_id: id, source_url: url }))) {
    showBusinessAlert(`${id} ist eine Beispiel- oder Testadresse (example, .invalid, .test) und kann keine Recherchequelle sein.`);
    return;
  }
  // Erst pruefen, dann Zugang abfragen: vorher wurde der Zugang gespeichert
  // (und ein vorhandener gleichnamiger ueberschrieben), bevor "bereits
  // vorhanden" kam.
  if (await state.collections.sources.findOne(id).exec()) { showBusinessAlert('Diese Quelle ist bereits vorhanden.'); return; }
  // Erst die Quelle, dann der Zugang: umgekehrt hinterliess ein Neuladen
  // waehrend "Sicher speichern" einen gespeicherten Zugang ohne Quelle
  // (Klicktest P4 T37b). Jetzt verweist die Quelle schon auf den Namen; der
  // Katalog zeigt ehrlich, ob dort (schon) ein Wert liegt.
  let quelleAngelegt = false;
  const legeQuelleAn = async (credentialSecretName) => {
    if (quelleAngelegt) return;
    const now = Date.now();
    const source = {
      id, label: id, url, countries: ['DE', 'AT', 'CH'], field_keys: [], enabled: true,
      requires_credential: Boolean(credentialSecretName), credential_secret_name: credentialSecretName, target_key: id.replace(/[^a-z0-9]+/g, '-'),
      // Mit Zugang wird die Quelle erst NACH dem bestaetigten Speichern
      // angelegt, in EINEM Schreibvorgang: ein zweiter Patch kurz nach dem
      // Einfuegen ging in der Replikation verloren, der Server blieb auf
      // "required" (Nachtest F, NF-2).
      adapter_status: 'draft', scrape_status: 'target_available', auth_status: credentialSecretName ? 'credential_available' : 'not_required',
      payload: { builtin: false, secret_value_in_payload: false }, created_at_ms: now, updated_at_ms: now,
    };
    await state.collections.sources.insert(source);
    quelleAngelegt = true;
    // The durable RxDB subscription remains authoritative, but its reflected
    // collection event may arrive after the currently open modal has rendered.
    // Mirror the successful insert immediately so the new adapter can be edited,
    // inspected or removed without closing/reloading the application.
    state.sources = [...state.sources, source].sort((a, b) => a.label.localeCompare(b.label, 'de'));
    renderSourcePanel();
  };
  const credentialSecretName = sourceCredentialSecretName(id);
  const choice = await showSourceCredentialDialog({
    existingSecretName: '',
    // Zugang zuerst, dann die Quelle — beides, solange der Dialog offen ist
    // und "Wird gespeichert …" zeigt; ein Fehler bleibt im Dialog. Scheitert
    // das Anlegen nach gespeichertem Zugang, liegt nur ein Secret ohne Quelle
    // vor; ein neuer Versuch ueberschreibt es unter demselben Namen.
    onSave: async ({ username, password }) => {
      await putSourceCredential(credentialSecretName, username, password);
      await legeQuelleAn(credentialSecretName);
    },
  });
  if (!choice) return;
  if (!(choice.mode === 'save' && choice.saved)) await legeQuelleAn('');
  await queueAdapterReconciliationAfterSourceChange('source_added');
}

function zugangsLegende(secretName) {
  if (!secretName) return ' · keiner hinterlegt';
  const code = `<code>${escapeHtml(secretName)}</code>`;
  const stand = secretStand(secretName);
  if (!stand) return state.secretKatalogLaeuft
    ? ` · Verweis auf ${code} (Stand wird geprüft …)`
    : ` · Verweis auf ${code} (Secret Store nicht erreichbar, Stand unbekannt)`;
  if (!stand.vorhanden) return ` · Verweis auf ${code}, im Secret Store liegt aber KEIN Wert`;
  return ` · hinterlegt${stand.datum ? ` am ${stand.datum}` : ''} (${code})`;
}

async function editSource(sourceId) {
  const item = state.sources.find((entry) => entry.id === sourceId);
  if (!item || isInternalResearchSource(item)) return;
  // Nicht auf den Katalog warten: der Dialog oeffnete erst nach bis zu 18 s
  // (Klicktest P4 T11). Er oeffnet sofort; die Legende wird nachgetragen.
  if (item.credential_secret_name && (!state.secretKatalogStand || Date.now() - state.secretKatalogStand > 60_000)) {
    void ladeSecretKatalog().then(async () => {
      // Lief schon eine Abfrage, kehrte der Aufruf sofort zurueck und die
      // Legende blieb auf "wird geprueft" stehen (Review, klein).
      for (let i = 0; i < 60 && state.secretKatalogLaeuft; i += 1) {
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      const legende = state.ctx.host.querySelector('.leadgen-source-settings legend');
      if (legende) legende.innerHTML = `Zugang${zugangsLegende(item.credential_secret_name)}`;
    });
  }
  const inputDriven = item.payload?.input_driven === true;
  // Ein Schluessel-Zugang (Bright Data) hat EIN Feld, kein Paar.
  const schluesselZugang = istSchluesselZugang(item.credential_secret_name || sourceCredentialSecretName(item.id));
  // Owner-Vorgabe 30.08.: EIN Einstellungsdialog mit URL, Zugangsdaten und
  // Freitext-Anweisungen - statt einer Prompt-Kette, die fuer input-getriebene
  // Quellen sogar vor den Zugangsdaten abbrach.
  const result = await new Promise((resolve) => {
    const layer = document.createElement('div');
    layer.className = 'business-dialog-layer is-info';
    layer.innerHTML = `
      <section class="business-dialog leadgen-source-settings" role="dialog" aria-modal="true" aria-labelledby="leadgenSourceSettingsTitle">
        <div class="business-dialog-copy">
          <h2 id="leadgenSourceSettingsTitle">Quelle: ${escapeHtml(item.label)}</h2>
        </div>
        ${inputDriven
          ? `<p class="leadgen-muted">Diese Quelle nutzt automatisch die Unternehmensdomain des jeweiligen Leads - keine feste URL.</p>`
          : `<label class="leadgen-form-field is-wide"><span>URL</span><input data-ss-url value="${escapeHtml(item.url || '')}"></label>`}
        <fieldset class="leadgen-source-settings-cred">
          <legend>Zugang${zugangsLegende(item.credential_secret_name)}</legend>
          <p class="leadgen-muted">${schluesselZugang
            ? 'Der API-Schlüssel landet verschlüsselt im CTOX Secret Store; App und Agenten sehen nur die Referenz. Der Datenzugriff holt ihn erst zur Laufzeit.'
            : 'E-Mail/Benutzername und Passwort landen verschlüsselt im CTOX Secret Store; App und Agenten sehen nur die Referenz.'}</p>
          ${schluesselZugang
            ? `<label class="leadgen-form-field is-wide"><span>API-Schlüssel</span><input data-ss-pass type="password" autocomplete="off" autocapitalize="off" autocorrect="off" spellcheck="false" data-1p-ignore data-lpignore="true" data-bwignore placeholder="${item.credential_secret_name ? 'unverändert lassen' : ''}"></label>`
            : `<label class="leadgen-form-field is-wide"><span>E-Mail oder Benutzername</span><input data-ss-user autocomplete="off" autocapitalize="off" autocorrect="off" spellcheck="false" data-1p-ignore data-lpignore="true" data-bwignore placeholder="${item.credential_secret_name ? 'unverändert lassen' : ''}"></label>
          <label class="leadgen-form-field is-wide"><span>Passwort</span><input data-ss-pass type="password" autocomplete="new-password" autocapitalize="off" autocorrect="off" spellcheck="false" data-1p-ignore data-lpignore="true" data-bwignore placeholder="${item.credential_secret_name ? 'unverändert lassen' : ''}"></label>`}
          ${item.credential_secret_name ? '<label class="leadgen-form-field"><input type="checkbox" data-ss-remove-cred> Zugang entfernen</label>' : ''}
        </fieldset>
        <label class="leadgen-form-field is-wide"><span>Anweisungen für diese Quelle (Freitext)</span>
          <textarea data-ss-instructions rows="4" placeholder="Welche Informationen sollen hier geholt werden, worauf ist zu achten?">${escapeHtml(String(item.payload?.instructions || ''))}</textarea>
        </label>
        <div class="business-dialog-actions">
          <button class="business-dialog-secondary" type="button" data-ss-cancel>Abbrechen</button>
          <button class="business-dialog-primary" type="button" data-ss-save>Speichern</button>
        </div>
      </section>`;
    // Hausregel: nichts ausserhalb der App. Auf document.body gestapelte
    // Vollbild-Schichten schluckten ALLE Klicks der Shell, sobald eine nicht
    // sauber schloss. Der Dialog lebt im App-Host, ist einzeln (alte Schicht
    // wird ersetzt) und schliesst per Escape und Backdrop-Klick.
    const dialogHost = state.ctx?.host || document.body;
    dialogHost.querySelectorAll(':scope > .business-dialog-layer').forEach((alt) => alt.remove());
    const escapeHandler = (event) => {
      if (event.key !== 'Escape' || !layer.isConnected) return;
      // Nur die oberste Ebene reagiert (liegt ein Hinweis darueber, ist er dran).
      const ebenen = layer.parentElement?.querySelectorAll(':scope > .business-dialog-layer') || [];
      if (ebenen.length && ebenen[ebenen.length - 1] !== layer) return;
      event.preventDefault();
      event.stopPropagation();
      layer.querySelector('[data-ss-cancel], [data-credential-cancel]')?.click();
    };
    layer.addEventListener('click', (event) => { if (event.target === layer) layer.querySelector('[data-ss-cancel], [data-credential-cancel]')?.click(); });
    // Es gibt kein DOM-Ereignis 'remove': der Handler blieb fuer immer haengen
    // (Klicktest-Befund P4 V9). Abgemeldet wird jetzt in close().
    document.addEventListener('keydown', escapeHandler, true);
    dialogHost.append(layer);
    // Ohne `is-open` bleibt die Ebene per CSS auf `opacity: 0` UND
    // `pointer-events: none`: der Dialog ist unsichtbar und alle Klicks und
    // Tastatureingaben gehen an das Fenster darunter. Genau das sah wie
    // "Dialog oeffnet unter dem Dialog" aus. Die Shell-Dialoge setzen die
    // Klasse im naechsten Frame; diese app-eigenen Dialoge taten es nie.
    // `requestAnimationFrame` feuert NICHT, solange das Fenster verdeckt oder
    // der Tab im Hintergrund ist — der Dialog bliebe dann fuer immer auf
    // opacity 0 und pointer-events none. Deshalb im Timer setzen (laeuft auch
    // verdeckt) und den Frame nur noch als Zusatz nehmen.
    const oeffnen = () => {
      layer.classList.add('is-open');
      // Nur fokussieren, solange noch nichts im Dialog den Fokus hat: der
      // spaete Frame (verdecktes Fenster) zog ihn sonst mitten im Tippen
      // ins erste Feld — das Passwort landete in der URL (Klicktest P0b).
      if (!layer.contains(document.activeElement)) layer.querySelector('input, textarea, select, button')?.focus?.();
    };
    window.setTimeout(oeffnen, 0);
    window.requestAnimationFrame(oeffnen);
    const close = (value) => { document.removeEventListener('keydown', escapeHandler, true); layer.classList.add('is-closing'); window.setTimeout(() => { layer.remove(); resolve(value); }, 120); };
    layer.querySelector('[data-ss-cancel]').addEventListener('click', () => close(null));
    layer.querySelector('[data-ss-save]').addEventListener('click', async () => {
      const passEl = layer.querySelector('[data-ss-pass]');
      // Neue Zugangsdaten UND "Zugang entfernen" zugleich: vorher gewann still
      // das Entfernen (Klicktest P4 T55). Im Dialog bleiben und fragen.
      const entfernen = layer.querySelector('[data-ss-remove-cred]')?.checked === true;
      const neueDaten = String(layer.querySelector('[data-ss-user]')?.value || '').trim() || String(passEl?.value || '');
      const zeigeFehler = (text) => {
        let hinweis = layer.querySelector('[data-ss-konflikt]');
        if (!hinweis) {
          hinweis = document.createElement('p');
          hinweis.className = 'leadgen-import-notice';
          hinweis.setAttribute('role', 'alert');
          hinweis.dataset.ssKonflikt = '';
          layer.querySelector('.business-dialog-actions')?.before(hinweis);
        }
        hinweis.textContent = text;
      };
      if (entfernen && neueDaten) {
        zeigeFehler('Bitte entweder neue Zugangsdaten eingeben oder „Zugang entfernen“ wählen – nicht beides.');
        return;
      }
      // Pruefungen IM Dialog: vorher schloss er erst und verwarf dann alle
      // Eingaben wegen einer Meldung (Klicktest P4 V12).
      const nurName = String(layer.querySelector('[data-ss-user]')?.value || '').trim();
      if (!entfernen && !schluesselZugang && Boolean(nurName) !== Boolean(String(passEl?.value || ''))) {
        zeigeFehler('E-Mail/Benutzername und Passwort sind beide erforderlich.');
        return;
      }
      const urlFeld = layer.querySelector('[data-ss-url]');
      if (urlFeld) {
        let geprueft = null;
        try { geprueft = new URL(String(urlFeld.value || '').trim()); } catch { geprueft = null; }
        if (!geprueft || !['http:', 'https:'].includes(geprueft.protocol) || !geprueft.hostname) {
          zeigeFehler('Bitte eine vollständige HTTP- oder HTTPS-Adresse eingeben.');
          return;
        }
        if (item.payload?.builtin !== true && geprueft.hostname.replace(/^www\./, '').toLowerCase() !== item.id) {
          zeigeFehler('Die Domain ist die stabile Quellen-ID. Für eine andere Domain bitte eine neue Quelle anlegen.');
          return;
        }
      }
      const value = {
        url: String(layer.querySelector('[data-ss-url]')?.value || '').trim(),
        username: String(layer.querySelector('[data-ss-user]')?.value || '').trim(),
        password: String(passEl?.value || ''),
        removeCred: layer.querySelector('[data-ss-remove-cred]')?.checked === true,
        instructions: String(layer.querySelector('[data-ss-instructions]')?.value || '').trim(),
      };
      // Owner-Befund 11.09.2026 (XING): der Dialog schloss sofort, das
      // Speichern lief danach bis zu 60 s unsichtbar — die Zeile sah
      // unveraendert aus, keine Bestaetigung, kein Fehler. Der Zugang wird
      // jetzt IM offenen Dialog gespeichert; er schliesst erst nach der
      // Serverbestaetigung, ein Fehler steht im Dialog, die Eingaben bleiben.
      if (!value.removeCred && value.password && (schluesselZugang || value.username)) {
        const secretName = item.credential_secret_name || sourceCredentialSecretName(item.id);
        const knoepfe = [...layer.querySelectorAll('button')];
        const speichern = layer.querySelector('[data-ss-save]');
        // Abbrechen bleibt IMMER bedienbar (Owner-Befund: Dialog sperrte die App).
        knoepfe.forEach((knopf) => { if (!knopf.matches('[data-ss-cancel], [data-credential-cancel]')) knopf.disabled = true; });
        layer.querySelectorAll('input, textarea').forEach((feld) => { feld.readOnly = true; });
        speichern.textContent = 'Wird gespeichert …';
        zeigeFehler('Wird gespeichert …');
        layer.querySelector('[data-ss-konflikt]')?.setAttribute('role', 'status');
        try {
          await putSourceCredential(secretName, value.username, value.password);
        } catch (error) {
          knoepfe.forEach((knopf) => { knopf.disabled = false; });
          layer.querySelectorAll('input, textarea').forEach((feld) => { feld.readOnly = false; });
          speichern.textContent = 'Speichern';
          zeigeFehler(error?.message || String(error));
          layer.querySelector('[data-ss-konflikt]')?.setAttribute('role', 'alert');
          return;
        }
        value.password = '';
        value.credentialSaved = true;
        value.credentialSecretName = secretName;
      }
      if (passEl) passEl.value = '';
      close(value);
    });
    (layer.querySelector('[data-ss-url]') || layer.querySelector('[data-ss-user]'))?.focus();
  });
  if (!result) return;
  let url = item.url;
  if (!inputDriven) {
    url = result.url;
    let parsed;
    try { parsed = new URL(url); } catch { showBusinessAlert('Bitte eine gültige URL eingeben.'); return; }
    if (!['http:', 'https:'].includes(parsed.protocol) || !parsed.hostname) {
      showBusinessAlert('Recherchequellen müssen eine vollständige HTTP- oder HTTPS-Adresse verwenden.');
      return;
    }
    const nextId = parsed.hostname.replace(/^www\./, '').toLowerCase();
    if (item.payload?.builtin !== true && nextId !== item.id) {
      showBusinessAlert('Die Domain ist die stabile Quellen-ID. Für eine andere Domain bitte eine neue Quelle anlegen.');
      return;
    }
  }
  let credentialSecretName = item.credential_secret_name || '';
  let credentialSaved = false;
  let zugangEntfernt = false;
  let zuLoeschendesSecret = '';
  if (result.removeCred) {
    // "Zugang entfernen" entfernte bisher nur den Verweis; der Wert blieb im
    // Secret Store liegen. Geloescht wird NACH dem Quellen-Patch: umgekehrt
    // zeigte die Quelle bei einem gescheiterten Patch auf ein geloeschtes
    // Secret (Klicktest P4 T40 / V21).
    zuLoeschendesSecret = credentialSecretName;
    // Eingebaute Login-Quellen (XING, LinkedIn, D&B, Leadfeeder) brauchen
    // immer einen Zugang: der Wert geht, Verweis und Pflicht bleiben. Vorher
    // setzte der naechste App-Start den Verweis wieder, requires_credential
    // blieb aber false — widerspruechlicher Stand (Klicktest-Befund P4 V8).
    const eingebauterLogin = SOURCE_DEFS.find((definition) => definition.id === item.id)?.credentialSecretName || '';
    zugangEntfernt = true;
    credentialSecretName = eingebauterLogin && eingebauterLogin === credentialSecretName ? credentialSecretName : '';
  } else if (result.credentialSaved) {
    // Im Dialog gespeichert und vom Server bestaetigt.
    credentialSecretName = result.credentialSecretName;
    credentialSaved = true;
  }
  // Nichts geaendert = nichts schreiben. Vorher setzte jedes Speichern den
  // Adapter auf "draft" zurueck und loeschte sein letztes Pruefergebnis
  // (Klicktest-Befund P4 V13).
  const nichtsGeaendert = !credentialSaved
    && !result.removeCred
    && String(url || '') === String(item.url || '')
    && String(result.instructions || '') === String(item.payload?.instructions || '');
  if (nichtsGeaendert) return;
  const now = Date.now();
  const patch = {
    url,
    requires_credential: Boolean(credentialSecretName),
    credential_secret_name: credentialSecretName,
    // Frisch gespeichert = hinterlegt. Unveraenderter Zugang behaelt seinen
    // Stand; vorher setzte JEDES Speichern (auch nur der Anweisungen) auf
    // "required" zurueck, und die Zeile zeigte "Zugang fehlt".
    auth_status: !credentialSecretName
      ? 'not_required'
      : credentialSaved
        ? 'credential_available'
        : zugangEntfernt
          ? 'required'
          : (item.auth_status && item.auth_status !== 'not_required' ? item.auth_status : 'required'),
    payload: { ...(item.payload || {}), instructions: result.instructions, secret_value_in_payload: false },
    updated_at_ms: now,
  };
  const sourceDoc = await state.collections.sources.findOne(item.id).exec();
  await sourceDoc?.incrementalPatch(patch);
  const adapter = state.adapters.find((entry) => entry.source_id === item.id);
  const adapterDoc = adapter ? await state.collections.adapters.findOne(adapter.id).exec() : null;
  await adapterDoc?.incrementalPatch({
    status: 'draft',
    scrape_status: 'target_available',
    auth_status: patch.auth_status,
    last_error: '',
    updated_at_ms: now,
  });
  state.sources = state.sources.map((entry) => entry.id === item.id ? { ...entry, ...patch } : entry);
  state.adapters = state.adapters.map((entry) => entry.source_id === item.id
    ? { ...entry, status: 'draft', scrape_status: 'target_available', auth_status: patch.auth_status, last_error: '', updated_at_ms: now }
    : entry);
  renderSourcePanel();
  if (zuLoeschendesSecret) {
    try {
      await deleteSourceCredential(zuLoeschendesSecret);
    } catch (error) {
      await showBusinessAlert(`Der Verweis ist entfernt, aber der gespeicherte Zugang ${zuLoeschendesSecret} konnte nicht gelöscht werden: ${error?.message || error}`);
      return;
    }
  }
  // Kein Hinweisfenster nach dem Speichern: die Zeile zeigt "wird
  // uebertragen …" und danach "hinterlegt (Datum)" (Owner-Vorgabe: nichts
  // blockiert die App, um zwei Werte zu speichern).
  if (zugangEntfernt) {
    await showBusinessAlert(`Zugang für ${item.label} wurde aus dem CTOX Secret Store entfernt.`);
  }
  await queueAdapterReconciliationAfterSourceChange('source_settings_changed');
}

async function deleteSource(sourceId) {
  const item = state.sources.find((entry) => entry.id === sourceId);
  if (!item || item.payload?.builtin !== false) return;
  const confirmed = await showBusinessConfirm(
    `Die Quelle „${item.label}“ und ihr eingerichteter Datenzugriff werden vollständig gelöscht.`,
    {
      title: 'Quelle löschen',
      confirmLabel: 'Quelle löschen',
      kind: 'danger',
    },
  );
  if (!confirmed) return;
  const adapter = state.adapters.find((entry) => entry.source_id === sourceId);
  const adapterDoc = adapter ? await state.collections.adapters.findOne(adapter.id).exec() : null;
  if (adapterDoc) await adapterDoc.remove();
  const sourceDoc = await state.collections.sources.findOne(sourceId).exec();
  const geloeschterStand = sourceDoc ? (sourceDoc.toJSON?.() || sourceDoc) : null;
  if (sourceDoc) await sourceDoc.remove();
  state.adapters = state.adapters.filter((entry) => entry.source_id !== sourceId);
  state.sources = state.sources.filter((entry) => entry.id !== sourceId);
  renderSourcePanel();
  // Erst wenn CTOX die Loeschung der Quelle bestaetigt hat, geht ihr Zugang.
  // Umgekehrt blieb in einem Tab mit gestoerter Verbindung die Quelle am
  // Server aktiv und zeigte auf einen geloeschten Zugang (Nachtest P4 N1).
  if (item.credential_secret_name && item.credential_secret_name === sourceCredentialSecretName(item.id)) {
    try {
      // Ohne Dokumentliste: alle ausstehenden lokalen Schreibvorgaenge der
      // Sammlung hochschieben und Erfolg verlangen (pushToRemotePeers).
      if (geloeschterStand) await flushReplicatedCollection('outbound_lead_generation_sources', []);
    } catch (error) {
      await showBusinessAlert(`Die Quelle ist hier gelöscht, CTOX hat die Löschung aber noch nicht bestätigt (${error?.message || error}). Der gespeicherte Zugang bleibt deshalb vorerst erhalten; bitte die Seite neu laden und prüfen.`);
      await queueAdapterReconciliationAfterSourceChange('source_deleted');
      return;
    }
    try {
      await deleteSourceCredential(item.credential_secret_name);
    } catch (error) {
      await showBusinessAlert(`Die Quelle ist gelöscht, ihr gespeicherter Zugang konnte aber nicht entfernt werden: ${error?.message || error}`);
    }
  }
  await queueAdapterReconciliationAfterSourceChange('source_deleted');
}

function adapterCommandOperation(commandType, targetKey) {
  const operation = String(commandType || '').trim();
  const target_key = String(targetKey || '').trim();
  if (!operation || !target_key) throw new Error('Adapter-Operation und Ziel sind erforderlich.');
  return { operation, target_key };
}

// Dieselbe Sammlung nutzt die Browser-App der Shell mit strengerem Schema
// (Pflicht: url, adapter_kind, enabled). Fehlen die Felder, lehnt der Server
// das Dokument ab (RC_PUSH) und der Adapter existiert nie — Klicktest P4
// T49/T52/T54. Jede Neuanlage traegt sie deshalb mit.
function adapterPflichtfelder(source) {
  return {
    url: String(source?.url || ''),
    adapter_kind: 'scrape_target',
    enabled: source?.enabled !== false,
  };
}

// Owner-Befund 18.09.2026: "warum gibt es kein Icon, um den Zugang zu testen?"
// Der Kolben prueft den Datenzugriff, der Pfeil startet eine Browser-Anmeldung
// — aber nichts beantwortete die Frage "ist der hinterlegte Zugang gueltig?".
// Dieser Knopf tut genau das: Liegt ein Wert im Secret Store? Und akzeptiert
// die Quelle ihn? Das Ergebnis steht danach in der Zeile.
async function pruefeZugang(sourceId) {
  const item = state.sources.find((entry) => entry.id === sourceId);
  if (!item) return;
  const name = String(item.credential_secret_name || '').trim();
  if (!name) {
    await showBusinessAlert(`Für „${item.label}“ ist kein Zugang vorgesehen.`);
    return;
  }
  // Im frisch geoeffneten Tab kann der erste Befehl noch ins Leere laufen
  // (Sync-Befund B1). Statt sofort aufzugeben: bis zu dreimal nachfassen.
  let stand = null;
  for (let versuch = 0; versuch < 3 && !stand; versuch += 1) {
    if (versuch) await new Promise((resolve) => { globalThis.setTimeout(resolve, 5000); });
    await ladeSecretKatalog();
    stand = secretStand(name);
  }
  if (!stand) {
    await showBusinessAlert('Der Stand des Zugangs ist unbekannt: Der CTOX Secret Store war in drei Versuchen nicht erreichbar. Bitte die Seite neu laden (⌘R) und erneut prüfen.');
    return;
  }
  if (!stand.vorhanden) {
    await showBusinessAlert(`Für „${item.label}“ liegt KEIN Zugang im CTOX Secret Store (${name}). Über das Zahnrad eintragen, dann erneut prüfen.`);
    return;
  }
  // Der eigentliche Beweis ist die Quelle selbst: derselbe Pruefbefehl wie beim
  // Kolben, nur dass hier der Zugang die Frage ist. Das Ergebnis landet in der
  // Zeile (Status und Zugangs-Chip) und braucht kein weiteres Fenster.
  await runAdapterCommand(sourceId, 'outbound.research_source.test');
}

// Der Server prueft einen Datenzugriff mit "Firma + Land". Ohne Angabe nahm er
// die Bezeichnung der Quelle als Firmennamen ("LinkedIn (Bright Data People
// Scraper)") — kein Register findet die, und jede Pruefung endete als
// "nicht gefunden", obwohl der Zugriff funktionierte (Befund 22.09.2026).
// Deshalb je Land eine real existierende, oeffentlich eingetragene Firma.
const PRUEF_FIRMEN = {
  DE: 'Carbosulf Chemische Werke GmbH',
  AT: 'voestalpine AG',
  CH: 'Lonza Group AG',
};
function pruefFirma(item) {
  const laender = Array.isArray(item.countries) ? item.countries : [];
  const country = ['DE', 'AT', 'CH'].find((land) => laender.includes(land)) || 'DE';
  return { company: PRUEF_FIRMEN[country], country };
}

async function runAdapterCommand(sourceId, commandType) {
  const item = state.sources.find((entry) => entry.id === sourceId);
  if (!item) return;
  try {
  const commandId = `cmd_leadgen_source_${crypto.randomUUID()}`;
  const adapter = {
    id: `adapter_leadgen_${item.target_key}`,
    source_id: item.id,
    label: item.label,
    url: item.url,
    input_driven: item.payload?.input_driven === true,
    start_url_source: String(item.payload?.start_url_source || ''),
    operator_instructions: String(item.payload?.instructions || ''),
    adapter_kind: 'scrape_target',
    target_key: item.target_key,
    countries: item.countries,
    field_keys: item.field_keys,
    enabled: item.enabled,
    requires_credential: item.requires_credential,
    credential_secret_name: item.credential_secret_name,
    // Ein Schluessel-Zugang (Bright Data) braucht keine Browser-Anmeldung: die
    // Pruefung lief sonst weiter ueber den alten Anmeldeweg und meldete
    // "authenticated capture returned no status", statt das API-Skript zu
    // fahren (Owner-Pruefdurchlauf 18.09.2026).
    auth_mode: item.requires_credential && !istSchluesselZugang(item.credential_secret_name) ? 'browser_session' : 'none',
    secret_value_in_payload: false,
  };
  const title = commandType.endsWith('.generate_adapter')
    ? `Datenzugriff einrichten: ${item.label}`
    : commandType.endsWith('.test')
      ? `Datenzugriff prüfen: ${item.label}`
      : `Anmeldung vorbereiten: ${item.label}`;
  const operation = adapterCommandOperation(commandType, item.target_key);
  // The payload stays typed (operation + target_key) so the server never has to
  // interpret prose. What the operator reads is a plain sentence naming exactly
  // that operation and target — deterministic underneath, legible on screen.
  const operationText = `${title} — ${operation.operation} · Ziel ${operation.target_key}`;
  // Die serverseitig erzeugten Sitzungsdaten kommen erst mit dem
  // Kommandoergebnis. Das Browserfenster selbst muss aber sofort sichtbar
  // werden, damit der Klick niemals wie ein toter Knopf wirkt.
  if (commandType.endsWith('.auth_assist')) {
    const openApp = state.ctx?.openApp || state.ctx?.openDesktopApp;
    if (typeof openApp === 'function') {
      try {
        await openApp.call(state.ctx, 'browser', { title: 'Browser', mode: 'maximized' });
      } catch (error) {
        // Immediate feedback is best effort. The durable auth request must
        // still be submitted so a transient shell/window error cannot turn
        // the source action into a dead button.
        console.warn('[outbound-lead-generation] Browser konnte nicht vorab geöffnet werden', error);
      }
    }
  }
  const command = {
    id: commandId, command_id: commandId, module: 'outbound', command_type: commandType,
    operation: operation.operation, target_key: operation.target_key,
    sync_queue_tasks: false,
    record_id: adapter.id, inbound_channel: 'business_os.outbound_lead_generation',
    payload: {
      ...operation,
      adapter_id: adapter.id, source_id: item.id, adapter,
      required_skills: ['outbound-lead-generation-research', 'universal-scraping', 'web-unlock'],
      scrape_contract: scrapeContract(item), secret_value_in_payload: false,
      ...(commandType.endsWith('.test') ? { test_input: pruefFirma(item) } : {}),
    },
    client_context: {
      source_module: 'outbound-lead-generation',
      source_id: item.id,
      actor: {
        id: state.ctx?.session?.user?.id || state.ctx?.session?.userId || '',
      },
    },
  };
  const requestedAt = Date.now();
  const requestedStatus = commandType.endsWith('.generate_adapter')
    ? 'generation_queued'
    : commandType.endsWith('.test') ? 'test_requested' : 'auth_requested';
  const requestedPatch = {
    ...adapterPflichtfelder(item),
    id: adapter.id,
    source_id: item.id,
    // Activation belongs to the source, but is duplicated on the adapter for
    // the shared Browser rail. Omitting this field let the v1 schema default
    // it to false, so enabled sources appeared as deactivated adapters.
    enabled: item.enabled !== false,
    status: requestedStatus,
    scrape_status: commandType.endsWith('.test') ? 'test_requested' : 'registration_requested',
    auth_status: commandType.endsWith('.auth_assist') ? 'browser_session_requested' : item.auth_status,
    last_command_id: commandId,
    last_task_id: '',
    last_error: '',
    payload: { operation, secret_value_in_payload: false },
    created_at_ms: requestedAt,
    updated_at_ms: requestedAt,
  };
  const requestedDoc = await state.collections.adapters.findOne(adapter.id).exec();
  if (requestedDoc) await requestedDoc.incrementalPatch(requestedPatch);
  else await state.collections.adapters.insert(requestedPatch);
  const requestedSourceDoc = await state.collections.sources.findOne(item.id).exec();
  await requestedSourceDoc?.incrementalPatch({
    adapter_status: requestedPatch.status,
    scrape_status: requestedPatch.scrape_status,
    auth_status: requestedPatch.auth_status,
    updated_at_ms: requestedAt,
  });
  state.adapters = [...state.adapters.filter((entry) => entry.id !== adapter.id), requestedPatch];
  state.sources = state.sources.map((entry) => entry.id === item.id
    ? { ...entry, adapter_status: requestedPatch.status, scrape_status: requestedPatch.scrape_status, auth_status: requestedPatch.auth_status, updated_at_ms: requestedAt }
    : entry);
  renderSourcePanel();
  let result;
  try {
    result = requireTrackedSubmission(await state.ctx.businessChat.submitTask({
      ...command,
      title,
      text: operationText,
      instruction: operationText,
      user_message: operationText,
      // Ein Rechercheauftrag darf das Chatfenster nicht aufreissen. Bei einem
      // Kampagnenlauf sprang es sonst je Lead erneut auf.
      open: false,
      control_command: true,
      client_context: {
        ...command.client_context,
        action: 'context-chat',
        response_channel: 'business_os_chat',
      },
    }), { allowTerminalCommand: true, allowControlCommand: true });
  } catch (error) {
    result = { status: 'failed', error: String(error?.message || error), command_id: commandId, task_id: '' };
    showBusinessAlert(result.error);
  }
  if (commandType.endsWith('.auth_assist') && result.status !== 'failed') {
    const authAssist = authAssistFromCommandResult(result);
    await openSourceAuthorization(item, authAssist);
  }
  const serverAdapter = result?.result?.adapter || result?.result?.outcome?.adapter || result?.adapter || {};
  const now = Date.now();
  const functionalStatus = String(
    serverAdapter.status || result?.result?.status || result?.result?.outcome?.status || '',
  ).trim();
  const functionalFailure = /^(failed|failure|error|blocked|rejected|unreachable|temporary_unreachable|unavailable|temporary_unavailable)$/i.test(functionalStatus);
  const status = functionalStatus || (result.task_id ? requestedStatus : result.status || 'failed');
  const doc = await state.collections.adapters.findOne(adapter.id).exec();
  const patch = {
    ...adapterPflichtfelder(item),
    id: adapter.id, source_id: item.id, status,
    enabled: item.enabled !== false,
    scrape_status: serverAdapter.scrape_status || (functionalFailure ? 'failed' : commandType.endsWith('.test') ? 'test_requested' : 'registration_requested'),
    auth_status: serverAdapter.auth_status || (commandType.endsWith('.auth_assist') ? 'browser_session_requested' : item.auth_status),
    last_command_id: result.command_id || commandId,
    last_task_id: result.task_id || '',
    last_error: result.error || result?.result?.error || serverAdapter.last_error || (functionalFailure ? functionalStatus : ''),
    payload: { result: sanitizeCommandResult(result), secret_value_in_payload: false },
    created_at_ms: doc?.created_at_ms || now, updated_at_ms: now,
  };
  if (doc) await doc.incrementalPatch(patch); else await state.collections.adapters.insert(patch);
  const sourceDoc = await state.collections.sources.findOne(item.id).exec();
  await sourceDoc?.incrementalPatch({ adapter_status: patch.status, scrape_status: patch.scrape_status, auth_status: patch.auth_status, updated_at_ms: now });
  state.adapters = [...state.adapters.filter((entry) => entry.id !== adapter.id), patch];
  state.sources = state.sources.map((entry) => entry.id === item.id
    ? { ...entry, adapter_status: patch.status, scrape_status: patch.scrape_status, auth_status: patch.auth_status, updated_at_ms: now }
    : entry);
  renderSourcePanel();
  } catch (error) {
    const message = String(error?.message || error || 'Der Datenzugriff konnte nicht gestartet werden.');
    console.error('[outbound-lead-generation] adapter command failed', error);
    showBusinessAlert(message);
  }
}

async function deleteSourceAdapter(sourceId) {
  const item = state.sources.find((entry) => entry.id === sourceId);
  const adapter = state.adapters.find((entry) => entry.source_id === sourceId);
  if (!item) return;
  // Ohne Adapterdatensatz warf der Abschluss einen TypeError (adapter.id) —
  // die Quelle war schon zurueckgesetzt, eine Meldung kam nie
  // (Klicktest-Befund P4 V4).
  if (!adapter) {
    await showBusinessAlert(`Für „${item.label}“ ist kein Datenzugriff eingerichtet.`);
    return;
  }
  const confirmed = await showBusinessConfirm(
    `Der eingerichtete Datenzugriff für „${item.label}“ wird gelöscht. Die Quelle selbst bleibt erhalten.`,
    {
      title: 'Datenzugriff löschen',
      confirmLabel: 'Datenzugriff löschen',
      kind: 'danger',
    },
  );
  if (!confirmed) return;
  const adapterDoc = adapter ? await state.collections.adapters.findOne(adapter.id).exec() : null;
  if (adapterDoc) await adapterDoc.remove();
  const now = Date.now();
  // Ein hinterlegter Zugang bleibt hinterlegt; nur der Datenzugriff geht
  // (vorher immer "required", Klicktest-Befund P4 V19).
  // Der Secret-Katalog entscheidet, sonst Quelle ODER Adapter: die Quelle
  // selbst stand oft noch auf "required", obwohl ein Zugang liegt (Nachtest F, T54).
  const katalog = secretStand(item.credential_secret_name);
  const zugangLiegt = katalog
    ? katalog.vorhanden
    : ['credential_available', 'authenticated'].includes(item.auth_status)
      || ['credential_available', 'authenticated'].includes(adapter.auth_status);
  const authStatus = !item.requires_credential
    ? 'not_required'
    : (zugangLiegt ? 'credential_available' : 'required');
  const sourceDoc = await state.collections.sources.findOne(item.id).exec();
  await sourceDoc?.incrementalPatch({
    adapter_status: 'draft',
    scrape_status: 'target_available',
    auth_status: authStatus,
    updated_at_ms: now,
  });
  state.adapters = state.adapters.filter((entry) => entry.id !== adapter.id);
  state.sources = state.sources.map((entry) => entry.id === item.id
    ? { ...entry, adapter_status: 'draft', scrape_status: 'target_available', auth_status: authStatus, updated_at_ms: now }
    : entry);
  renderSourcePanel();
}

function authAssistFromCommandResult(result) {
  const candidates = [
    result?.result?.auth_assist,
    result?.result?.outcome?.auth_assist,
    result?.payload?.outcome?.auth_assist,
    result?.payload?.auth_assist,
    result?.auth_assist,
  ];
  return candidates.find((candidate) => candidate && typeof candidate === 'object') || null;
}

function browserAuthAssistLaunchArgs(authAssist) {
  if (!authAssist || typeof authAssist !== 'object') return null;
  const sessionId = String(authAssist.session_id || '').trim();
  const targetUrl = String(authAssist.target_url || '').trim();
  if (!sessionId || !targetUrl) return null;
  return {
    session_id: sessionId,
    tab_id: String(authAssist.tab_id || `browser_tab_${sessionId}`).trim(),
    source_id: String(authAssist.source_id || '').trim(),
    purpose: 'web_stack_auth',
    target_url: targetUrl,
    allowed_domains: Array.isArray(authAssist.allowed_domains)
      ? authAssist.allowed_domains.map((entry) => String(entry || '').trim()).filter(Boolean)
      : [],
    capture_script: String(authAssist.capture_script || '').trim(),
    verify_selector: String(authAssist.verify_selector || '').trim(),
    secret_name: String(authAssist.required_secret_name || '').trim(),
    auth_assist_command_id: String(authAssist.command_id || '').trim(),
    auth_assist_task_id: String(authAssist.task_id || authAssist.execution_task_id || '').trim(),
    requesting_task_id: String(authAssist.requesting_task_id || '').trim(),
    instruction: String(authAssist.instruction || '').trim(),
    auth_assist_status: 'pending',
    profile_mode: 'persistent',
    secret_value_in_rxdb: false,
  };
}

async function openSourceAuthorization(item, authAssist) {
  const openApp = state.ctx?.openApp || state.ctx?.openDesktopApp;
  if (typeof openApp !== 'function') {
    showBusinessAlert('Die Browser-App konnte nicht geöffnet werden. Öffnen Sie „Browser“ und wählen Sie die angeforderte Anmeldung.');
    return;
  }
  const args = browserAuthAssistLaunchArgs(authAssist);
  if (!args) {
    showBusinessAlert('CTOX hat keine gültige Browser-Sitzung für diese Anmeldung zurückgegeben.');
    return;
  }
  await openApp.call(state.ctx, 'browser', {
    title: 'Browser',
    mode: 'maximized',
    args,
  });
}

async function reconcileAdapterCommands({ authoritative = false } = {}) {
  if (state.reconcilingAdapterCommands) return false;
  const trackedAdapters = state.adapters.filter((adapter) => {
    const commandId = String(adapter.last_command_id || '').trim();
    return commandId && adapter.payload?.reconciled_command_id !== commandId;
  });
  if (!trackedAdapters.length) return false;
  state.reconcilingAdapterCommands = true;
  let changed = false;
  try {
    const commandIds = [...new Set(trackedAdapters.map((adapter) => String(adapter.last_command_id).trim()))];
    const statuses = await loadCommandStatuses(commandIds, { authoritative, quelle: "adapter" });
    const commands = new Map(statuses.map((command) => {
      return [String(command.command_id || command.id || '').trim(), command];
    }));
    for (const adapter of trackedAdapters) {
      const command = commands.get(String(adapter.last_command_id || '').trim());
      const patch = adapterCommandRecordPatch(adapter, command);
      if (!patch) continue;
      const adapterDoc = await state.collections.adapters.findOne(adapter.id).exec();
      if (!adapterDoc) continue;
      await adapterDoc.incrementalPatch(patch);
      const sourceDoc = await state.collections.sources.findOne(adapter.source_id).exec();
      await sourceDoc?.incrementalPatch({
        adapter_status: patch.status,
        scrape_status: patch.scrape_status,
        auth_status: patch.auth_status,
        updated_at_ms: patch.updated_at_ms,
      });
      changed = true;
    }
  } finally {
    state.reconcilingAdapterCommands = false;
  }
  return changed;
}

function adapterCommandRecordPatch(adapter, command) {
  if (!command) return null;
  const commandId = String(command.command_id || command.id || '').trim();
  const status = String(
    command.terminal_status || command.status || command.result?.status || '',
  ).trim().toLowerCase();
  if (!['completed', 'failed', 'blocked', 'cancelled', 'canceled'].includes(status)) return null;
  if (adapter.payload?.reconciled_command_id === commandId) return null;
  const serverAdapter = command.result?.adapter
    || command.result?.outcome?.adapter
    || command.payload?.outcome?.adapter
    || {};
  const failed = ['failed', 'blocked', 'cancelled', 'canceled'].includes(status);
  const nextStatus = String(serverAdapter.status || (failed ? 'failed' : status));
  const nextScrapeStatus = String(
    serverAdapter.scrape_status || (failed ? 'failed' : adapter.scrape_status || 'test_requested'),
  );
  const nextAuthStatus = String(serverAdapter.auth_status || adapter.auth_status || 'not_required');
  const error = String(
    command.error_message
      || command.error
      || command.result?.error
      || serverAdapter.last_error
      || '',
  );
  return {
    status: nextStatus,
    scrape_status: nextScrapeStatus,
    auth_status: nextAuthStatus,
    last_error: error,
    payload: {
      ...(adapter.payload || {}),
      reconciled_command_id: commandId,
      reconciled_command_status: status,
      result: sanitizeCommandResult(command),
      secret_value_in_payload: false,
    },
    updated_at_ms: Date.now(),
  };
}

function sourceNeedsBrowserAuthorization(item, adapter) {
  if (isInternalResearchSource(item)) return false;
  if (item?.requires_credential) return true;
  const status = String(adapter?.status || item?.adapter_status || '').toLowerCase();
  const scrapeStatus = String(adapter?.scrape_status || item?.scrape_status || '').toLowerCase();
  const authStatus = String(adapter?.auth_status || item?.auth_status || '').toLowerCase();
  return status.includes('auth_required')
    || ['blocked', 'auth_required', 'browser_required'].includes(scrapeStatus)
    || ['required', 'auth_required', 'browser_session_requested'].includes(authStatus);
}

function rxdbIdSlug(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '');
}

function scrapeContract(item) {
  return {
    skill: 'outbound-lead-generation-research',
    skills: ['outbound-lead-generation-research', 'universal-scraping', 'web-unlock'],
    target_key: item.target_key,
    source_id: item.id,
    input_driven: item.payload?.input_driven === true,
    start_url_source: String(item.payload?.start_url_source || ''),
    operator_instructions: String(item.payload?.instructions || ''),
    output_schema: 'prospect.v1',
    min_independent_sources: 1,
    fallback: {
      allow_browser_assist: true,
      credential_ref: item.credential_secret_name ? `ctox-secret://credentials/${item.credential_secret_name}` : '',
    },
    unlock: { detect_access_challenge: true, record_signal: true, allow_access_control_bypass: false },
  };
}

async function createCampaign() {
  const name = String(await showBusinessPrompt(tr('campaignNamePrompt', 'Name der neuen Kampagne'), {
    title: tr('newCampaign', 'Neue Kampagne'),
    placeholder: tr('campaignName', 'Kampagnenname'),
    confirmLabel: tr('continue', 'Weiter'),
  }) || '').trim();
  if (!name) return;
  // Wie beim Umbenennen: ein vorhandener Name wird nicht still zusammengelegt
  // (Klicktest-Befund P1 #12).
  const vorhanden = campaignRows().some((campaign) => campaign.name.localeCompare(name, 'de', { sensitivity: 'base' }) === 0);
  if (vorhanden) {
    const weiter = await showBusinessConfirm(
      `Die Kampagne „${name}“ gibt es schon. Der Import ergänzt sie um die neuen Leads.`,
      { title: 'Kampagne vorhanden', confirmLabel: 'In bestehende Kampagne importieren' },
    );
    if (!weiter) return;
  }
  await openImporter(name);
}

// Mehrfachaktionen in kleinen Paketen: 133 gleichzeitige findOne sprengten das
// Abfragebudget des Browsers (QUERY_QUEUE_LIMIT beim Verschieben, 28.09.2026).
const LEAD_PAKET = 10;
async function inPaketen(items, fn, paket = LEAD_PAKET) {
  for (let start = 0; start < items.length; start += paket) {
    await Promise.all(items.slice(start, start + paket).map(fn));
  }
}

async function leadDokumenteInPaketen(leads) {
  const karte = await ladeLeadDokumente(leads.map((lead) => lead.id));
  const fehlend = leads.filter((lead) => !karte.has(lead.id));
  await inPaketen(fehlend, async (lead) => {
    const doc = await state.collections.leads.findOne(lead.id).exec();
    if (doc) karte.set(lead.id, doc);
  });
  return leads.map((lead) => karte.get(lead.id)).filter(Boolean);
}

// Leads liessen sich bisher weder loeschen noch die Kampagne wechseln. Damit
// blieb eine Firma, die in der falschen Kampagne landete, dort fuer immer.
async function verschiebeAuswahlInKampagne() {
  const leads = state.leads.filter((lead) => state.selectedLeadIds.has(lead.id));
  if (!leads.length) return;
  const vorhandene = campaignRows().map((campaign) => campaign.name).filter(Boolean);
  // Keine fremde Kampagne vorbelegen (Klicktest P2 V5): leer lassen.
  const vorschlag = '';
  const ziel = String(await showBusinessPrompt(
    `${leads.length} Lead${leads.length === 1 ? '' : 's'} in welche Kampagne verschieben?${vorhandene.length ? ` Vorhanden: ${vorhandene.join(' · ')}` : ''}`,
    { title: 'In Kampagne verschieben', defaultValue: vorschlag, confirmLabel: 'Verschieben' },
  ) || '').trim();
  if (!ziel) return;
  state.campaignMutationMessage = leads.length === 1 ? '1 Lead wird verschoben …' : `${leads.length} Leads werden verschoben …`;
  renderCampaigns();
  try {
    const docs = await leadDokumenteInPaketen(leads);
    // Nur tatsaechlich verschobene Leads zaehlen; vorher meldete ein
    // Verschieben in dieselbe Kampagne "N Leads verschoben" (Klicktest P2 V4).
    let verschoben = 0;
    await inPaketen(docs, async (doc) => {
      const lead = doc.toJSON?.() || doc;
      if (String(lead.campaign || '').trim() === ziel) return;
      verschoben += 1;
      await doc.incrementalPatch({
        campaign: ziel,
        payload: {
          ...(lead.payload || {}),
          previous_campaign: String(lead.campaign || ''),
          campaign_moved_at_ms: Date.now(),
        },
        updated_at_ms: Date.now(),
      });
    });
    await flushReplicatedCollection(
      'outbound_lead_generation_leads',
      docs.map((doc) => doc.toJSON?.() || doc),
    );
    state.selectedCampaign = ziel;
    state.selectedLeadIds = new Set();
    await reload();
    zeigeHinweis(verschoben
      ? `${verschoben} Lead${verschoben === 1 ? '' : 's'} nach „${ziel}“ verschoben.`
      : `Die Leads sind bereits in „${ziel}“.`);
  } catch (error) {
    await showBusinessAlert(`Verschieben fehlgeschlagen: ${error?.message || error}`);
  } finally {
    state.campaignMutationMessage = '';
    render();
  }
}

async function renameCampaign(currentName) {
  const current = String(currentName || '').trim();
  if (!current) return;
  const next = String(await showBusinessPrompt('Neuer Name der Kampagne', {
    title: 'Kampagne umbenennen',
    defaultValue: current,
    confirmLabel: 'Umbenennen',
  }) || '').trim();
  if (!next || next === current) return;
  if (campaignRows().some((campaign) => campaign.name === next)) {
    await showBusinessAlert('Eine Kampagne mit diesem Namen existiert bereits.');
    return;
  }
  state.campaignMutationMessage = `Kampagne „${current}“ wird gespeichert …`;
  renderCampaigns();
  try {
  const leads = campaignLeads(current);
  const leadDocs = await leadDokumenteInPaketen(leads);
  await inPaketen(leadDocs, async (doc) => {
    const lead = doc.toJSON?.() || doc;
    // Mitglieder (Heimat in einer anderen Kampagne) behalten ihre Heimat; nur
    // der Eintrag in weitere_kampagnen wird umbenannt.
    if (heimatKampagne(lead) !== current) {
      const weitere = (lead.payload?.weitere_kampagnen || []).map((name) => (name === current ? next : name));
      await doc.incrementalPatch({ payload: { ...(lead.payload || {}), weitere_kampagnen: weitere }, updated_at_ms: Date.now() });
      return;
    }
    await doc.incrementalPatch({
      campaign: next,
      payload: {
        ...(lead.payload || {}),
        previous_campaign: current,
        campaign_renamed_at_ms: Date.now(),
      },
      updated_at_ms: Date.now(),
    });
  });
  await flushReplicatedCollection(
    'outbound_lead_generation_leads',
    leadDocs.map((doc) => doc.toJSON?.() || doc),
  );
  // Import records are immutable audit evidence: their title records the name
  // under which the import was confirmed. Campaign identity and membership
  // live exclusively on the lead records (see campaignRows/campaignLeads).
  // Mutating both collections made rename a non-atomic two-phase operation:
  // the authoritative lead write could succeed while a delayed imports bridge
  // left the UI spinning forever. One acknowledged lead batch is the complete
  // campaign mutation and preserves the historical import trail.
  const run = state.campaignRuns.get(current);
  if (run) {
    state.campaignRuns.delete(current);
    state.campaignRuns.set(next, run);
  }
  state.selectedCampaign = next;
  state.leads = state.leads.map((lead) => (
    String(lead.campaign || '').trim() === current ? { ...lead, campaign: next } : lead
  ));
  state.campaignMutationMessage = '';
  render();
  scheduleCollectionReload();
  } catch (error) {
    state.campaignMutationMessage = '';
    render();
    await withTimeout(
      reload(),
      'Der aktuelle Kampagnenstand konnte nicht neu geladen werden.',
      5_000,
    ).catch(() => {});
    render();
    await showBusinessAlert(`Die Kampagne konnte nicht vollständig gespeichert werden: ${error?.message || error}`);
  }
}

async function deleteCampaign(campaignName) {
  const campaign = String(campaignName || '').trim();
  if (!campaign) return;
  // Geloescht werden nur Leads mit dieser Heimatkampagne. Firmen, die hier nur
  // zusaetzlich Mitglied sind, verlieren die Mitgliedschaft und bleiben in
  // ihrer Heimatkampagne (sonst loeschte "Welle 1" die Leads von "Welle 2").
  const mitglieder = campaignLeads(campaign).filter((lead) => heimatKampagne(lead) !== campaign);
  const leads = heimatLeads(campaign);
  const running = leads.filter((lead) => ['queued', 'running'].includes(lead.research_status));
  // A lead can keep `running` long after its task is gone — the status is a
  // durable field, not a heartbeat. Refusing on it alone made campaigns
  // permanently undeletable. Only a run that is still reporting blocks; a
  // stale one is named and left to the operator to decide.
  const live = running.filter((lead) => Date.now() - Number(
    lead.research_updated_at_ms || lead.payload?.research_started_at_ms || lead.research_started_at_ms || lead.updated_at_ms || 0,
  ) < RESEARCH_HEARTBEAT_STALE_MS);
  if (live.length > 0) {
    await showBusinessAlert(`Die Kampagne kann nicht gelöscht werden, solange ${live.length === 1 ? 'eine Recherche läuft' : `${live.length} Recherchen laufen`}.`);
    return;
  }
  const staleNote = running.length
    ? ` ${running.length} Lauf${running.length === 1 ? '' : 'e'} steht noch auf „läuft“, meldet sich aber nicht mehr — der wird mitgelöscht.`
    : '';
  const confirmed = await showBusinessConfirm(
    `Die Kampagne „${campaign}“ und ${leads.length} Lead${leads.length === 1 ? '' : 's'} werden dauerhaft gelöscht.${staleNote}`
      + (mitglieder.length ? ` ${mitglieder.length} weitere Firma/Firmen bleiben in ihrer eigenen Kampagne und verlieren nur die Zuordnung zu dieser.` : ''),
    {
      title: 'Kampagne löschen',
      confirmLabel: 'Endgültig löschen',
      // Owner-Befund 03.09.2026: "man kann immer noch keine kampangen
      // loeschen!". Der Weg funktionierte technisch, verlangte aber, den
      // vollstaendigen Namen exakt abzutippen - bei "Sellify: Automatiktueren
      // & Drehtueren D - Welle 3 - 04.09.2025" und einer Seitenleiste, die den
      // Namen abschneidet, ist das praktisch unmoeglich. Der Dialog nennt
      // Kampagne und Lead-Anzahl und hat einen eigenen roten Knopf; das ist
      // die gleiche Schwelle wie bei jeder anderen zerstoerenden Aktion hier.
      kind: 'danger',
    },
  );
  if (!confirmed) return;
  for (const lead of mitglieder) {
    try {
      const doc = await state.collections.leads.findOne(lead.id).exec();
      const current = doc?.toJSON?.() || doc;
      if (!doc) continue;
      const weitere = (current.payload?.weitere_kampagnen || []).filter((name) => name !== campaign);
      await doc.incrementalPatch({ payload: { ...(current.payload || {}), weitere_kampagnen: weitere }, updated_at_ms: Date.now() });
      lead.payload = { ...(lead.payload || {}), weitere_kampagnen: weitere };
    } catch (fehler) {
      console.warn('[olg] Mitgliedschaft nicht entfernt', lead.id, fehler);
    }
  }
  // Owner-Befund 18.09.2026 ("warum kann ich keine Kampagnen loeschen?"):
  // Nach dem Bestaetigen blieb die Kampagne im Tab des Owners 18,7 s stehen —
  // ohne Wartezustand, ohne Meldung. Gemessen in seiner Instanz: EIN
  // Lead-Dokument lesen 0,6 s, alle 54 lesen 7,2 s; das abschliessende
  // vollstaendige Nachladen kostete den Rest. Wer in dieser Zeit erneut
  // klickt, trifft die inzwischen nachgerueckte NACHBARKAMPAGNE.
  // Deshalb: Anzeige sofort aktualisieren, danach loeschen, Nachladen im
  // Hintergrund, und jeder Fehlschlag wird benannt.
  if (!(state.kampagnenLoeschen instanceof Set)) state.kampagnenLoeschen = new Set();
  if (state.kampagnenLoeschen.has(campaign)) return;
  state.kampagnenLoeschen.add(campaign);
  const leadIds = new Set(leads.map((lead) => lead.id));
  const leadImportIds = new Set(leads.flatMap((lead) => [
    String(lead.import_id || '').trim(),
    String(lead.payload?.previous_import_id || '').trim(),
  ]).filter(Boolean));
  const importIds = new Set((state.imports || [])
    .filter((item) => leadImportIds.has(String(item.id || '').trim()) || String(item.title || '').trim() === campaign)
    .map((item) => item.id));
  const vorherLeads = state.leads;
  const vorherImports = state.imports;
  const vorherKampagne = state.selectedCampaign;
  state.leads = state.leads.filter((lead) => !leadIds.has(lead.id));
  state.imports = (state.imports || []).filter((item) => !importIds.has(item.id));
  state.campaignRuns.delete(campaign);
  state.selectedLeadIds.clear();
  state.selectedLeadId = '';
  if (state.selectedCampaign === campaign) state.selectedCampaign = '';
  render();
  try {
    const leadDocs = (await Promise.all(
      [...leadIds].map((id) => state.collections.leads.findOne(id).exec()),
    )).filter(Boolean);
    const importDocs = (await Promise.all(
      [...importIds].map((id) => state.collections.imports.findOne(id).exec()),
    )).filter(Boolean);
    await Promise.all(leadDocs.map((doc) => doc.remove()));
    await Promise.all(importDocs.map((doc) => doc.remove()));
  } catch (error) {
    state.leads = vorherLeads;
    state.imports = vorherImports;
    state.selectedCampaign = vorherKampagne;
    render();
    await showBusinessAlert(`Die Kampagne „${campaign}“ wurde NICHT gelöscht: ${zugangsFehlerText(String(error?.message || error))}`);
    return;
  } finally {
    state.kampagnenLoeschen.delete(campaign);
  }
  // Der Abgleich mit dem Server laeuft im Hintergrund; die Anzeige steht schon.
  void reload();
}

const NEUER_ANSPRECHPARTNER = '__neuer_ansprechpartner__';

function openLeadEditor(id) {
  const lead = state.leads.find((entry) => entry.id === id);
  if (!lead) return;
  state.leadEditorId = lead.id;
  // Der Editor zeigt dieselben Werte wie die Feldansicht. Bis 10.09.2026 las
  // er nur die Importschluessel (address_line, postal_code, ...); bei
  // recherchierten Leads stand alles leer, und ein Speichern - etwa nur um den
  // Verantwortlichen einzutragen - loeschte die Belege von sechs Firmenfeldern.
  state.leadDraft = {
    name: lead.name || '',
    website: lead.website || researchFieldValue(lead, 'firma_domain') || '',
    address_line: researchFieldValue(lead, 'firma_anschrift') || '',
    postal_code: researchFieldValue(lead, 'firma_plz') || '',
    city: researchFieldValue(lead, 'firma_ort') || '',
    country: lead.country || researchFieldValue(lead, 'firma_land') || 'DE',
    email: researchFieldValue(lead, 'firma_email') || '',
    phone: researchFieldValue(lead, 'firma_telefon') || '',
    campaign: lead.campaign || '',
    ...Object.fromEntries(GOVERNANCE_FIELDS.map((key) => [key, researchFieldValue(lead, key)])),
    ...Object.fromEntries(EXTRA_COMPANY_EDIT_FIELDS.map((key) => [key, researchFieldValue(lead, key)])),
  };
  // Personenfelder gehoeren zu der Person, die der Personenreiter gerade zeigt.
  const aktivePerson = contactTabLead(lead).contacts?.[0] || null;
  // Ohne Person gab es keinen Weg, einen Ansprechpartner einzutragen: "eintragen"
  // an einem Personenfeld oeffnete einen Editor ohne Personenfelder
  // (Klicktest P3 REV-02b). Jetzt: Abschnitt fuer einen neuen Ansprechpartner.
  state.leadEditorContactId = aktivePerson?.id || NEUER_ANSPRECHPARTNER;
  state.leadEditorContactName = aktivePerson ? personDisplayName(aktivePerson) : '';
  for (const key of PERSON_EDIT_FIELDS) {
    state.leadDraft[`person:${key}`] = aktivePerson
      ? String(firstValue(aktivePerson, RESEARCH_FIELD_VALUE_KEYS[key] || [key]) || '').trim()
      : '';
  }
  state.leadDraftOriginal = { ...state.leadDraft };
  state.leadEditorOpen = true;
  renderLeadEditor();
  const fokus = String(state.leadEditorFocusField || '').trim();
  state.leadEditorFocusField = '';
  if (fokus) {
    const editorKey = fokus.startsWith('person_') ? `person:${fokus}` : (EDITOR_KEY_FOR_FIELD[fokus] || fokus);
    queueMicrotask(() => {
      const input = state.ctx.host.querySelector(`[data-lead-edit-field="${editorKey}"]`);
      if (!input) return;
      input.scrollIntoView({ block: 'center' });
      input.focus();
    });
  }
}

function closeLeadEditor() {
  state.leadEditorOpen = false;
  state.leadEditorId = '';
  state.leadEditorContactId = '';
  state.leadDraft = null;
  state.leadDraftOriginal = null;
  renderLeadEditor();
}

async function saveLeadEditor() {
  const lead = state.leads.find((entry) => entry.id === state.leadEditorId);
  const draft = state.leadDraft;
  if (!lead || !draft) return;
  const name = String(draft.name || '').trim();
  const campaign = String(draft.campaign || '').trim();
  if (!name || !campaign) {
    await showBusinessAlert('Organisation und Kampagne sind Pflichtfelder.');
    return;
  }
  // Eine Website ohne https:// loeschte die Domain des Leads (domainFromUrl
  // braucht ein Schema); der Import ergaenzt es, der Editor tat es nicht
  // (Klicktest-Befund P3 #11).
  // Verglichen wird die ROHE Eingabe mit dem Stand beim Oeffnen; das Schema
  // kommt nur an den geschriebenen Wert. Sonst galt jede Domain ohne Schema als
  // geaendert und ihre Belege wurden bei jedem Speichern ersetzt (Review 1).
  const websiteEingabe = String(draft.website || '').trim();
  const website = websiteEingabe;
  const websiteGeschrieben = websiteEingabe && !/^[a-z][a-z0-9+.-]*:\/\//i.test(websiteEingabe)
    ? `https://${websiteEingabe}`
    : websiteEingabe;
  const editFieldMap = {
    name: 'firma_name', website: 'firma_domain', address_line: 'firma_anschrift',
    postal_code: 'firma_plz', city: 'firma_ort', country: 'firma_land',
    email: 'firma_email', phone: 'firma_telefon',
    ...Object.fromEntries(GOVERNANCE_FIELDS.filter((key) => key !== 'firma_land').map((key) => [key, key])),
    ...Object.fromEntries(EXTRA_COMPANY_EDIT_FIELDS.map((key) => [key, key])),
  };
  // Das Land wird gespeichert, wie es eingegeben wurde. normalizedResearchCountry
  // bildet alles ausser AT/CH auf DE ab und gehoert an die Recherche-Uebergabe,
  // nicht in den Datensatz: vorher wurde aus einem importierten "FR" bei JEDEM
  // Speichern still "DE" (Klicktest-Befund P3 V-1, 11.09.2026).
  const draftCountry = String(draft.country || '').trim().toUpperCase();
  const nextEditorValues = { ...draft, name, website, country: draftCountry };
  // Geaendert ist, was der Nutzer im Dialog geaendert hat - verglichen mit dem
  // Stand beim Oeffnen. Der Vergleich gegen den Rechercheschluessel machte aus
  // "Buck Chemie GmbH" (Leadname) gegen "Buck-Chemie GmbH" (Recherche) eine
  // Aenderung und warf die Namensbelege weg (10.09.2026).
  const original = state.leadDraftOriginal || {};
  const originalValue = (editorKey) => (editorKey === 'country'
    ? String(original.country || '').trim().toUpperCase()
    : String(original[editorKey] || '').trim());
  const changedEntries = Object.entries(editFieldMap)
    .filter(([editorKey]) => String(nextEditorValues[editorKey] || '').trim() !== originalValue(editorKey));
  const changedFieldKeys = changedEntries.map(([, fieldKey]) => fieldKey);
  // Nur was sich wirklich geaendert hat, wird geschrieben - unter dem
  // Rechercheschluessel UND dem Importschluessel, damit Anzeige, Uebergabe
  // und Editor denselben Wert lesen.
  const legacyKeys = { address_line: 'address_line', postal_code: 'postal_code', city: 'city', email: 'email', phone: 'phone' };
  const nextData = { ...(lead.data || {}) };
  for (const [editorKey, fieldKey] of changedEntries) {
    const value = String(nextEditorValues[editorKey] || '').trim();
    nextData[fieldKey] = value;
    if (legacyKeys[editorKey]) nextData[legacyKeys[editorKey]] = value;
    // Weitere Schluessel, unter denen derselbe Wert schon steht (revenue_mio
    // fuer den Umsatz liest die Uebergabe), ziehen mit.
    for (const alias of RESEARCH_FIELD_VALUE_KEYS[fieldKey] || []) {
      if (alias !== fieldKey && Object.prototype.hasOwnProperty.call(nextData, alias)) nextData[alias] = value;
    }
  }
  // Personenfelder der im Personenreiter gezeigten Person.
  const personChanges = [];
  let nextContacts = lead.contacts || [];
  // Neuer Kontakt aus Editorfeldern mit Praefix (person: ohne Person, neu:
  // fuer einen weiteren) — vorher liess sich nach der ersten Person keine
  // zweite anlegen (Nachtest P3 N-1).
  const legeKontaktAn = (praefix) => {
    const werte = Object.fromEntries(PERSON_EDIT_FIELDS
      .map((key) => [key, String(draft[`${praefix}${key}`] || '').trim()])
      .filter(([, value]) => value));
    if (!Object.keys(werte).length) return;
    const id = `contact_manual_${crypto.randomUUID()}`;
    const neu = { id, person_key: id, source: 'manual', manually_added_at_ms: Date.now(), ...werte };
    if (werte.person_vorname) neu.first_name = werte.person_vorname;
    if (werte.person_nachname) neu.last_name = werte.person_nachname;
    if (werte.person_email) neu.email = werte.person_email;
    neu.name = [werte.person_vorname, werte.person_nachname].filter(Boolean).join(' ');
    nextContacts = [...nextContacts, neu];
    for (const [key, value] of Object.entries(werte)) personChanges.push({ key, value, personKey: id });
  };
  if (state.leadEditorContactId === NEUER_ANSPRECHPARTNER) {
    legeKontaktAn('person:');
  } else if (state.leadEditorContactId) {
    nextContacts = (lead.contacts || []).map((contact) => {
      if (contact.id !== state.leadEditorContactId) return contact;
      const updated = { ...contact };
      for (const key of PERSON_EDIT_FIELDS) {
        const value = String(draft[`person:${key}`] || '').trim();
        const before = String(original[`person:${key}`] || '').trim();
        if (value === before) continue;
        updated[key] = value;
        for (const alias of RESEARCH_FIELD_VALUE_KEYS[key] || []) {
          if (alias !== key && Object.prototype.hasOwnProperty.call(updated, alias)) updated[alias] = value;
        }
        personChanges.push({ key, value, personKey: String(contact.person_key || contact.sellify_person_id || contact.id || '') });
      }
      // Anzeigename folgt Vor-/Nachname; sonst blieb der alte Name an Chip,
      // Ueberschrift und Empfaengerliste stehen (Nachtest P3 N-2).
      const vorname = String(updated.person_vorname || updated.first_name || '').trim();
      const nachname = String(updated.person_nachname || updated.last_name || '').trim();
      const namensAenderung = ['person_vorname', 'person_nachname']
        .some((key) => String(draft[`person:${key}`] || '').trim() !== String(original[`person:${key}`] || '').trim());
      if (namensAenderung && (vorname || nachname)) {
        if (Object.prototype.hasOwnProperty.call(updated, 'first_name') || vorname) updated.first_name = vorname;
        if (Object.prototype.hasOwnProperty.call(updated, 'last_name') || nachname) updated.last_name = nachname;
        updated.name = [vorname, nachname].filter(Boolean).join(' ');
        if (Object.prototype.hasOwnProperty.call(updated, 'display_name')) updated.display_name = updated.name;
      }
      return updated;
    });
    legeKontaktAn('neu:');
  }
  const nextCity = changedFieldKeys.includes('firma_ort') ? String(draft.city || '').trim() : (lead.city || '');
  const personChangeKeys = new Set(personChanges.map((change) => `${change.key}|${change.personKey}`));
  const retainedEvidence = (lead.evidence || []).filter((entry) => {
    const fieldKey = entry?.field_key || entry?.field;
    if (changedFieldKeys.includes(fieldKey)) return false;
    const bound = String(entry?.person_key || entry?.person_id || '').trim();
    return !(bound && personChangeKeys.has(`${fieldKey}|${bound}`));
  });
  const manualEvidence = changedFieldKeys
    .map((fieldKey) => ({
      field_key: fieldKey,
      value: String(nextData[fieldKey] ?? researchFieldValue({ ...lead, name, website, country: nextEditorValues.country, data: nextData }, fieldKey) ?? '').trim(),
      confidence: 'operator',
      source_id: 'operator',
      source_url: '',
      tier: 'O',
      via: 'manual-edit',
      label: tr('operatorEdited', 'Vom Nutzer geändert'),
    }))
    .filter((entry) => entry.value);
  for (const change of personChanges) {
    if (!change.value) continue;
    manualEvidence.push({
      field_key: change.key,
      value: change.value,
      person_key: change.personKey,
      confidence: 'operator',
      source_id: 'operator',
      source_url: '',
      tier: 'O',
      via: 'manual-edit',
      label: tr('operatorEdited', 'Vom Nutzer geändert'),
    });
  }
  // Das Fenster schliesst sofort; gespeichert wird im Hintergrund mit sichtbarer
  // Rueckmeldung. Vorher blieb es offen, bis Datenbankabfrage und Schreiben
  // durch den Abgleich waren - unter Last sah "Speichern" tot aus (25.09.2026).
  state.selectedCampaign = campaign;
  closeLeadEditor();
  zeigeHinweis(`„${name}“ wird gespeichert …`, 0);
  render();
  const speichern = patchLead(lead.id, {
    ...(personChanges.length ? { contacts: nextContacts } : {}),
    name,
    campaign,
    website: changedFieldKeys.includes('firma_domain') ? websiteGeschrieben : (lead.website || ''),
    domain: changedFieldKeys.includes('firma_domain') ? (websiteGeschrieben ? domainFromUrl(websiteGeschrieben) : '') : (lead.domain || ''),
    city: nextCity,
    country: draftCountry || lead.country || 'DE',
    data: nextData,
    evidence: deduplicateEvidence([...retainedEvidence, ...manualEvidence]),
    payload: {
      ...(lead.payload || {}),
      manually_edited_at_ms: Date.now(),
      manually_edited_field_keys: [...new Set([...(lead.payload?.manually_edited_field_keys || []), ...changedFieldKeys])],
      conflicting_field_keys: (lead.payload?.conflicting_field_keys || []).filter((key) => !changedFieldKeys.includes(key)),
    },
  });
  try {
    await speichern;
    zeigeHinweis(`„${name}“ gespeichert.`, 6000);
  } catch (error) {
    zeigeHinweis(`Speichern von „${name}“ fehlgeschlagen.`, 30000);
    await showBusinessAlert(`Die Änderungen an „${name}“ konnten nicht gespeichert werden: ${String(error?.message || error)}`);
  }
  await reload().catch(() => {});
  render();
}

// Der gepflegte Rechercheablauf gehoert dorthin, wo der CTOX-Agent ihn ohne
// den urspruenglichen Auftrag wiederfindet: in den Scraping-Bereich der CTOX-
// SQLite als Ziel `outbound-lead-generation-policy` (target_kind `app-policy`).
// Der Agent liest ihn mit `ctox scrape show-target --target-key <app>-policy`.
// So bleiben lange Instruktionen aus dem Chat-Prompt heraus und stehen auch
// einer Fortsetzung nach Anmeldung oder einer Reparaturaufgabe zur Verfuegung.
async function publishResearchPolicyToScrapeStore(record) {
  const dispatch = state.ctx?.commandBus?.dispatch;
  if (typeof dispatch !== 'function') return { ok: false, reason: 'command-bus-unavailable' };
  const commandId = `cmd_leadgen_policy_${crypto.randomUUID?.() || Date.now()}`;
  try {
    const result = await dispatch({
      id: commandId,
      command_id: commandId,
      module: 'outbound-lead-generation',
      command_type: 'outbound.research_policy.publish',
      record_id: 'policy:outbound-lead-generation',
      status: 'pending_sync',
      payload: {
        app: 'outbound-lead-generation',
        skill: 'outbound-lead-generation-research',
        research_instructions: hebeBelegregelAn(String(record?.instructions || '').trim()),
        followup_instructions: String(record?.followup_instructions || '').trim(),
        fields: normalizeResearchFieldKeys(record?.field_keys || RESEARCH_FIELDS),
        person_priorities: [...PERSON_RESEARCH_PRIORITIES],
        min_independent_sources: 1,
        source_policy: enabledSourcePolicy(),
        policy_version: Number(record?.version_number ?? record?.version ?? 0) || 0,
        updated_at_ms: Number(record?.updated_at_ms || Date.now()),
      },
      client_context: { source: 'outbound-lead-generation.research-policy-save' },
    // Kontrollbefehl ohne Aufgabe: ohne sync_queue_tasks:false und Frist
    // konnte das Veroeffentlichen haengen wie frueher das Zugang-Speichern
    // (Klicktest-Befund P4 V6).
    }, { until: 'accepted', timeoutMs: 45_000, sync_queue_tasks: false });
    console.info('[olg-trace] policy published to scrape store', result?.status || 'dispatched');
    return { ok: true };
  } catch (error) {
    console.warn('[olg-trace] policy publish failed:', error?.message || error);
    return { ok: false, reason: String(error?.message || error) };
  }
}

async function saveResearchPolicy() {
  const instructions = String(state.researchPolicyDraft || '').trim();
  const fieldKeys = [...RESEARCH_FIELDS];
  console.info('[olg-trace] savePolicy start', { len: instructions.length });
  if (!instructions) {
    showBusinessAlert(tr('policyRequired', 'Der Rechercheablauf darf nicht leer sein.'));
    return;
  }
  const existingDoc = await mitKanalHeilung(() => state.collections.researchPolicies.findOne(RESEARCH_POLICY_ID).exec(), 'save-policy-read');
  console.info('[olg-trace] savePolicy findOne ok', { existiert: Boolean(existingDoc) });
  const existing = existingDoc?.toJSON?.() || existingDoc;
  const followupInstructions = String(state.researchPolicyFollowupDraft || '').trim();
  // Unveraendert = nichts schreiben, nichts veroeffentlichen; vorher stieg die
  // Version bei jedem Speichern (Klicktest P4 T22).
  if (existing
    && researchPolicyInstructions(existing) === instructions
    && followupResearchPolicyInstructions(existing) === followupInstructions) {
    // Nicht neu speichern (Version bleibt), aber erneut veroeffentlichen: ein
    // frueher gescheitertes Veroeffentlichen haette sonst nie nachgeholt
    // werden koennen (Review 7).
    const erneut = await publishResearchPolicyToScrapeStore(existing);
    showBusinessAlert(erneut?.ok
      ? 'Keine Änderung – der Rechercheablauf ist so gespeichert und wurde erneut an CTOX übergeben.'
      : `Keine Änderung am Rechercheablauf; die Übergabe an CTOX ist fehlgeschlagen: ${erneut?.reason || 'unbekannt'}`);
    return;
  }
  const record = researchPolicyRecord(
    existing,
    instructions,
    Date.now(),
    fieldKeys,
    followupInstructions,
  );
  await mitKanalHeilung(async () => {
    const doc = await state.collections.researchPolicies.findOne(RESEARCH_POLICY_ID).exec();
    if (doc) await doc.incrementalPatch(record);
    else await state.collections.researchPolicies.insert(record);
  }, 'save-policy-write');
  console.info('[olg-trace] savePolicy write ok');
  state.researchPolicy = instructions;
  state.researchPolicyDraft = instructions;
  state.researchPolicyFollowup = followupInstructions;
  state.researchPolicyFollowupDraft = followupInstructions;
  state.researchFieldKeys = [...fieldKeys];
  state.researchFieldKeysDraft = [...fieldKeys];
  state.researchPolicyRecord = record;
  await publishResearchPolicyToScrapeStore(record);
  const aktiveQuellen = state.sources.filter((source) => !isInternalResearchSource(source) && source.enabled !== false);
  let adapterRelevant = true;
  try {
    const erwartet = await adapterConfigurationDigest(record, aktiveQuellen);
    adapterRelevant = String(existing?.configuration_digest || '').trim() !== erwartet;
  } catch { adapterRelevant = true; }
  if (!adapterRelevant) {
    showBusinessAlert('Rechercheablauf gespeichert. Die Adapter sind davon nicht betroffen.');
    return;
  }
  try {
    const submission = await queueAdapterReconciliation('research_policy_changed', record);
    showBusinessAlert(submission?.deferred
      ? 'Rechercheablauf gespeichert. Er gilt ab dem nächsten Recherchestart.'
      : submission?.deduplicated
        ? 'Rechercheablauf gespeichert. Der passende Adapter-Stand ist bereits aktiv.'
        : 'Rechercheablauf gespeichert. CTOX gleicht jetzt alle Adapter in einem Vorgang ab.');
  } catch (error) {
    showBusinessAlert(`Der Rechercheablauf wurde gespeichert, aber der Adapter-Abgleich konnte nicht gestartet werden: ${String(error?.message || error)}`);
  }
}

async function adapterConfigurationDigest(policy, sources) {
  if (!globalThis.crypto?.subtle || typeof TextEncoder !== 'function') {
    throw new Error('Der sichere Konfigurationsabgleich ist in diesem Browser nicht verfügbar.');
  }
  // Ein Adapter liest Felder aus einer Quelle; der Wortlaut der
  // Rechercheanweisung aendert daran nichts. Frueher ging der Text in den
  // Fingerabdruck ein, also loeste jede Textkorrektur einen neuen
  // Adapter-Abgleich fuer alle 20 Quellen aus — einen vollen Worker-Durchlauf,
  // der am 10.09.2026 zudem an der Sandbox scheiterte und laufende Recherchen
  // in der Warteschlange hinter sich warten liess. `policy` bleibt Parameter,
  // damit die Aufrufer unveraendert bleiben.
  void policy;
  const canonical = JSON.stringify({
    schema: 'ctox.outbound.adapter_configuration.v2',
    field_keys: activeResearchFields(),
    sources: sources.map((source) => ({
      id: source.id,
      url: source.url,
      target_key: source.target_key,
      enabled: source.enabled !== false,
      requires_credential: Boolean(source.requires_credential),
      credential_secret_name: String(source.credential_secret_name || ''),
      field_keys: Array.isArray(source.field_keys) ? source.field_keys : [],
    })).sort((left, right) => left.id.localeCompare(right.id)),
  });
  const digest = await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(canonical));
  return `sha256:${[...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('')}`;
}

function adapterReconciliationSource(item) {
  const existing = state.adapters.find((adapter) => adapter.source_id === item.id);
  return {
    id: item.id,
    adapter_id: existing?.id || `adapter_leadgen_${item.target_key}`,
    label: item.label,
    url: item.url,
    target_key: item.target_key,
    countries: Array.isArray(item.countries) ? item.countries : [],
    field_keys: Array.isArray(item.field_keys) && item.field_keys.length ? item.field_keys : activeResearchFields(),
    enabled: item.enabled !== false,
    requires_credential: Boolean(item.requires_credential),
    credential_secret_name: String(item.credential_secret_name || ''),
    credential_ref: item.credential_secret_name ? `ctox-secret://credentials/${item.credential_secret_name}` : '',
    input_driven: item.payload?.input_driven === true,
    start_url_source: String(item.payload?.start_url_source || ''),
    operator_instructions: String(item.payload?.instructions || ''),
    secret_value_in_payload: false,
  };
}

async function markAdaptersForReconciliation(sources, commandId, configurationDigest) {
  const now = Date.now();
  for (const source of sources) {
    const existing = state.adapters.find((adapter) => adapter.source_id === source.id);
    const adapterId = existing?.id || `adapter_leadgen_${source.target_key}`;
    const patch = {
      ...adapterPflichtfelder(source),
      id: adapterId,
      source_id: source.id,
      status: 'reconciliation_queued',
      scrape_status: 'generation_queued',
      auth_status: source.requires_credential ? (source.auth_status || 'required') : 'not_required',
      last_command_id: commandId,
      last_task_id: '',
      last_error: '',
      payload: {
        ...(existing?.payload || {}),
        configuration_digest: configurationDigest,
        reconciliation_command_id: commandId,
        secret_value_in_payload: false,
      },
      created_at_ms: Number(existing?.created_at_ms || now),
      updated_at_ms: now,
    };
    const doc = await state.collections.adapters.findOne(adapterId).exec();
    if (doc) await doc.incrementalPatch(patch);
    else await state.collections.adapters.insert(patch);
    const sourceDoc = await state.collections.sources.findOne(source.id).exec();
    await sourceDoc?.incrementalPatch({
      adapter_status: patch.status,
      scrape_status: patch.scrape_status,
      auth_status: patch.auth_status,
      updated_at_ms: now,
    });
    state.adapters = [...state.adapters.filter((adapter) => adapter.id !== adapterId), patch];
    state.sources = state.sources.map((item) => item.id === source.id
      ? { ...item, adapter_status: patch.status, scrape_status: patch.scrape_status, auth_status: patch.auth_status, updated_at_ms: now }
      : item);
  }
}

// Der Abgleich laeuft als Worker-Aufgabe und muss `ctox scrape upsert-target`
// ausfuehren. Die Worker-Sandbox reicht diesen Befehl nicht durch (Feldbefund
// docs/ctox-feldbefund-20260910-sandbox-scrape-cli.md); jeder Abgleich seit
// dem 05.09. ist gescheitert, belegte einen Worker und erschien im Chat als
// "Adapter-Abgleich fuer Recherchekonfiguration sha256:...". Bis der Harness
// das kann, wird er nicht mehr angestossen. Der Rechercheablauf wirkt davon
// unabhaengig ab dem naechsten Recherchestart.
const ADAPTER_RECONCILIATION_VIA_WORKER = false;
async function queueAdapterReconciliation(reason, policyRecord = state.researchPolicyRecord) {
  if (!ADAPTER_RECONCILIATION_VIA_WORKER) {
    console.info('[olg] Adapter-Abgleich zurueckgestellt (Worker-Sandbox)', { reason });
    return { deferred: true, command_id: '', task_id: '' };
  }
  if (state.adapterReconciliationPending) return null;
  const sources = state.sources.filter((source) => !isInternalResearchSource(source) && source.enabled !== false);
  if (!sources.length) {
    throw new Error('Mindestens eine aktive Recherchequelle ist erforderlich.');
  }
  const policy = policyRecord || researchPolicyRecord(null, state.researchPolicyDraft);
  const configurationDigest = await adapterConfigurationDigest(policy, sources);
  // Owner-Vorgabe 04.09.2026: "die adapter sollten nicht jedes mal geupdated
  // werden sondern nur, wenn sich wirklich etwas aendert."
  //
  // Die Entdopplung verlangte bisher ZUSAETZLICH einen bestimmten
  // Abgleichsstatus. Erreichte ein Abgleich diesen Status nie - etwa weil er
  // abgebrochen wurde -, galt die Konfiguration dauerhaft als ungeprueft und
  // JEDER Recherchestart stiess einen neuen 15-Minuten-Lauf mit 21
  // Reparaturauftraegen an. Am 04.09. lagen dadurch 39 Adapter-Reparaturen in
  // der Warteschlange und die Firmenrecherchen verhungerten dahinter.
  //
  // Die Pruefsumme allein entscheidet: gleiche Konfiguration, kein Abgleich.
  // Der Status sagt etwas ueber den letzten Lauf, nicht darueber, ob sich die
  // Konfiguration geaendert hat.
  if (policy.configuration_digest === configurationDigest) {
    return {
      deduplicated: true,
      command_id: policy.reconciliation_command_id || '',
      task_id: policy.reconciliation_task_id || '',
    };
  }
  if (typeof state.ctx?.businessChat?.submitTask !== 'function') {
    throw new Error('CTOX Agent-Aufgaben sind derzeit nicht verfügbar.');
  }
  state.adapterReconciliationPending = true;
  renderSourcePanel();
  const commandId = `cmd_outbound_adapter_reconcile_${crypto.randomUUID()}`;
  try {
    await markAdaptersForReconciliation(sources, commandId, configurationDigest);
    const sourceContract = sources.map(adapterReconciliationSource);
    const resultContract = {
      schema: 'ctox.outbound.adapter_reconciliation.v1',
      configuration_digest: configurationDigest,
      status: 'completed|needs_attention',
      discovered_sources: [{
        id: 'domain.tld', label: 'Provider', url: 'https://domain.tld/', target_key: 'domain-tld',
        countries: ['DE', 'AT', 'CH'], field_keys: ['requested_field'], enabled: true,
        requires_credential: false, reason: 'explicitly mentioned by the research instruction',
      }],
      adapters: [{
        source_id: 'domain.tld', target_key: 'domain-tld', status: 'ready|auth_required|failed|disabled|needs_attention',
        scrape_status: 'test_passed|auth_required|script_failed|temporary_unreachable|disabled',
        auth_status: 'not_required|required|authenticated|auth_required', adapter_revision: 'sha256:...',
        script_path: 'runtime/scraping/targets/domain-tld/scripts/capture.js', last_error: '',
        test: { ok: true, records_found: 1, latency_ms: 1, evidence: [] },
      }],
    };
    const instruction = [
      'Gleiche in EINEM dauerhaften Agent-Lauf alle Playwright-Scraper dieser Outbound-Recherchekonfiguration ab.',
      'Erkenne Anbieter oder Quellen, die in der Rechercheanweisung ausdrücklich genannt sind, aber in sources fehlen, und nimm sie in discovered_sources auf.',
      'Erzeuge oder aktualisiere für jede aktive und jede neu entdeckte Quelle den CTOX Universal-Scraping-Adapter unter runtime/scraping/targets/<target_key>/scripts/.',
      'Registriere jeden Stand mit `ctox scrape upsert-target` und `ctox scrape register-script` und teste ihn anschließend mit `ctox scrape execute --target-key <target_key> --allow-heal`.',
      'Die Skripte müssen genau die angeforderten field_keys liefern und die Rechercheanweisung berücksichtigen.',
      'Falls eine Anmeldung, MFA, CAPTCHA oder menschliche Freigabe nötig ist: status=auth_required, scrape_status=auth_required; niemals Zugangswerte lesen, ausgeben oder in Code/Logs schreiben. Nur credential_ref darf verwendet werden.',
      'Bei Fehlern einen expliziten scrape_status und last_error setzen. Kein stiller Fallback und keine erfundenen Testresultate.',
      'Antworte ausschließlich mit einem minifizierten JSON-Objekt exakt nach dem Ergebnisvertrag, ohne Markdown oder Begleittext.',
      `configuration_digest: ${configurationDigest}`,
      `reason: ${reason}`,
      `research_instruction:\n${researchPolicyInstructions(policy)}`,
      `field_keys: ${JSON.stringify(activeResearchFields())}`,
      `sources: ${JSON.stringify(sourceContract)}`,
      `result_contract: ${JSON.stringify(resultContract)}`,
    ].join('\n\n');
    const submission = requireTrackedSubmission(await state.ctx.businessChat.submitTask({
      instruction,
      prompt: instruction,
      user_message: `Adapter-Abgleich für Recherchekonfiguration ${configurationDigest}`,
      title: 'Recherche-Adapter abgleichen',
      // Hintergrundpflege: darf laufende Recherchen nicht verdrängen (07.09.2026:
      // 'urgent' plus zehn 'high'-Reparaturen hielten vier Nachrecherchen 30 min auf).
      priority: 'low',
      open: false,
      module: 'outbound-lead-generation',
      source_module: 'outbound-lead-generation',
      command_id: commandId,
      command_type: 'outbound.research.adapters.reconcile',
      record_id: RESEARCH_POLICY_ID,
      thread_key: `business-os/outbound-lead-generation/adapter-reconciliation/${configurationDigest}`,
      target: 'data',
      required_skills: ['outbound-lead-generation-research', 'universal-scraping', 'web-unlock'],
      response_channel: 'business_os_chat',
      payload: {
        title: 'Recherche-Adapter abgleichen',
        priority: 'low',
        instruction,
        response_channel: 'business_os_chat',
        configuration_digest: configurationDigest,
        reason,
        policy_id: RESEARCH_POLICY_ID,
        research_instruction: researchPolicyInstructions(policy),
        field_keys: activeResearchFields(),
        sources: sourceContract,
        required_skills: ['outbound-lead-generation-research', 'universal-scraping', 'web-unlock'],
        writeback_contract: {
          schema: 'ctox.outbound.adapter_reconciliation.v1',
          source_collection: 'outbound_lead_generation_sources',
          adapter_collection: 'outbound_lead_generation_adapters',
          policy_collection: 'outbound_lead_generation_research_policies',
          secret_value_in_payload: false,
        },
      },
      client_context: {
        action: 'context-chat',
        source: 'outbound-lead-generation-adapter-reconciliation',
        source_module: 'outbound-lead-generation',
        response_channel: 'business_os_chat',
        writeback_required: true,
      },
    }));
    const now = Date.now();
    const policyDoc = await state.collections.researchPolicies.findOne(RESEARCH_POLICY_ID).exec();
    const reconciliationPatch = {
      configuration_digest: configurationDigest,
      reconciliation_status: 'queued',
      reconciliation_command_id: submission.command_id || commandId,
      reconciliation_task_id: submission.task_id || '',
      reconciliation_error: '',
      updated_at_ms: now,
    };
    await policyDoc?.incrementalPatch(reconciliationPatch);
    state.researchPolicyRecord = { ...(state.researchPolicyRecord || policy), ...reconciliationPatch };
    for (const adapter of state.adapters.filter((entry) => entry.last_command_id === commandId)) {
      const doc = await state.collections.adapters.findOne(adapter.id).exec();
      await doc?.incrementalPatch({ last_task_id: submission.task_id || '', updated_at_ms: now });
      adapter.last_task_id = submission.task_id || '';
    }
    return submission;
  } catch (error) {
    const message = String(error?.message || error);
    const policyDoc = await state.collections.researchPolicies.findOne(RESEARCH_POLICY_ID).exec();
    const failurePatch = {
      reconciliation_status: 'failed',
      reconciliation_command_id: commandId,
      reconciliation_error: message,
      updated_at_ms: Date.now(),
    };
    await policyDoc?.incrementalPatch(failurePatch);
    state.researchPolicyRecord = { ...(state.researchPolicyRecord || policy), ...failurePatch };
    throw error;
  } finally {
    state.adapterReconciliationPending = false;
    renderSourcePanel();
  }
}

// Sellify-Abfragen: der direkte Weg ueber den Datenkanal startet sofort. Kommt
// nach 1,5 s keine Antwort, laeuft der Befehlsweg parallel an; die erste
// gueltige Antwort gewinnt. Vorher wartete jede Abfrage erst 6 s auf den
// direkten Weg und ging danach allein auf den langsamen Befehlsweg - jede
// Kampagnensuche und jeder Importschritt kostete so 15-100 s (25.09.2026).
// ctox-rxdb-js (bis Shell-Build v399) schickt bei requestNative das
// RxCollection-OBJEKT als frame.collection; der CTOX-Server verwirft solche
// Frames still, jede direkte Sellify-Abfrage lief deshalb in ihren Timeout und
// jeder Abgleich ging den 12-s-Umweg ueber einen Befehl. Mit dem Collection-
// NAMEN antwortet derselbe Weg in unter einer Sekunde (Kundeninstanz 26.09.2026).
// Bis der Shell-Fix ueberall geladen ist, fragt die App den Peer selbst.
async function sellifyDirektAnfrage(payload, timeoutMs) {
  const sync = state.ctx?.sync;
  const collection = 'outbound_lead_generation_leads';
  try {
    let bridge = await sync?.startCollection?.(collection, { pin: false, forceDirect: true });
    if (!bridge?.state && bridge?.ready) bridge = await bridge.ready;
    const replication = bridge?.state;
    const verhandelt = await replication?.shared?.ensureNegotiatedPeer?.();
    if (verhandelt?.peerId && typeof replication?.peer?.request === 'function') {
      return await replication.peer.request(
        verhandelt.peerId,
        'ctox.outbound.sellify_lookup.v1',
        [payload],
        timeoutMs,
        collection,
      );
    }
  } catch (error) {
    if (!/Timed out|not open|not connected/i.test(String(error?.message || error))) throw error;
  }
  if (typeof sync?.requestNative !== 'function') throw new Error('Direkter Sellify-Weg nicht verfügbar.');
  // Die native Autorisierung ist collection-gebunden (Lead-Lesezugriff).
  return sync.requestNative('ctox.outbound.sellify_lookup.v1', payload, { timeoutMs, collection });
}

async function sellifyNativeLookup(payload, { commandTimeoutMs = 90_000 } = {}) {
  const gueltig = (antwort) => {
    const inhalt = antwort?.result?.records || antwort?.result?.groups ? antwort.result : antwort;
    if (inhalt?._omitted) throw new Error('Das Sellify-Ergebnis ist zu gross fuer die Uebertragung; bitte genauer suchen.');
    if (!Array.isArray(inhalt?.records) && !Array.isArray(inhalt?.groups)) throw new Error('Sellify lieferte kein gültiges Ergebnis.');
    return { records: Array.isArray(inhalt.records) ? inhalt.records : [], groups: inhalt.groups, truncated: inhalt.truncated };
  };
  let fertig = false;
  const direkt = (async () => {
    if (typeof state.ctx?.sync?.requestNative !== 'function') throw new Error('Direkter Sellify-Weg nicht verfügbar.');
    if (Date.now() < Number(state.sellifyDirectDownUntil || 0)) throw new Error('Direkter Sellify-Weg vorübergehend aus.');
    try {
      const ergebnis = gueltig(await withTimeout(
        sellifyDirektAnfrage(payload, 20_000),
        'Die direkte Sellify-Abfrage hat zu lange gedauert.',
        21_000,
      ));
      fertig = true;
      return ergebnis;
    } catch (error) {
      if (!fertig) state.sellifyDirectDownUntil = Date.now() + 60_000;
      befehlsDiagnose({ command_type: 'native.sellify_lookup' }, `FEHLER ${String(error?.message || error).slice(0, 160)}`);
      throw error;
    }
  })();
  const befehl = (async () => {
    // Nur wenn der direkte Weg nicht schnell antwortet.
    await Promise.race([direkt.catch(() => {}), new Promise((resolve) => setTimeout(resolve, 4_000))]);
    if (fertig) throw new Error('bereits beantwortet');
    let letzterFehler = null;
    for (let versuch = 0; versuch < 2 && !fertig; versuch += 1) {
      const commandId = `cmd_leadgen_sellify_lookup_${crypto.randomUUID()}`;
      try {
        const receipt = await sendeBefehl({
          id: commandId,
          command_id: commandId,
          module: 'outbound',
          command_type: 'outbound.sellify.lookup',
          record_id: String(payload?.entity || 'company'),
          inbound_channel: 'business_os.outbound_lead_generation',
          payload,
          client_context: {
            source_module: 'outbound-lead-generation',
            record_id: String(payload?.entity || 'company'),
            business_chat_auto_focus: false,
            // Diagnose (25.09.2026): der Server protokolliert je Befehl Warte-
            // und Ausfuehrungszeit (command_intake_queue_sample), um die
            // 13-33 s je Sellify-Abgleich Transport, Warteschlange oder
            // Sperren zuzuordnen.
            command_timing_probe: true,
          },
        }, { until: 'terminal', timeoutMs: commandTimeoutMs, sync_queue_tasks: false });
        return gueltig(receipt?.result?.result || receipt?.result || receipt);
      } catch (error) {
        letzterFehler = error;
        console.warn('[olg-sellify-lookup] Command-Versuch', versuch + 1, 'fehlgeschlagen', String(error?.message || error).slice(0, 120));
      }
    }
    throw letzterFehler || new Error('Sellify-Abfrage fehlgeschlagen.');
  })();
  try {
    return await Promise.any([direkt, befehl]);
  } catch (sammel) {
    const fehler = (sammel?.errors || []).find((error) => !/bereits beantwortet/.test(String(error?.message || '')));
    throw fehler || new Error('Sellify-Abfrage fehlgeschlagen.');
  } finally {
    fertig = true;
  }
}

// Die WAHRHEIT ueber eine Quelle steht in der Scrape-Registry, nicht in den
// App-eigenen Adapterdatensaetzen. Owner-Befund 03.09.2026: "diese ganze
// adapter liste steht ueberall nur status geht nicht". Gemessen: die Registry
// fuehrte 21 Ziele, alle `active`, waehrend die Liste Pruefergebnisse vom
// 31.08. und "noch nie geprueft" zeigte.
async function ladeAdapterSkript(sourceId) {
  const item = state.sources.find((entry) => entry.id === sourceId);
  const targetKey = String(item?.target_key || '').trim();
  if (!targetKey) return;
  state.adapterInspectorScriptLaeuft = true;
  const commandId = `cmd_leadgen_registry_script_${crypto.randomUUID()}`;
  try {
    const receipt = await sendeBefehl({
      id: commandId,
      command_id: commandId,
      module: 'outbound',
      command_type: 'outbound.research_source.registry_read',
      record_id: 'registry',
      inbound_channel: 'business_os.outbound_lead_generation',
      payload: { target_keys: [targetKey], include_script_target: targetKey },
      client_context: { source_module: 'outbound-lead-generation', record_id: 'registry', business_chat_auto_focus: false },
    }, { until: 'terminal', timeoutMs: 45_000, sync_queue_tasks: false });
    const result = receipt?.result?.result || receipt?.result || receipt;
    if (state.adapterInspectorSourceId !== sourceId) return;
    state.adapterInspectorScript = result?.script && typeof result.script === 'object'
      ? result.script
      : { target_key: targetKey, available: false, error: 'CTOX liefert das Skript in dieser Version noch nicht mit.' };
  } catch (error) {
    if (state.adapterInspectorSourceId !== sourceId) return;
    state.adapterInspectorScript = { target_key: targetKey, available: false, error: String(error?.message || error) };
  } finally {
    state.adapterInspectorScriptLaeuft = false;
    if (state.adapterInspectorSourceId === sourceId) renderLeadEditor();
  }
}

// Der letzte bekannte Registry-Stand erscheint sofort (Browserspeicher), die
// Antwort des Servers ersetzt ihn, sobald sie da ist. Vorher stand die Liste
// bis zu 27 s auf "wird geladen" (Rundgang 25.09.2026).
function registryAusSpeicher() {
  if (state.registryStand) return;
  try {
    const gespeichert = JSON.parse(globalThis.localStorage?.getItem('ctox.olg.registry.v1') || 'null');
    if (!Array.isArray(gespeichert?.ziele) || !gespeichert.ziele.length) return;
    state.registry = new Map(gespeichert.ziele.map((z) => [String(z.target_key || ''), z]));
    state.registryStand = Number(gespeichert.at) || 1;
  } catch { /* nichts gespeichert */ }
}

async function ladeQuellenRegistry() {
  registryAusSpeicher();
  if (state.registryLaeuft) return;
  state.registryLaeuft = true;
  const commandId = `cmd_leadgen_registry_read_${crypto.randomUUID()}`;
  try {
    const receipt = await sendeBefehl({
      id: commandId,
      command_id: commandId,
      module: 'outbound',
      command_type: 'outbound.research_source.registry_read',
      record_id: 'registry',
      inbound_channel: 'business_os.outbound_lead_generation',
      payload: {},
      client_context: {
        source_module: 'outbound-lead-generation',
        record_id: 'registry',
        business_chat_auto_focus: false,
      },
    }, { until: 'terminal', timeoutMs: 45_000, sync_queue_tasks: false });
    const result = receipt?.result?.result || receipt?.result || receipt;
    const ziele = Array.isArray(result?.targets) ? result.targets : [];
    state.registry = new Map(ziele.map((z) => [String(z.target_key || ''), z]));
    state.registryStand = Date.now();
    try { globalThis.localStorage?.setItem('ctox.olg.registry.v1', JSON.stringify({ at: state.registryStand, ziele })); } catch { /* nur Komfort */ }
    renderSourcePanel();
  } catch (error) {
    // Kein Ersatzwert erfinden: ohne Antwort bleibt die Liste bei dem, was sie
    // selbst weiss, und sagt das auch.
    state.registryFehler = String(error?.message || error).slice(0, 140);
    console.warn('[olg] Registry nicht lesbar', state.registryFehler);
    if (!state.registryNachgefasst) {
      state.registryNachgefasst = true;
      globalThis.setTimeout(() => { void ladeQuellenRegistry(); }, 15_000);
    }
  } finally {
    state.registryLaeuft = false;
  }
}

async function sucheSellifyKampagnen() {
  const input = state.ctx.host.querySelector('[data-sellify-campaign-query]');
  const query = String(input?.value || '').trim();
  state.sellifyImportQuery = query;
  if (query.length < 2) {
    state.sellifyImportNotice = 'Bitte mindestens zwei Zeichen des Kampagnennamens eingeben.';
    renderLeadEditor();
    return;
  }
  state.sellifyImportSucheLaeuft = true;
  const stopp = laufanzeige((sek) => `Sellify-Kampagnen mit „${query}“ werden gesucht … ${sek} s`);
  try {
    // Beauty, 11.09.2026: 2000 volle Mitgliedszeilen waren 1,1 MB, der Peer
    // liess das Ergebnis weg ("exceeds peer wire budget") und die Suche fand
    // "keine Kampagne". Der Server gruppiert jetzt selbst (group_by: name).
    const antwort = await sellifyNativeLookup({
      entity: 'campaign',
      fuzzy_selectors: [{ field: 'name', value: query }],
      group_by: 'name',
      limit: 50000,
    });
    const gruppen = new Map();
    if (Array.isArray(antwort?.groups)) {
      for (const gruppe of antwort.groups) {
        const name = String(gruppe?.name || '').trim();
        if (name) gruppen.set(name, Number(gruppe?.count) || 0);
      }
    } else {
      // Aelterer Server ohne group_by: wie bisher im Browser gruppieren.
      for (const row of (Array.isArray(antwort?.records) ? antwort.records : [])) {
        if (row?.is_deleted) continue;
        const name = String(row?.name || row?.title || '').trim();
        if (!name) continue;
        gruppen.set(name, (gruppen.get(name) || 0) + 1);
      }
    }
    const gekuerzt = Boolean(antwort?.truncated);
    // Neueste Kampagnen zuerst (Datum im Namen), dann nach Groesse; frueher
    // zeigte die Liste nur die 40 groessten, neue Wellen fehlten.
    const datumAusName = (name) => {
      const treffer = [...String(name).matchAll(/(\d{1,2})\.(\d{1,2})\.(\d{4})/g)].pop();
      if (treffer) return Number(treffer[3]) * 10000 + Number(treffer[2]) * 100 + Number(treffer[1]);
      const jahr = String(name).match(/\b(20\d{2})\b/);
      return jahr ? Number(jahr[1]) * 10000 : 0;
    };
    state.sellifyImportResults = [...gruppen.entries()]
      .map(([name, count]) => ({ name, count, truncated: gekuerzt }))
      .sort((a, b) => datumAusName(b.name) - datumAusName(a.name) || b.count - a.count || a.name.localeCompare(b.name, 'de'))
      .slice(0, 200);
    state.sellifyImportNotice = state.sellifyImportResults.length
      ? `${state.sellifyImportResults.length} Kampagnen gefunden, neueste zuerst. Ein Klick importiert die Firmen der Kampagne.`
      : `Keine Sellify-Kampagne mit „${query}“ gefunden.`;
  } catch (error) {
    state.sellifyImportResults = [];
    state.sellifyImportNotice = `Suche fehlgeschlagen: ${String(error?.message || error)}`;
  } finally {
    stopp();
    state.sellifyImportSucheLaeuft = false;
    renderLeadEditor();
  }
}

// Sellify fuehrt das Land nicht oben im Firmendatensatz, sondern in
// payload.sql.country_code ("CH") und mehrsprachig in country
// (`GE:"Schweiz";US:"Switzerland";BA:"CH"`). Der Import las nur
// firma.country_code und setzte deshalb bei allen 32 Firmen der Kampagne
// "Unternehmen CH - Chemie" das Land DE (Kundeninstanz 23.09.2026) - die Recherche
// haette deutsche statt Schweizer Register befragt.
function sellifyLaenderkennung(firma) {
  const sql = firma?.payload?.sql || firma?.sql || {};
  const direkt = [firma?.country_code, sql.country_code, firma?.country_iso, sql.country_iso]
    .map((wert) => String(wert || '').trim().toUpperCase())
    .find((wert) => /^[A-Z]{2}$/.test(wert));
  if (direkt) return direkt;
  const mehrsprachig = String(firma?.country || sql.country || '');
  const kurz = mehrsprachig.match(/\bBA:"([A-Z]{2})"/);
  if (kurz) return kurz[1];
  const name = normalizeProtectionText(mehrsprachig.replace(/\b[A-Z]{2}:/g, ' '));
  if (/(schweiz|switzerland|suisse)/.test(name)) return 'CH';
  if (/(osterreich|österreich|austria)/.test(name)) return 'AT';
  if (/(deutschland|germany)/.test(name)) return 'DE';
  return '';
}

// Einmalige Korrektur bereits importierter Sellify-Leads mit dem falschen
// Standardland DE. Ein Lead, dessen Rechercheauftrag CTOX nie erreichte, wird
// dabei wieder auf "offen" gesetzt.
async function korrigiereSellifyImportlaender() {
  // Laufende Recherchen bleiben unberuehrt; alle anderen bekommen nur das
  // richtige Land (die Recherche selbst hatte CH bereits erkannt).
  const alleLeads = listLeads();
  const kandidaten = alleLeads.filter((lead) => String(lead.country || '') === 'DE'
    && lead.payload?.imported_row?.sellify_contact_id
    && !['queued', 'running', 'requested'].includes(String(lead.research_status || '')));
  if (globalThis.__olgLaenderkorrektur) globalThis.__olgLaenderkorrektur.kandidaten = kandidaten.length;
  if (!kandidaten.length) return 0;
  const ids = [...new Set(kandidaten.map((lead) => String(lead.payload.imported_row.sellify_contact_id)))];
  const laender = new Map();
  // Volle Firmendatensaetze: 90 auf einmal sprengten das Uebertragungsbudget
  // ("Sellify-Ergebnis ist zu gross", 84 Kandidaten am 23.09.2026).
  for (let offset = 0; offset < ids.length; offset += 15) {
    const antwort = await sellifyNativeLookup({
      entity: 'company',
      selectors: ids.slice(offset, offset + 15).map((id) => ({ field: 'contact_id', value: id })),
      limit: 20,
    });
    for (const firma of (Array.isArray(antwort?.records) ? antwort.records : [])) {
      const land = sellifyLaenderkennung(firma);
      if (land) laender.set(String(firma.contact_id), land);
    }
    if (globalThis.__olgLaenderkorrektur) {
      globalThis.__olgLaenderkorrektur.abgefragt = Math.min(offset + 15, ids.length);
      try { document.documentElement.dataset.olgLaenderkorrektur = JSON.stringify(globalThis.__olgLaenderkorrektur); } catch { /* nur Diagnose */ }
    }
  }
  if (globalThis.__olgLaenderkorrektur) globalThis.__olgLaenderkorrektur.laender = Object.fromEntries(laender);
  let korrigiert = 0;
  for (const summary of kandidaten) {
    const land = laender.get(String(summary.payload.imported_row.sellify_contact_id));
    if (!land || land === 'DE') continue;
    const [lead] = await ensureFullLeads([summary.id]);
    if (['queued', 'running', 'requested'].includes(String(lead.research_status || ''))) continue;
    const nieGesendet = lead.research_status === 'failed' && !hatRechercheErgebnis(lead)
      && auftragNichtZugestellt(lead.payload?.research_error || lead.research_error || '');
    await patchLead(lead.id, {
      country: land,
      ...(nieGesendet ? { research_status: 'new', research_error: '', payload: { ...lead.payload, research_error: '', research_not_sent_reason: String(lead.payload?.research_error || '') } } : {}),
    });
    korrigiert += 1;
  }
  if (korrigiert) console.info(`[outbound] Land von ${korrigiert} Sellify-Lead(s) aus Sellify korrigiert`);
  return korrigiert;
}

function starteLaenderkorrektur() {
  const status = { gestartet_ms: Date.now(), ergebnis: null, fehler: '' };
  globalThis.__olgLaenderkorrektur = status;
  // Ablesbar ueber das DOM (Diagnose; die App laeuft nicht im Fenster-Scope).
  const melde = () => {
    try { document.documentElement.dataset.olgLaenderkorrektur = JSON.stringify(status); } catch { /* nur Diagnose */ }
  };
  melde();
  korrigiereSellifyImportlaender()
    .then(async (anzahl) => {
      status.ergebnis = anzahl;
      melde();
      if (anzahl) await reload();
    })
    .catch((error) => {
      status.fehler = String(error?.message || error);
      melde();
      console.warn('[outbound] Laenderkorrektur fuer Sellify-Leads nicht moeglich', error);
    });
}

async function importiereSellifyKampagne(kampagnenName) {
  const name = String(kampagnenName || '').trim();
  if (!name) return;
  if (state.hintergrundImport) {
    zeigeHinweis(`Es läuft bereits ein Import („${state.hintergrundImport}“). Bitte kurz warten.`, 8000);
    return;
  }
  // Der Import laeuft im Hintergrund: der Dialog schliesst sofort, die Leiste
  // zeigt jede Stufe mit Zaehler. Vorher blieb der Dialog bis zum Ende gesperrt
  // und die App wirkte eingefroren (25.09.2026).
  state.hintergrundImport = name;
  state.sellifyImportOpen = false;
  state.sellifyImportNotice = '';
  renderLeadEditor();
  const titel = `Sellify: ${name}`;
  const start = Date.now();
  let stufe = 'Mitglieder werden aus Sellify geladen';
  const zeige = () => zeigeHinweis(`Import „${name}“: ${stufe} … ${Math.floor((Date.now() - start) / 1000)} s`, 0);
  zeige();
  const uhr = setInterval(zeige, 1000);
  try {
    const mitglieder = await sellifyNativeLookup({
      entity: 'campaign',
      selectors: [{ field: 'name', value: name }],
      fields: ['contact_id', 'company_id', 'person_id', 'name', 'is_deleted'],
      limit: 2000,
    });
    const rows = (Array.isArray(mitglieder?.records) ? mitglieder.records : [])
      .filter((row) => row && !row.is_deleted);
    const abgeschnitten = mitglieder?.truncated === true
      || (Array.isArray(mitglieder?.records) && mitglieder.records.length >= 2000);
    const contactIds = [...new Set(rows
      .map((row) => String(row.contact_id || row.company_id || '').trim())
      .filter((id) => id && id !== '0'))];
    if (!contactIds.length) {
      clearInterval(uhr);
      zeigeHinweis(`„${name}“ hat keine importierbaren Firmen.`, 15000);
      return;
    }
    const firmen = [];
    const bloecke = [];
    for (let offset = 0; offset < contactIds.length; offset += 30) bloecke.push(contactIds.slice(offset, offset + 30));
    let geladen = 0;
    stufe = `${rows.length} Kontakte aus ${contactIds.length} Firmen gefunden · Firmendaten 0 / ${contactIds.length}`;
    zeige();
    let naechster = 0;
    const arbeiter = async () => {
      while (naechster < bloecke.length) {
        const teil = bloecke[naechster];
        naechster += 1;
        const antwort = await sellifyNativeLookup({
          entity: 'company',
          selectors: teil.map((id) => ({ field: 'contact_id', value: id })),
          limit: 100,
        });
        for (const record of (Array.isArray(antwort?.records) ? antwort.records : [])) {
          if (record && !record.is_deleted) firmen.push(record);
        }
        geladen += teil.length;
        stufe = `${rows.length} Kontakte aus ${contactIds.length} Firmen gefunden · Firmendaten ${Math.min(geladen, contactIds.length)} / ${contactIds.length}`;
        zeige();
      }
    };
    await Promise.all(Array.from({ length: Math.min(3, bloecke.length) }, arbeiter));
    if (!firmen.length) {
      clearInterval(uhr);
      zeigeHinweis(`Zu „${name}“ wurden ${contactIds.length} Firmen gefunden, aber keine Firmendatensätze.`, 20000);
      return;
    }
    const leadRows = firmen.map((firma, index) => normalizeCompanyRow({
      row_index: index,
      name: String(firma.name || '').trim(),
      website: String(firma.website_url || firma.website || '').trim(),
      domain: normalizedDomain(firma.website_url || firma.website || ''),
      country: sellifyLaenderkennung(firma),
      city: String(firma.city || '').trim(),
      raw: { sellify_contact_id: firma.contact_id, sellify_campaign: name },
    }, index)).filter((row) => row.name);
    stufe = 'Leads werden angelegt';
    zeige();
    const ergebnis = await importPayload({
      source_type: 'sellify_campaign',
      source: { campaign: name },
      title: titel,
      rows: leadRows,
    }, { fortschritt: (text) => { stufe = text; zeige(); } });
    clearInterval(uhr);
    await reload();
    if (Number(ergebnis?.lead_count || 0) > 0) {
      if (campaignListLeads(titel).length) state.selectedCampaign = titel;
      else state.kampagneNachImport = { titel, bis: Date.now() + 120_000 };
      render();
      zeigeHinweis(`Import „${name}“ fertig: ${ergebnis?.lead_count} Firmen aus ${rows.length} Kontakten in ${Math.round((Date.now() - start) / 1000)} s.`
        + (abgeschnitten ? ' Achtung: mehr als 2000 Kontakte, nur die ersten 2000 übernommen.' : ''), 20000);
    } else {
      render();
      zeigeHinweis(`${ergebnis?.message || ''} Keine neue Firma übrig – keine Kampagne „${titel}“ angelegt.`.trim(), 20000);
    }
  } catch (error) {
    clearInterval(uhr);
    zeigeHinweis(`Import „${name}“ fehlgeschlagen: ${String(error?.message || error)}`, 30000);
  } finally {
    clearInterval(uhr);
    state.hintergrundImport = '';
    renderLeadEditor();
  }
}

// normalizeCompanyRow legt die Eingabezeile unter `raw` ab; der Import
// normalisiert jede Zeile zwei- bis dreimal. Gespeichert wurde deshalb
// raw.raw.raw — die Sellify-Firmen-ID eines Kampagnenimports lag drei Ebenen
// tief, und die App (liest imported_row.sellify_contact_id) fand sie nie
// (Klicktest-Befund P1 #2, 11.09.2026). Hier wird bis zur Eingabezeile
// ausgepackt.
// Eine bereits normalisierte Zeile nicht erneut normalisieren: beim zweiten
// Durchgang gewann die blanke Domain vor der Website, der Pfad ("/impressum")
// ging verloren und ftp:// wurde zu https:// (Klicktest P1 D-P1-01).
function normalisiereImportzeile(raw, index) {
  const schonNormalisiert = raw && typeof raw === 'object' && !Array.isArray(raw)
    && Object.prototype.hasOwnProperty.call(raw, 'row_index')
    && Object.prototype.hasOwnProperty.call(raw, 'raw')
    && Object.prototype.hasOwnProperty.call(raw, 'website');
  const zeile = schonNormalisiert ? raw : normalizeCompanyRow(raw, index);
  return spaltenOhneKopfNachInhalt(zeile);
}

// Eine Tabelle ohne Kopfzeile liest der gemeinsame Importer starr als
// "Spalte 1 = Firma, Spalte 2 = Website". Euphrasies Excel (25.09.2026) hatte
// Firma | Land | Ort | Taetigkeit: jede Zeile bekam die Website "AT" und fiel
// als ungueltig heraus. Ohne Kopf werden die Spalten nach ihrem Inhalt
// zugeordnet: Adresse -> Website, Zwei-Buchstaben-Code oder Landesname ->
// Land, kurzer Text -> Ort, langer Text -> Geschaeftstaetigkeit.
function spaltenOhneKopfNachInhalt(zeile) {
  // Nur der kopflose Pfad des gemeinsamen Importers: raw ist dort entweder die
  // Zellenliste selbst oder { __rowIndex, company, domain, raw: Zellen }. Eine
  // Tabelle mit erkannter Kopfzeile traegt benannte Spalten und bleibt, wie sie ist.
  const quelle = Array.isArray(zeile?.raw)
    ? zeile.raw
    : (Array.isArray(zeile?.raw?.raw)
      && Object.keys(zeile.raw).every((key) => ['__rowIndex', 'company', 'domain', 'raw'].includes(key))
      ? zeile.raw.raw
      : null);
  const zellen = quelle ? quelle.map((wert) => String(wert ?? '').trim()) : null;
  if (!zellen || zellen.length < 2) return zeile;
  const istAdresse = (wert) => /^(https?:\/\/)?(www\.)?[a-z0-9-]+(\.[a-z0-9-]+)*\.[a-z]{2,}(\/\S*)?$/i.test(wert);
  const land = (wert) => (/^[A-Z]{2}$/.test(wert) ? wert : (wert.length <= 20 ? sellifyLaenderkennung({ country: wert }) : ''));
  let website = '';
  let country = '';
  let city = '';
  let taetigkeit = '';
  for (const wert of zellen.slice(1)) {
    if (!wert) continue;
    if (!website && istAdresse(wert)) { website = /^https?:\/\//i.test(wert) ? wert : `https://${wert}`; continue; }
    if (!country && land(wert)) { country = land(wert); continue; }
    if (!city && wert.length <= 40 && !/\d{3,}/.test(wert)) { city = wert; continue; }
    if (!taetigkeit && wert.length > 40) taetigkeit = wert;
  }
  return {
    ...zeile,
    name: zeile.name || zellen[0],
    website,
    domain: website ? domainFromUrl(website) : '',
    country: country || '',
    city: city || zeile.city || '',
    ...(taetigkeit ? { geschaeftstaetigkeit: taetigkeit } : {}),
  };
}

function urspruenglicheImportzeile(value) {
  let row = value;
  for (let tiefe = 0; tiefe < 8; tiefe += 1) {
    const inner = row?.raw;
    const istHuelle = row && typeof row === 'object' && !Array.isArray(row)
      && Object.prototype.hasOwnProperty.call(row, 'row_index')
      && inner && typeof inner === 'object' && !Array.isArray(inner);
    if (!istHuelle) break;
    row = inner;
  }
  return row && typeof row === 'object' ? row : {};
}

async function openImporter(defaultTitle = '') {
  await openUniversalImporter(state.ctx, {
    side: 'right', moduleId: 'outbound-lead-generation', entityType: 'lead', commandType: 'outbound-lead-generation.import',
    title: 'Leads importieren', kicker: 'Neu- und Nachrecherche', defaultSource: 'excel', showFileExplorer: false,
    defaultTitle: defaultTitle || `Recherche ${new Date().getFullYear()}`, helperText: 'Excel, Text oder URL importieren. Die Recherche beginnt erst nach der Sichtprüfung.',
    // Das Importfenster ist ein Vollbild-Overlay ueber der GESAMTEN Shell, nicht
    // nur ueber dieser App. Es offen zu halten blockiert das ganze System,
    // solange der Import laeuft. Es schliesst deshalb sofort; die Rueckmeldung
    // ist die Kampagne, die mit ihrer Lead-Zahl in der Liste erscheint.
    submitLabel: 'Vorschau prüfen', confirmSubmitLabel: 'Gültige Leads importieren', submittingLabel: 'Import wird verarbeitet...', doneLabel: 'Leads importiert.', closeOnSubmit: true, dispatch: false,
    previewImport: async ({ payload }) => importPreview(payload),
    // Das Fenster schliesst vor dem Import. Scheitert der Import danach, sieht
    // der Nutzer gar nichts: keine Kampagne, keine Meldung, kein Fehler.
    // Gemessen am 09.09.2026: zweimal "2 gueltige Leads" in der Vorschau,
    // danach kein Datensatz und kein Wort. Ein Fehlschlag muss sichtbar sein.
    onImport: async ({ payload }) => {
      try {
        const ergebnis = await importPayload(payload);
        // Die Kampagne erschien erst nach dem Neuladen der Seite — der Import
        // war laengst auf dem Server (gemessen 10.09.2026: Importjob und Lead
        // um 09:22, in der Liste nichts). Deshalb am 09.09. der Fehlschluss,
        // der Import tue nichts. Jetzt wird neu geladen und die neue Kampagne
        // gleich geoeffnet.
        try {
          await reload();
          const titel = String(payload?.title || '').trim();
          // Stehen die neuen Leads beim ersten Nachladen noch nicht bereit,
          // holt reloadAusfuehren die Auswahl nach (Nachtest P2 FX-1).
          if (titel && !campaignListLeads(titel).length) state.kampagneNachImport = { titel, bis: Date.now() + 120_000 };
          if (titel && campaignListLeads(titel).length) {
            state.selectedCampaign = titel;
            // Die Detailspalte zeigte sonst weiter einen Lead der vorigen
            // Kampagne (Klicktest P1 #7, P3 FX-01).
            const auswahlInKampagne = campaignListLeads(titel).some((lead) => lead.id === state.selectedLeadId);
            if (!auswahlInKampagne) state.selectedLeadId = campaignListLeads(titel)[0]?.id || '';
          }
          render();
        } catch (fehler) {
          console.warn('[outbound-lead-generation] Liste nach dem Import nicht neu geladen', fehler);
        }
        // Die Recherche startet bewusst erst nach der Sichtpruefung; der Hinweis
        // sagt jetzt, wo (Kunde wartete am 28.09.2026 auf eine Recherche).
        zeigeHinweis(`${ergebnis?.message || 'Leads importiert.'} Recherche starten: „Alle recherchieren“ in der Kampagne.`, 0);
        return ergebnis;
      } catch (fehler) {
        const grund = String(fehler?.message || fehler);
        console.error('[outbound-lead-generation] Import fehlgeschlagen', fehler);
        zeigeHinweis(`Import fehlgeschlagen: ${grund}`, 0);
        await showBusinessAlert(
          `Der Import wurde nicht ausgeführt.\n\n${grund}\n\n`
          + 'Es wurde nichts gespeichert. Bitte erneut versuchen; bleibt es dabei, ist die Datenverbindung noch nicht bereit.',
        );
        throw fehler;
      }
    },
  });
}

const IMPORT_KOPF_MUSTER = /(^|[,;\t])\s*"?(name|company|companyname|unternehmen|firma|organisation|organization|account|website|domain|url|webseite|homepage|city|ort|stadt|country|land)"?\s*([,;\t]|$)/i;

// Die Website wird beim Import normalisiert; dabei wurde aus "ftp://…" eine
// gueltige https-Adresse und die Pruefung griff nie (Klicktest P1 IMP-15a).
// Deshalb den Rohwert der Eingabezeile pruefen.
function rohWebsiteMitFremdemSchema(raw) {
  const zeile = urspruenglicheImportzeile(raw);
  for (const [key, value] of Object.entries(zeile || {})) {
    if (!/^(website|url|domain|webseite|homepage)$/i.test(String(key).trim())) continue;
    const text = String(value || '').trim();
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text) && !/^https?:\/\//i.test(text)) return text;
  }
  return '';
}

async function analyzeImportPayload(payload) {
  let rows = [];
  let meta = { skippedOutsideTable: 0, sheets: {}, hasWorkbookMeta: false };
  let rowOffset = 1;
  if (Array.isArray(payload.rows)) {
    // Vorbereitete Zeilen (Sellify-Kampagnen-Import) durchlaufen dieselbe
    // Validierung und Dublettenzusammenfuehrung wie jeder andere Import.
    rows = payload.rows;
  } else if (payload.source_type === 'text') {
    const text = String(payload.source?.text || '');
    const firstLine = text.split(/\r?\n/).find((line) => line.trim()) || '';
    // Dieselben Kopfbegriffe wie der Importer (normalizeCompanyRow); mit der
    // kuerzeren Liste galt "Unternehmen;Webseite" nicht als Kopf und jede
    // Zeilennummer der Vorschau war um eins zu niedrig (Klicktest P1 IMP-07c).
    if (IMPORT_KOPF_MUSTER.test(firstLine)) {
      rowOffset = 2;
      rows = parseDelimitedText(text).map((row, index) => normalizeCompanyRow(row, index));
    } else {
      // Ohne Kopfzeile wurde die erste Firma zur Kopfzeile und die Spalten
      // falsch zugeordnet (Laender fehlten; Rundgang 25.09.2026). Jede Zeile ist
      // eine Firma, Spalten werden nach Inhalt zugeordnet.
      const trenner = [';', '\t', ','].find((zeichen) => firstLine.includes(zeichen)) || ';';
      rows = text.split(/\r?\n/).map((zeile) => zeile.trim()).filter(Boolean)
        .map((zeile, index) => {
          const zellen = zeile.split(trenner).map((zelle) => zelle.trim());
          return normalizeCompanyRow({ __rowIndex: index, company: zellen[0] || '', raw: zellen }, index);
        });
    }
  } else {
    const extracted = await extractImportRows(payload, {
      extractCompanyRowsFromWorkbookFile, extractCompanyRowsFromText,
      parseDelimitedText, importDateiText, normalizeCompanyRow,
    });
    rows = extracted.rows;
    meta = extracted.meta;
    // Dateien mit Kopfzeile: Datenzeile 1 steht in Zeile 2 (P1 D-P1-04).
    const ersteDatei = (payload.source?.files || [])[0];
    if (ersteDatei && /\.xlsx$/i.test(ersteDatei.name || '')) rowOffset = 2;
    else if (ersteDatei) {
      const ersteZeile = importDateiText(ersteDatei).split(/\r?\n/).find((line) => line.trim()) || '';
      if (IMPORT_KOPF_MUSTER.test(ersteZeile)) rowOffset = 2;
    }
  }
  if (!rows.length && payload.source_type === 'url') {
    // Eine Adresse ohne https:// warf "Failed to construct 'URL'" bis in den
    // Dialog (Klicktest-Befund P1 #14). Schema ergaenzen, sonst deutsch melden.
    const eingabe = String(payload.source?.url || '').trim();
    const adresse = /^[a-z][a-z0-9+.-]*:\/\//i.test(eingabe) ? eingabe : `https://${eingabe}`;
    let host = '';
    try { host = new URL(adresse).hostname; } catch { host = ''; }
    if (!host || !host.includes('.')) {
      throw new Error(`„${eingabe}“ ist keine gültige Webadresse.`);
    }
    rows = [{
      row_index: 0,
      name: host.replace(/^www\./, ''),
      website: adresse,
      domain: domainFromUrl(adresse),
      country: '',
      city: '',
      raw: {},
    }];
  }
  const entries = [];
  for (const [index, raw] of rows.entries()) {
    const row = { ...normalisiereImportzeile(raw, index) };
    const rowNumber = Number(row.row_index) + rowOffset;
    const problems = [];
    const hints = [];
    if (!row.name) problems.push('Firmenname fehlt');
    // Eine unlesbare Website oder ein ausgeschriebenes Land kostete bisher den
    // ganzen Lead: ein Excel aus Sellify ergab "0 gueltige Leads", jede Zeile
    // mit "Website ist keine gueltige HTTP(S)-URL" (Euphrasie, 25.09.2026).
    // Beides ermittelt die Recherche ohnehin; der Lead wird importiert, das
    // Feld bleibt leer, und die Vorschau nennt den gelesenen Wert.
    const fremdesSchema = rohWebsiteMitFremdemSchema(raw);
    if ((row.website && (!/^https?:\/\//i.test(row.website) || !domainFromUrl(row.website))) || fremdesSchema) {
      hints.push(`Website „${String(fremdesSchema || row.website).slice(0, 60)}“ nicht lesbar – bleibt leer und wird recherchiert`);
      row.website = '';
      row.domain = '';
    }
    if (row.country && !/^[A-Za-z]{2}$/.test(row.country)) {
      const iso = sellifyLaenderkennung({ country: row.country });
      if (iso) {
        row.country = iso;
      } else {
        hints.push(`Land „${String(row.country).slice(0, 40)}“ nicht erkannt – bleibt leer und wird recherchiert`);
        row.country = '';
      }
    }
    row.id = `lead_${fingerprint(`${row.name}|${row.domain || row.website}|${row.country}`)}`;
    entries.push({ row, rowNumber, problems, hints });
  }
  return finalizeImportAnalysis(entries, meta, payload.selected_groups,
    !Array.isArray(payload.rows) && (payload.source?.files || []).some((file) => /\.xlsx$/i.test(file.name || '')));
}

async function importPreview(payload) {
  const analysis = await analyzeImportPayload(payload);
  // Vorhandene Leads in einer ANDEREN Kampagne bleiben dort (importPayload
  // ueberspringt sie); die Vorschau versprach "werden aktualisiert" (P1 #5).
  const zielKampagne = String(payload.title || '').trim();
  // Wie importPayload: ueber ID ODER Name + Domain in der Zielkampagne
  // (Nachtest P1 IMP-07d: Vorschau "2 gueltige", Import "1 vorhanden aktualisiert").
  const vorhandene = analysis.validRows
    .map((row) => listLeads().find((lead) => lead.id === row.id)
      || listLeads().find((lead) => String(lead.campaign || '').trim() === zielKampagne
        && String(lead.name || '').trim().toLowerCase() === String(row.name || '').trim().toLowerCase()
        && String(lead.domain || '').trim().toLowerCase() === String(row.domain || domainFromUrl(row.website) || '').trim().toLowerCase()))
    .filter(Boolean);
  const existingCount = vorhandene.filter((lead) => String(lead.campaign || '').trim() === zielKampagne).length;
  const andereKampagne = vorhandene.length - existingCount;
  const items = [
    { kind: analysis.validRows.length ? 'success' : 'error', text: `${analysis.validRows.length} gültige, eindeutige Leads` },
    ...analysis.duplicates.map((item) => ({ kind: 'warning', text: `Zeile ${item.rowNumber}: Dublette „${item.name}“ wird zusammengeführt.` })),
    ...analysis.invalid.map((item) => ({ kind: 'error', text: `Zeile ${item.rowNumber}: ${item.problems.join('; ')}.` })),
    ...(analysis.hinweise || []).map((item) => ({ kind: 'warning', text: `Zeile ${item.rowNumber}: ${item.text}.` })),
  ];
  const zielLeads = zielKampagne ? campaignListLeads(zielKampagne).length : 0;
  if (zielKampagne.startsWith('Sellify:')) {
    items.unshift({ kind: 'error', text: `„${zielKampagne}“ ist eine Sellify-Kampagne und nimmt keine Datei-Importe auf. Bitte oben einen eigenen Titel eintragen.` });
  } else if (zielLeads) {
    items.unshift({ kind: 'warning', text: `Ziel ist die bestehende Kampagne „${zielKampagne}“ (${zielLeads} Leads); die neuen Leads werden dort ergänzt. Für eine neue Kampagne oben den Titel ändern.` });
  }
  if (existingCount) items.push({ kind: 'warning', text: `${existingCount} vorhandene Leads werden aktualisiert, nicht dupliziert.` });
  if (andereKampagne) items.push({ kind: 'warning', text: `${andereKampagne === 1 ? '1 Firma ist' : `${andereKampagne} Firmen sind`} bereits in einer anderen Kampagne und ${andereKampagne === 1 ? 'bleibt' : 'bleiben'} dort.` });
  const extras = buildImportPreviewExtras(analysis);
  items.push(...extras.items);
  return {
    groups: extras.groups,
    groupsLabel: extras.groupsLabel,
    groupLimit: extras.groupLimit,
    heading: 'Importvorschau',
    items,
    canProceed: extras.canProceed && !zielKampagne.startsWith('Sellify:'),
    message: extras.message || (analysis.validRows.length
      ? 'Vorschau geprüft. Nur die gültigen, eindeutigen Leads werden importiert.'
      : 'Der Import enthält keine gültigen Leads.'),
  };
}

// Der Importauftrag stand bisher schon VOR dem ersten Lead auf "imported".
// Brach der Import ab (Fenster geschlossen), sah eine halbe Kampagne fertig aus
// - gemessen 11.09.2026: 17 von 30 Firmen. Jetzt steht er auf "importing",
// traegt die Zeilen zum Fortsetzen und wird erst am Ende abgeschlossen.
async function importPayload(payload, { resumeImportId = '', fortschritt = null } = {}) {
  const now = Date.now();
  const importId = resumeImportId || `import_${crypto.randomUUID()}`;
  const analysis = await analyzeImportPayload(payload);
  const normalizedRows = analysis.validRows;
  if (!analysis.canProceed || normalizedRows.length > 5000) {
    throw new Error(analysis.message || 'Bitte eine gültige Auswahl mit höchstens 5.000 Firmen wählen.');
  }
  if (!resumeImportId) {
    await state.collections.imports.insert({
      id: importId, title: payload.title || 'Lead-Import', source_type: payload.source_type || 'text',
      status: normalizedRows.length ? 'importing' : 'empty', lead_count: 0,
      payload: {
        source_url: payload.source?.url || '',
        file_names: (payload.source?.files || []).map((file) => file.name),
        invalid_rows: analysis.invalid,
        duplicate_rows: analysis.duplicates.map(({ rowNumber, name }) => ({ row_number: rowNumber, name })),
        expected_lead_count: normalizedRows.length,
        resume_rows: normalizedRows.length <= 2000 ? normalizedRows : [],
        resume_source_type: payload.source_type || 'text',
        secret_value_in_payload: false,
      },
      created_at_ms: now, updated_at_ms: now,
    });
  }
  // Rundgang 11.09.2026: Ein erneuter Import VERSCHOB eine Firma, die schon
  // in einer anderen Kampagne war, und setzte sie zurueck - samt aller Belege
  // (resetLeadForImport: evidence: []). So verlor Sasol beim Import einer
  // Sellify-Kampagne seine Rechercheergebnisse. Jetzt: Firmen aus anderen
  // Kampagnen bleiben, wo sie sind, und werden gemeldet; ein erneuter Import in
  // DIESELBE Kampagne aktualisiert nur die Importzeile.
  const verschoben = [];
  const zielKampagne = payload.title || `Recherche ${new Date().getFullYear()}`;
  let importiert = 0;
  let aktualisiert = 0;
  let zugeordnet = 0;
  // Vorhandene Leads kennt die App bereits aus ihrer Liste. Frueher fragte der
  // Import fuer JEDEN Lead einzeln die Datenbank (unter Last ein Netz-Umlauf je
  // Firma) und legte jeden einzeln an; eine haengende Einzelabfrage liess den
  // Import bei 21 von 29 Firmen stehen (Kundeninstanz 25.09.2026). Neue Leads werden
  // gesammelt und blockweise angelegt, mit Fortschrittsmeldung.
  const identityRows = listLeads();
  const bekannteIds = new Set(identityRows.map((lead) => lead.id));
  const neueLeads = [];
  const neueLeadIds = new Set();
  const gesamt = normalizedRows.length;
  const melde = (text) => { try { fortschritt?.(text); } catch { /* nur Anzeige */ } };
  // Vorhandene Leads in wenigen Sammelabfragen laden statt je Firma einzeln:
  // 55 Firmen einer Sellify-Kampagne brauchten unter Serverlast ueber sieben
  // Minuten, je Firma ein Netz-Umlauf (Kundeninstanz 26.09.2026).
  const zeilen = normalizedRows.map((raw, index) => {
    const row = normalisiereImportzeile(raw, index);
    return { raw, row, id: raw.id || `lead_${fingerprint(`${row.name}|${row.domain || row.website}|${row.country}`)}` };
  });
  const vorabIds = new Set();
  for (const { row, id } of zeilen) {
    if (bekannteIds.has(id)) vorabIds.add(id);
    const name = String(row.name || '').trim().toLowerCase();
    const domain = String(row.domain || domainFromUrl(row.website) || '').trim().toLowerCase();
    const gleich = identityRows.find((lead) => String(lead.campaign || '').trim() === zielKampagne
      && String(lead.name || '').trim().toLowerCase() === name
      && String(lead.domain || '').trim().toLowerCase() === domain);
    if (gleich) vorabIds.add(gleich.id);
  }
  if (vorabIds.size) melde(`Vorhandene Firmen werden geladen (${vorabIds.size})`);
  const vorab = await ladeLeadDokumente([...vorabIds]);
  const holeDoc = (leadId) => (vorab.has(leadId)
    ? Promise.resolve(vorab.get(leadId))
    : withTimeout(state.collections.leads.findOne(leadId).exec(), 'Lead konnte nicht geladen werden.', 8000).catch(() => null));
  for (const [index, { raw, row, id: zeilenId }] of zeilen.entries()) {
    let id = zeilenId;
    if (index % 5 === 0) melde(`Firmen werden abgeglichen: ${index} / ${gesamt}`);
    // Kampagnenpruefung aus der geladenen Liste; die Datenbank wird nur gefragt,
    // wenn der Lead in DIESER Kampagne aktualisiert werden muss (mit Frist).
    // Eine haengende Einzelabfrage liess Importe auf "importing" stehen.
    const bekannt = bekannteIds.has(id);
    let existing = null;
    if (bekannt) {
      existing = await holeDoc(id);
      // Vorhandener Lead nicht ladbar: NIE neu anlegen (das ueberschriebe seine
      // Recherche), sondern auslassen und nennen.
      if (!existing) { verschoben.push(`${row.name} (vorhanden, gerade nicht ladbar – unverändert)`); continue; }
    }
    // Die ID enthaelt das ROHE Land ("" oder "DE"), gespeichert wird aber "DE":
    // dieselbe Firma kam so zweimal in dieselbe Kampagne (Klicktest P1
    // IMP-07d). Deshalb auch ueber Name + Domain in der Zielkampagne finden.
    if (!existing) {
      const name = String(row.name || '').trim().toLowerCase();
      const domain = String(row.domain || domainFromUrl(row.website) || '').trim().toLowerCase();
      const gleich = identityRows.find((lead) => String(lead.campaign || '').trim() === zielKampagne
        && String(lead.name || '').trim().toLowerCase() === name
        && String(lead.domain || '').trim().toLowerCase() === domain);
      if (gleich) {
        id = gleich.id;
        existing = await holeDoc(id);
        if (!existing) { verschoben.push(`${row.name} (vorhanden, gerade nicht ladbar – unverändert)`); continue; }
      }
      if (neueLeadIds.has(id)) continue;
    }
    const bisherigeKampagne = String(existing?.campaign || '').trim();
    if (existing && bisherigeKampagne && bisherigeKampagne !== zielKampagne) {
      // Nicht verschieben und nicht doppelt anlegen: die vorhandene Firma wird
      // zusaetzlich Mitglied dieser Kampagne; Recherche und Belege bleiben.
      const current = existing.toJSON?.() || existing;
      const weitere = Array.isArray(current.payload?.weitere_kampagnen) ? current.payload.weitere_kampagnen : [];
      if (!weitere.includes(zielKampagne)) {
        const neuePayload = { ...(current.payload || {}), weitere_kampagnen: [...weitere, zielKampagne] };
        if (typeof existing.incrementalPatch === 'function') {
          await existing.incrementalPatch({ payload: neuePayload, updated_at_ms: now });
        } else {
          const doc = await holeDoc(current.id);
          if (doc) await doc.incrementalPatch({ payload: neuePayload, updated_at_ms: now });
        }
        const lokal = state.leads.find((lead) => lead.id === current.id);
        if (lokal) lokal.payload = neuePayload;
      }
      zugeordnet += 1;
      verschoben.push(`${row.name} (auch in „${bisherigeKampagne}")`);
      continue;
    }
    if (existing) {
      const current = existing.toJSON?.() || existing;
      const importzeile = urspruenglicheImportzeile(row.raw || raw);
      // Unveraenderte Importzeile nicht neu schreiben: jeder Schreibvorgang
      // geht einzeln ueber den Abgleich zum Server.
      if (JSON.stringify(current.payload?.imported_row ?? null) !== JSON.stringify(importzeile ?? null)) {
        await existing.incrementalPatch({
          payload: { ...(current.payload || {}), imported_row: importzeile, import_id_last: importId },
          updated_at_ms: now,
        });
      }
      importiert += 1;
      aktualisiert += 1;
      continue;
    }
    const lead = {
      id, import_id: importId, campaign: zielKampagne,
      name: row.name, domain: row.domain || domainFromUrl(row.website), website: row.website || '', city: row.city || '', country: row.country || '',
      ...resetLeadForImport(null), data: {}, contacts: [],
      payload: {
        imported_row: urspruenglicheImportzeile(row.raw || raw),
        min_independent_sources: 1,
      },
      created_at_ms: now, updated_at_ms: now,
    };
    neueLeads.push(lead);
    neueLeadIds.add(lead.id);
  }
  const block = 10;
  for (let offset = 0; offset < neueLeads.length; offset += block) {
    const teil = neueLeads.slice(offset, offset + block);
    melde(`Leads werden angelegt: ${Math.min(offset + teil.length, neueLeads.length)} / ${neueLeads.length}${gesamt > neueLeads.length ? ` (${gesamt - neueLeads.length} bereits vorhanden)` : ''}`);
    let ergebnis = null;
    if (typeof state.collections.leads.bulkInsert === 'function') {
      try {
        ergebnis = await state.collections.leads.bulkInsert(teil);
      } catch (error) {
        console.warn('[olg-import] Blockweises Anlegen fehlgeschlagen, lege einzeln an', error);
        ergebnis = null;
      }
    }
    if (Array.isArray(ergebnis)) {
      importiert += ergebnis.length;
    } else if (ergebnis) {
      importiert += (ergebnis.success || []).length;
      // Einzelne Konflikte (Lead entstand parallel) einzeln nachziehen.
      for (const fehler of (ergebnis.error || [])) {
        const doc = teil.find((lead) => lead.id === (fehler?.documentId || fehler?.documentInDb?.id));
        if (!doc) continue;
        try { await state.collections.leads.upsert(doc); importiert += 1; } catch (einzel) { console.warn('[olg-import] Lead nicht angelegt', doc.id, einzel); }
      }
    } else {
      for (const lead of teil) {
        await state.collections.leads.insert(lead);
        importiert += 1;
      }
    }
  }
  const job = await state.collections.imports.findOne(importId).exec();
  if (job) {
    const current = job.toJSON?.() || job;
    await job.incrementalPatch({
      status: importiert + zugeordnet ? 'imported' : 'empty',
      lead_count: importiert + zugeordnet,
      payload: { ...(current.payload || {}), resume_rows: [], skipped_other_campaign: verschoben, also_member_count: zugeordnet },
      updated_at_ms: Date.now(),
    });
  }
  return {
    status: 'completed',
    lead_count: importiert + zugeordnet,
    skipped_other_campaign: verschoben.length,
    // Neue und aktualisierte Leads getrennt benennen; vorher zaehlten
    // Aktualisierungen als "importiert" und es hiess "1 Leads" (Klicktest P1 #6).
    message: `${importiert - aktualisiert === 1 ? '1 neuer Lead' : `${importiert - aktualisiert} neue Leads`} importiert`
      + (aktualisiert ? `, ${aktualisiert === 1 ? '1 vorhandener aktualisiert' : `${aktualisiert} vorhandene aktualisiert`}` : '')
      + `; ${analysis.duplicates.length} Dubletten zusammengeführt; ${analysis.invalid.length} fehlerhafte Zeilen ausgelassen.`
      + (zugeordnet ? ` ${zugeordnet === 1 ? '1 Firma stand' : `${zugeordnet} Firmen standen`} schon in einer anderen Kampagne und ${zugeordnet === 1 ? 'gehört' : 'gehören'} jetzt zusätzlich zu dieser (Recherche bleibt erhalten).` : '')
      + (verschoben.length > zugeordnet ? ` ${verschoben.length - zugeordnet} Firma/Firmen konnten gerade nicht geladen werden und bleiben unverändert.` : ''),
  };
}

// Laedt Lead-Dokumente blockweise per id-$in-Abfrage (Fenster max. 200).
async function ladeLeadDokumente(ids) {
  const karte = new Map();
  const liste = [...new Set((ids || []).filter(Boolean))];
  for (let offset = 0; offset < liste.length; offset += 100) {
    const teil = liste.slice(offset, offset + 100);
    try {
      const docs = await withTimeout(
        state.collections.leads.find({ selector: { id: { $in: teil } }, limit: 200 }).exec(),
        'Vorhandene Leads konnten nicht geladen werden.',
        30_000,
      );
      for (const doc of docs || []) {
        const leadId = String((doc?.toJSON?.() || doc)?.id || '');
        if (leadId) karte.set(leadId, doc);
      }
    } catch (error) {
      // Fehlende Eintraege holt holeDoc einzeln nach.
      console.warn('[olg-import] Sammelabfrage fehlgeschlagen', error);
    }
  }
  return karte;
}

// Ein Import, der seit Minuten auf "importing" steht, wurde unterbrochen.
function unvollstaendigerImport(campaignName) {
  const name = String(campaignName || '').trim();
  const jobs = (state.imports || []).filter((job) => String(job?.title || '').trim() === name);
  const offen = jobs.find((job) => job?.status === 'importing'
    && Date.now() - Number(job.updated_at_ms || job.created_at_ms || 0) > 3 * 60 * 1000);
  if (!offen) return null;
  // Ein spaeter abgeschlossener Import derselben Kampagne ersetzt den abgebrochenen.
  const spaeterFertig = jobs.some((job) => job?.status === 'imported'
    && Number(job.created_at_ms || 0) > Number(offen.created_at_ms || 0));
  if (spaeterFertig) return null;
  if (campaignListLeads(name).length >= Number(offen.payload?.expected_lead_count || 0)) return null;
  return offen;
}
async function setzeImportFort(importId) {
  const job = (state.imports || []).find((entry) => entry.id === importId);
  const rows = Array.isArray(job?.payload?.resume_rows) ? job.payload.resume_rows : [];
  if (!job || !rows.length) {
    showBusinessAlert('Dieser Import lässt sich nicht fortsetzen – die Zeilen sind nicht gespeichert. Bitte erneut importieren.');
    return;
  }
  const ergebnis = await importPayload({ title: job.title, source_type: job.payload?.resume_source_type || job.source_type, rows }, { resumeImportId: job.id });
  await reload();
  render();
  showBusinessAlert(`Import fortgesetzt. ${ergebnis?.message || ''}`);
}

function resetLeadForImport(existing) {
  return {
    research_status: 'new',
    validation_status: 'pending',
    sellify_status: 'not_started',
    task_id: '',
    command_id: '',
    selected_contact_ids: [],
    evidence: [],
  };
}

async function extractRows(payload) {
  if (payload.source_type === 'text') return extractCompanyRowsFromText(payload.source?.text || '');
  const rows = [];
  for (const file of payload.source?.files || []) {
    if (/\.xlsx$/i.test(file.name || '')) rows.push(...await extractCompanyRowsFromWorkbookFile(file));
    else if (/\.(csv|tsv|txt)$/i.test(file.name || '')) rows.push(...parseDelimitedText(importDateiText(file)).map((row, index) => normalizeCompanyRow(row, index)));
  }
  return rows;
}

// Per Drag & Drop abgelegte Dateien bringt der Importer nur als base64 mit;
// `file.text` ist dann leer und CSV/TSV/TXT ergaben 0 Zeilen (Klicktest-Befund
// P1 #3, 11.09.2026).
function importDateiText(file) {
  if (file?.text) return String(file.text);
  const base64 = String(file?.base64 || '').replace(/^data:[^,]*,/, '');
  if (!base64) return '';
  try {
    const bytes = Uint8Array.from(atob(base64), (zeichen) => zeichen.charCodeAt(0));
    // Excel exportiert CSV oft als Windows-1252; ungueltiges UTF-8 dann so lesen.
    try {
      return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
      return new TextDecoder('windows-1252').decode(bytes);
    }
  } catch {
    return '';
  }
}

// Felder, die ein weiterer Lauf gezielt schliessen soll: alles Angeforderte,
// was nicht verifiziert mit Wert vorliegt. Optionale Felder mit belegtem
// "nicht gefunden" werden nicht erneut verfolgt.
// Personenfelder liefert die Recherche je Kontakt; auf Lead-Ebene setzt der
// Rueckschreibpruefer fuer sie nur den Platzhalter "unsupported". Ein
// Personenfeld gilt deshalb als belegt, wenn der vorrangige Kontakt den Wert hat
// und eine externe Quelle ihn traegt: fuer Name/Funktion die Quellen des
// Kontakts, fuer E-Mail und Pruefung ein feldgenauer Beleg derselben Person.
// Sellify allein belegt nichts (Owner-Regel). Vorher forderte jeder
// Lueckenschluss die schon gefundenen Personen erneut an (Karl Knauer, 25.09.2026).
const PERSON_IDENTITAETSFELDER = new Set(['person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_titel', 'person_geschlecht']);
function personenfeldDurchKontaktBelegt(lead, key) {
  if (!String(key || '').startsWith('person_')) return false;
  const kontaktLead = contactTabLead(lead);
  const kontakt = kontaktLead?.contacts?.[0];
  if (!kontakt || !researchFieldValue(kontaktLead, key)) return false;
  const personKey = String(kontakt.person_key || kontakt.sellify_person_id || '').trim();
  const extern = (eintrag) => {
    const quelle = evidenceSourceKey(eintrag);
    return Boolean(quelle) && quelle !== SELLIFY_SOURCE_ID && !isDocumentationSourceKey(quelle);
  };
  const feldBeleg = [...(Array.isArray(lead?.evidence) ? lead.evidence : []), ...(Array.isArray(kontakt.evidence) ? kontakt.evidence : [])]
    .some((eintrag) => (eintrag?.field_key || eintrag?.field) === key && extern(eintrag)
      && (!eintrag?.person_key || !personKey || String(eintrag.person_key) === personKey));
  if (feldBeleg) return true;
  if (!PERSON_IDENTITAETSFELDER.has(key)) return false;
  return (Array.isArray(kontakt.sources) ? kontakt.sources : [])
    .some((quelle) => extern({ source_id: quelle?.source_id, source_url: quelle?.url || quelle?.source_url }));
}

function offeneRecherchefelder(lead) {
  const optional = optionalResearchFields();
  return activeResearchFields().filter((key) => {
    const status = lead?.field_status?.[key];
    const verifiziert = String(status?.status || '') === 'verified' && researchFieldValue(lead, key) !== '';
    if (verifiziert) return false;
    if (personenfeldDurchKontaktBelegt(lead, key)) return false;
    if (optional.has(key) && researchAnsweredNotFound(lead, key)) return false;
    // Vom Nutzer ohne Wert freigegeben: nicht erneut recherchieren.
    if (leerFreigegeben(lead, key)) return false;
    return true;
  });
}

// Startet genau die Leads, die der Knopf zählt: bei aktiver Suche oder Filter
// nur die sichtbaren, nie stillschweigend die ganze Kampagne (28.09.2026).
async function schliesseKampagnenLuecken(kampagne, sichtbareIds = '') {
  const auswahl = new Set(String(sichtbareIds || '').split(',').filter(Boolean));
  await ensureFullLeads(campaignListLeads(kampagne)
    .filter((lead) => !auswahl.size || auswahl.has(lead.id)).map((lead) => lead.id));
  const leads = campaignLeads(kampagne).filter((lead) => (!auswahl.size || auswahl.has(lead.id))
    && hatRechercheErgebnis(lead)
    && !researchInFlight(lead) && !researchSubmissionPending(lead) && offeneRecherchefelder(lead).length);
  if (!leads.length) return;
  const felderGesamt = leads.reduce((summe, lead) => summe + offeneRecherchefelder(lead).length, 0);
  const ok = await showBusinessConfirm(
    `Bei ${leads.length} Leads werden nur die noch nicht belegten Felder erneut recherchiert (${felderGesamt} Felder insgesamt). Belegte Werte bleiben unverändert.`,
    { title: 'Lücken schließen', confirmLabel: `${leads.length} Leads starten` },
  );
  if (!ok) return;
  let gestartet = 0;
  let nichtGesendet = 0;
  // Drei Starts nebeneinander und ein Zaehler: strikt nacheinander ohne Anzeige
  // hiess bei 18 Leads rund zwoelf Minuten ohne Rueckmeldung (26.09.2026).
  let fertig = 0;
  let naechster = 0;
  const melde = () => zeigeHinweis(`Lücken schließen: ${fertig} / ${leads.length} Leads gestartet oder geprüft …`, 0);
  melde();
  const arbeiter = async () => {
    while (naechster < leads.length) {
      const lead = leads[naechster];
      naechster += 1;
      try {
        const ergebnis = await researchLead(lead.id, { nurFelder: offeneRecherchefelder(lead), suppressAlerts: true });
        if (['queued', 'running'].includes(ergebnis?.status)) gestartet += 1;
        if (ergebnis?.status === 'not_sent') nichtGesendet += 1;
      } catch (error) {
        console.warn('[olg] Lueckenschluss nicht gestartet', lead.id, error);
      }
      fertig += 1;
      melde();
    }
  };
  await Promise.all(Array.from({ length: Math.min(3, leads.length) }, arbeiter));
  zeigeHinweis(`Lückenschluss für ${gestartet} von ${leads.length} Leads gestartet.`, 15000);
  showBusinessAlert(`Lückenschluss für ${gestartet} von ${leads.length} Leads gestartet.${nichtGesendet ? ` ${nichtGesendet} Aufträge haben CTOX nicht erreicht – bitte Seite neu laden und erneut starten.` : ''}`);
}

function rechercheFortsetzung(lead, felder) {
  const bisher = {};
  for (const key of felder) {
    const status = lead?.field_status?.[key];
    if (!status) continue;
    bisher[key] = {
      status: String(status.status || ''),
      reason: String(status.reason || '').slice(0, 400),
    };
  }
  return {
    kind: 'gap_closure',
    open_fields: felder,
    // Nicht erneut recherchieren: belegte Felder und optionale Felder mit
    // begruendetem "nicht gefunden" - getrennt, damit der Worker ein
    // "nicht gefunden" nicht als Beleg liest.
    verified_fields: activeResearchFields().filter((key) => !felder.includes(key)
      && String(lead?.field_status?.[key]?.status || '') === 'verified'),
    settled_not_found_fields: activeResearchFields().filter((key) => !felder.includes(key)
      && String(lead?.field_status?.[key]?.status || '') !== 'verified'),
    previous_status: bisher,
    previous_research_finished_at_ms: Number(lead?.payload?.research_finished_at_ms || 0),
  };
}

async function researchLead(id, options = {}) {
  const lead = state.leads.find((entry) => entry.id === id);
  if (!lead) return { status: 'missing', error: 'Lead nicht gefunden.' };
  if (researchSubmissionPending(lead)) {
    return { status: 'submitting', commandId: '', error: '' };
  }
  if (researchInFlight(lead)) {
    return { status: 'running', commandId: String(lead.command_id || ''), error: '' };
  }
  state.pendingResearchIds.add(id);
  renderCenter();
  renderDetail();
  const commandId = `leadgen-lead-research-${crypto.randomUUID()}`;
  // Vor der nativen Annahme ist der Vorgang nur "wird gestartet", niemals
  // "Wartet". Eine optimistische queued-Projektion war eine falsche
  // Erfolgsbestätigung: schloss man das Fenster während der Sellify-Vorprüfung,
  // existierte serverseitig noch gar kein Command. Erst die bestätigte
  // Command-ID darf den dauerhaften Lead-Lifecycle verändern.
  // Die Dublettenpruefung entscheidet ueber Neu- oder Nachrecherche. Faellt sie
  // aus (Projektion noch nicht da), bleibt es bei der bisherigen Herleitung.
  let bekannteFirma = null;
  let vorwissen = null;
  let vorwissenFehler = null;
  try {
    // Die Weichen-Varianten DUERFEN nicht blind starten — sie bekommen ein
    // ehrliches Budget (Punktabfragen + Fuzzy-Fallback brauchen unter Last
    // mehrere Runden über den nativen Kanal). Der alte 6s-Deckel liess die
    // Weiche unter Live-Last permanent "Abgleich fehlgeschlagen" melden.
    vorwissen = await withTimeout(
      sellifyVorwissen(lead),
      'Sellify-Vorabgleich hat zu lange gedauert; die Recherche wird ohne Vorwissen gestartet.',
      // 6 s reichten unter Last nicht; die Recherche lief dann ohne jedes
      // Sellify-Wissen los (AKEMI, 10.09.2026).
      options.variant ? 120000 : 30000,
    );
    bekannteFirma = vorwissen;
    // Das Ergebnis des Vorabgleichs wurde bisher nach dem Start verworfen.
    // Die Pflegefeld "Aenderungsart" (Neuanlage oder Aktualisierung) haengt
    // genau daran und blieb deshalb bei allen 26 Leads leer.
    const precheck = {
      known: Boolean(vorwissen),
      contact_id: vorwissen?.contact_id ? String(vorwissen.contact_id) : '',
      name: String(vorwissen?.name || ''),
      checked_at_ms: Date.now(),
      version: SELLIFY_PRECHECK_VERSION,
    };
    lead.payload = {
      ...(lead.payload || {}),
      sellify_precheck: precheck,
      ...(vorwissen ? { sellify_snapshot: sellifySnapshotAusVorwissen(vorwissen) } : {}),
    };
    patchLead(lead.id, {
      payload: {
        sellify_precheck: precheck,
        ...(vorwissen ? { sellify_snapshot: sellifySnapshotAusVorwissen(vorwissen) } : {}),
      },
    }, { payloadMerge: true }).catch((fehler) => {
      console.warn('[outbound-lead-generation] Vorabgleich konnte nicht gespeichert werden', fehler);
    });
  } catch (error) {
    vorwissenFehler = error;
    console.warn('[outbound-lead-generation] Sellify-Dublettenpruefung vor der Recherche fehlgeschlagen', error);
  }
  // Harte Sellify-Weiche (Owner-Vorgabe 31.08.): "Neue Recherche" nur für
  // Firmen, die das CRM noch nicht kennt; "Nachrecherche" nur für Firmen,
  // die es kennt. Der Abbruch ist eine Ansage, kein Fehlerzustand des Leads.
  const variant = options.variant || '';
  if (variant) {
    const abbruch = (hinweis) => {
      state.pendingResearchIds.delete(id);
      renderCenter();
      renderDetail();
      if (!options.suppressAlerts) showBusinessAlert(hinweis);
      return { status: 'blocked', commandId: '', error: hinweis };
    };
    // Befund 06.09.2026: Bei verstopfter Sendewarteschlange im Browser brauchte
    // die Antwort auf den Vorabgleich 15-20 Minuten; der 120-s-Deckel brach dann
    // JEDEN Recherchestart ab ("Sellify-Abgleich fehlgeschlagen"). Ein Timeout
    // ist kein Beleg gegen Sellify - die Recherche prueft Sellify selbst noch
    // einmal (Skill-Regel 2) und schreibt nichts nach Sellify. Deshalb: bei
    // Timeout starten und es sagen; nur ein ECHTER Fehler des Abgleichs bricht ab.
    // Befund 07.09.2026: Ueber den Befehlskanal antwortet der Vorabgleich bei
    // verstopftem Sync mit "CTOX wartet noch auf die Rueckmeldung" (Antwort
    // steht aus) - ebenfalls kein Beleg gegen Sellify. Berg, Chemotechnik und
    // Dreidoppel brachen damit still ab; die Recherche prueft Sellify selbst.
    const vorabgleichTimeout = Boolean(vorwissenFehler)
      && /zu lange gedauert|wartet noch auf die R(ue|ü)ckmeldung|Rueckmeldung steht aus|timed out|Timeout/i.test(String(vorwissenFehler?.message || vorwissenFehler));
    if (vorwissenFehler && !vorabgleichTimeout) {
      return abbruch('Der Sellify-Abgleich ist fehlgeschlagen. Da Sellify die Primärquelle ist, wurde keine Recherche gestartet — bitte erneut versuchen.');
    }
    if (vorabgleichTimeout) {
      console.warn('[outbound-lead-generation] Sellify-Vorabgleich ohne Antwort in 120 s - Recherche startet ohne Vorwissen', { leadId: id, variant });
      if (!options.suppressAlerts) {
        zeigeHinweis('Sellify hat nicht rechtzeitig geantwortet – Recherche startet ohne Vorwissen, Sellify wird im Lauf erneut geprüft.', 20000);
      }
    }
    if (variant === 'new' && bekannteFirma) {
      const kennung = [bekannteFirma.name, bekannteFirma.contact_id ? `contact_id ${bekannteFirma.contact_id}` : ''].filter(Boolean).join(', ');
      return abbruch(`Diese Firma wird bereits in Sellify geführt (${kennung}). Hier ist nur eine Nachrecherche möglich.`);
    }
    if (variant === 'followup' && !bekannteFirma && !vorabgleichTimeout) {
      return abbruch('Diese Firma wurde in Sellify nicht gefunden (auch nicht unter Namensvarianten). Hier ist nur eine Neue Recherche möglich.');
    }
  }
  const researchMode = variant === 'new'
    ? 'new_record'
    : variant === 'followup'
      ? 'update_firm'
      : leadResearchMode(lead, bekannteFirma);
  const crmWissen = sellifyVorwissenAlsText(vorwissen);
  // Ein Satz. Feldsatz, Vorrang des Sellify-Bestands, Quellen, Belegregeln und
  // Rückschreibvertrag stehen im Skill `outbound-lead-generation-research`;
  // der gepflegte Rechercheablauf reist strukturiert im Payload mit.
  // Lueckenschluss (Owner 23.09.2026): nach einem Lauf, in dem Quellen
  // zeitweise ausfielen, werden nur die noch nicht belegten Felder erneut
  // recherchiert; Belegtes bleibt unangetastet.
  const nurFelder = Array.isArray(options.nurFelder) && options.nurFelder.length ? options.nurFelder : null;
  const fortsetzung = nurFelder ? rechercheFortsetzung(lead, nurFelder) : null;
  // Agenten liessen D&B trotz Pflichtvorgabe im Skill gelegentlich aus
  // ("Login-Quelle, in dieser Sitzung nicht ausgefuehrt", 26.09.2026); WZ,
  // Umsatz und Mitarbeiter blieben dann offen, obwohl D&B sie liefert. Fehlt
  // ein D&B-Beleg, nennt der Auftragssatz den Adapter ausdruecklich.
  const dnbFelder = ['wz_code', 'umsatz', 'mitarbeiter', 'firma_geschaeftstaetigkeit'];
  const ohneDnb = fortsetzung
    && nurFelder.some((key) => dnbFelder.includes(key))
    && enabledPrivateResearchSources().includes('dnbhoovers.com')
    && !JSON.stringify(lead.evidence || []).includes('dnbhoovers')
    && !JSON.stringify(lead.field_status || {}).includes('dnbhoovers');
  const dnbPflicht = ohneDnb
    ? ' Pflicht vor allem anderen: ctox_web_scrape mit target_key "dnbhoovers-com" (mode execute, timeout_seconds 400, task_id = diese Auftrags-ID) ausführen; D&B meldet sich selbst an, ein Login ist kein Grund, es auszulassen.'
    : '';
  const prompt = fortsetzung
    ? `Setze die Outbound-Recherche für ${lead.name} [${lead.id}] fort (Auftrag ${commandId}): Lücken schließen, nur diese ${nurFelder.length} Felder: ${nurFelder.join(', ')}. Bereits verifizierte Felder weder erneut recherchieren noch senden. Quellen, die beim letzten Lauf ausgefallen, gesperrt oder nicht angemeldet waren, jetzt erneut versuchen.${dnbPflicht}`
    : `Starte eine Outbound ${researchMode === 'update_firm' ? 'Nachrecherche' : 'Neurecherche'} für ${lead.name} [${lead.id}] (Auftrag ${commandId}).${enabledPrivateResearchSources().includes('dnbhoovers.com') ? ' Die registrierten Adapter aus source_policy laufen zuerst, dnbhoovers-com eingeschlossen (timeout_seconds 400, task_id = diese Auftrags-ID); D&B meldet sich selbst an.' : ''}`;
  try {
    if (typeof state.ctx?.businessChat?.submitTask !== 'function') {
      throw new Error('CTOX Chat ist nicht verfügbar. Die Recherche wurde nicht gestartet.');
    }
    const result = requireTrackedSubmission(await state.ctx.businessChat.submitTask({
      instruction: prompt,
      prompt,
      user_message: prompt,
      title: fortsetzung ? `Lücken schließen: ${lead.name}` : `${researchMode === 'update_firm' ? 'Nachrecherche' : 'Neurecherche'}: ${lead.name}`,
      // Owner-Prinzip: der EINZELNE Nachrecherche-Klick oeffnet sofort das
      // Chatfenster mit dem Auftrag; Bulk-/Kampagnenlaeufe halten es zu.
      open: options.openChat === true,
      module: 'outbound-lead-generation',
      source_module: 'outbound-lead-generation',
      command_id: commandId,
      // Kein Steuerbefehl, keine Sonderlogik: ein normaler Chat-Task, den der
      // CTOX-Agent mit dem Skill `outbound-lead-generation-research` abarbeitet.
      record_id: lead.id,
      thread_key: options.threadKey || `business-os/outbound-lead-generation/lead/${lead.id}`,
      mode: researchMode,
      target: 'data',
      required_skills: ['outbound-lead-generation-research', 'universal-scraping', 'web-unlock'],
      writeback_contract: {
        collection: 'outbound_lead_generation_leads',
        allowed_collections: ['outbound_lead_generation_leads'],
        record_ids: [lead.id],
        min_independent_sources: 1,
        // Befund 04.09.2026: Der Vertrag nannte das Ziel, aber nicht den WEG.
        // Vier Recherchen (BEWI RAW, BNT, BUEFA, CHEMOFAST) waren inhaltlich
        // fertig - Identitaet, Anschrift, Kennzahlen, Ansprechpartner - und
        // gingen verloren, weil der Agent den Writeback ueber die CLI
        // versuchte. Sein eigener Schritt 6 protokolliert:
        // "Writeback-Befehl via CLI (Sandbox blockiert SQLite - writeback
        // nicht moeglich)". Der Weg steht jetzt im Vertrag.
        command_type: 'outbound.lead.research_writeback',
        mechanism: 'business_command',
        forbidden_mechanisms: ['cli', 'shell', 'terminal', 'sqlite', 'direct_sql'],
        note: 'Das Ergebnis ausschliesslich ueber das MCP-Werkzeug business_os.execute_writeback '
          + '(record_id = diese lead_id, payload = field_status + result) zurueckschreiben; '
          + 'es erzeugt den Business-Command "outbound.lead.research_writeback" gebunden an diesen Auftrag. '
          + 'Terminal, Shell, CLI (ctox business-os commands dispatch), direkter SQLite-Zugriff sowie '
          + 'business_os.execute_action/propose_action sind KEIN Writeback; ein Writeback ueber diese Wege '
          + 'verliert die gesamte Recherche und die Aufgabe gilt als nicht erledigt.',
      },
      payload: {
        lead_id: lead.id,
        // Was der Agent über den Lead schon weiss (er hat keinen Record-Lesebefehl):
        lead_snapshot: { data: lead.data || {}, contacts: lead.contacts || [] },
        company: lead.name,
        country: normalizedResearchCountry(lead.country),
        mode: researchMode,
        fields: nurFelder || activeResearchFields(),
        ...(fortsetzung ? { continuation: fortsetzung } : {}),
        include_private: enabledPrivateResearchSources(),
        person_priorities: [...PERSON_RESEARCH_PRIORITIES],
        // Der gepflegte Rechercheablauf (Schritte 0..x) ist der verbindliche
        // Auftragsteil: der Agent liest ihn per `commands inspect` und folgt
        // ihm. Nachrecherche nutzt den eigenen Ablauf, sofern gepflegt.
        research_instructions: (researchMode === 'update_firm'
          ? followupResearchPolicyInstructions(state.researchPolicyRecord)
          : '')
          || researchPolicyInstructions(state.researchPolicyRecord)
          || state.researchPolicy
          || DEFAULT_RESEARCH_POLICY,
        research_instructions_variant: researchMode === 'update_firm'
          && followupResearchPolicyInstructions(state.researchPolicyRecord)
          ? 'followup' : 'default',
        research_instructions_default: researchPolicyInstructions(state.researchPolicyRecord) || state.researchPolicy || DEFAULT_RESEARCH_POLICY,
        known_person_records: sellifyVorwissenAlsPersonRecords(vorwissen),
        // Regel 0 des Rechercheablaufs: der Sellify-Bestand ist Ausgangswert
        // und Quelle. Der Worker hat keinen eigenen Sellify-Zugang.
        sellify_company: sellifySnapshotAusVorwissen(vorwissen),
        crm_knowledge: crmWissen,
        auto_browser_capture: true,
        campaign_run_id: options.campaignRunId || '',
        workflow_id: options.campaignRunId || '',
        response_channel: 'business_os_chat',
        title: fortsetzung ? `Lücken schließen: ${lead.name}` : `${researchMode === 'update_firm' ? 'Nachrecherche' : 'Neurecherche'}: ${lead.name}`,
        source_policy: enabledSourcePolicy(),
        // Harness liest den Vertrag aus command.payload.writeback_contract -
        // also DIESEN, nicht den auf Task-Ebene. Gemessen 05.09.2026: der
        // persistierte Vertrag hatte weder mechanism noch command_type, der
        // Harness konnte den Writeback-Weg deshalb nicht aktivieren.
        // supports_command_writeback verlangt mechanism=business_command,
        // command_type=outbound.lead.research_writeback, record_ids nicht leer.
        writeback_contract: {
          collection: 'outbound_lead_generation_leads',
          allowed_collections: ['outbound_lead_generation_leads'],
          command_type: 'outbound.lead.research_writeback',
          mechanism: 'business_command',
          forbidden_mechanisms: ['cli', 'shell', 'terminal', 'sqlite', 'direct_sql'],
          record_ids: [lead.id],
          min_independent_sources: 1,
        },
      },
      client_context: {
        action: 'context-chat',
        source: 'outbound-lead-generation-lead-research',
        source_module: 'outbound-lead-generation',
        record_id: lead.id,
        response_channel: 'business_os_chat',
        writeback_required: true,
        campaign_run_id: options.campaignRunId || '',
      },
    }), { allowControlCommand: true });
    lead.research_status = 'running';
    lead.command_id = result.command_id;
    await patchLead(id, {
      research_status: 'running',
      command_id: result.command_id,
      task_id: result.task_id,
      payload: {
        ...lead.payload,
        campaign_run_id: options.campaignRunId || '',
        research_started_at_ms: Date.now(),
      },
    });
    return { status: 'queued', commandId: result.command_id, taskId: result.task_id || '' };
  } catch (error) {
    const message = String(error?.message || error);
    if (istKanalAbriss(error) && options._kanalRetry !== true) {
      await recoverCommandChannel('research-lead');
      state.pendingResearchIds.delete(id);
      return researchLead(id, { ...options, _kanalRetry: true });
    }
    // Ein abgelaufener Wartender ist KEIN gescheiterter Auftrag. Der Command-Bus
    // meldet den Zeitablauf ausdruecklich als transient/projection_delayed: der
    // Vorgang laeuft weiter, nur die Projektion kam nicht rechtzeitig hinterher.
    // Vorher setzte jeder Fehler den Lead auf failed — am 11.08.2026 lagen
    // dadurch bei vier Firmen (ANGUS, Berg, Dr. Kurt Richter, Aeroxon) je 21
    // eingebettete Felder samt Quellen fertig im Ergebnis, waehrend der Lead
    // "fehlgeschlagen, 0 belegt" zeigte. Der Lead bleibt jetzt verfolgbar; der
    // Abgleich in reconcileResearchCommands wendet das Ergebnis an, sobald die
    // Projektion es sichtbar macht.
    if (isTransientResearchWaitError(error)) {
      lead.research_status = 'running';
      lead.command_id = commandId;
      await patchLead(id, {
        research_status: 'running',
        command_id: commandId,
        payload: {
          ...lead.payload,
          campaign_run_id: options.campaignRunId || '',
          research_wait_note: message,
          research_wait_since_ms: Number(lead.payload?.research_wait_since_ms || Date.now()),
        },
      });
      return { status: 'running', commandId, note: message };
    }
    if (auftragNichtZugestellt(error)) {
      // Nichts wurde gestartet: Status bleibt, nur der Hinweis wird vermerkt.
      await patchLead(id, {
        payload: {
          ...lead.payload,
          research_not_sent_at_ms: Date.now(),
          research_not_sent_reason: message,
        },
      });
      if (!options.suppressAlerts) showBusinessAlert(NICHT_ZUGESTELLT_HINWEIS);
      return { status: 'not_sent', commandId, error: message };
    }
    const startPatch = fehlerOhneErgebnisverlust(lead, message, {
      campaign_run_id: options.campaignRunId || '',
      research_finished_at_ms: Date.now(),
    });
    lead.research_status = startPatch.research_status;
    lead.command_id = commandId;
    await patchLead(id, { ...startPatch, command_id: commandId });
    if (!options.suppressAlerts) showBusinessAlert(message);
    return { status: 'failed', commandId, error: message };
  } finally {
    state.pendingResearchIds.delete(id);
    renderCenter();
    renderDetail();
  }
}

// Eine Firma, die im CRM bereits existiert, ist eine NACHrecherche — auch wenn
// wir sie selbst noch nie uebergeben haben. Vorher entschied allein
// `sellify_status`, also ob WIR schon einmal geschrieben hatten; eine seit
// Jahren in Sellify gefuehrte Firma lief damit als Neuanlage, und die im CRM
// vorhandenen Ansprechpartner (im Schnitt 3,5 je Firma) blieben unbeachtet.
function leadResearchMode(lead, existingSellifyCompany = null) {
  if (lead?.sellify_status === 'completed') return 'update_firm';
  return existingSellifyCompany ? 'update_firm' : 'new_record';
}

// Einmalige Reparatur fuer Leads, die vor 1.0.95 auf "Unvollstaendig"
// zurueckgestuft wurden, obwohl ihr Ergebnis noch im Datensatz steht.
// Leads, die die alte 3-h-Frist als "nicht zurueckgemeldet" beendet hat,
// obwohl ihr Auftrag noch wartete, laufen wieder, solange sie innerhalb der
// neuen Frist liegen. Kommt der Auftrag nie an, beendet sie die 24-h-Frist.
async function stelleWartendeAuftraegeWiederHer() {
  const jetzt = Date.now();
  const betroffen = state.leads.filter((lead) => {
    if (!String(lead?.command_id || '').trim()) return false;
    const start = Number(lead.payload?.research_wait_since_ms || lead.payload?.research_started_at_ms || 0);
    if (!(start > 0 && jetzt - start < RESEARCH_RUNNING_MAX_MS)) return false;
    const status = String(lead?.research_status || '');
    if (status === 'failed') {
      return String(lead?.research_error || lead?.payload?.research_error || '') === NICHT_ZURUECKGEMELDET;
    }
    // Leads mit frueherem Ergebnis landeten statt auf failed auf "Pruefung
    // noetig" (fehlerOhneErgebnisverlust); nur solange danach kein neueres
    // Ergebnis eintraf.
    if (status === 'needs_review') {
      const payload = lead.payload || {};
      return String(payload.research_last_failure || '') === NICHT_ZURUECKGEMELDET
        && Number(payload.research_last_failure_at_ms || 0) >= start
        && Number(payload.research_last_failure_at_ms || 0) >= Number(payload.research_finished_at_ms || 0) - 1000;
    }
    return false;
  });
  for (const lead of betroffen) {
    await patchLead(lead.id, {
      research_status: 'running',
      research_error: '',
      payload: {
        ...(lead.payload || {}),
        research_error: '',
        research_last_failure: '',
        research_resumed_waiting_at_ms: Date.now(),
      },
    });
  }
  return betroffen.length > 0;
}

async function stelleUeberschriebeneErgebnisseWiederHer() {
  const betroffen = state.leads.filter((lead) => (
    String(lead?.research_status || '') === 'failed'
    && Array.isArray(lead?.payload?.researched_field_keys)
    && lead.payload.researched_field_keys.length > 0
  ));
  if (!betroffen.length) return false;
  for (const lead of betroffen) {
    await patchLead(lead.id, {
      research_status: 'needs_review',
      research_updated_at_ms: Date.now(),
      payload: {
        ...(lead.payload || {}),
        research_error: '',
        research_last_failure: String(lead.payload?.research_error || 'Der Task endete fehlerhaft, das Ergebnis war aber bereits geschrieben.'),
        research_result_restored_at_ms: Date.now(),
      },
    });
  }
  zeigeHinweis(`${betroffen.length} überschriebene${betroffen.length === 1 ? 's' : ''} Rechercheergebnis${betroffen.length === 1 ? '' : 'se'} wiederhergestellt.`);
  return true;
}

// Diagnose (27.09.2026): SLM/LUZI blieben nach 1.0.269 unberuehrt, ohne dass
// sichtbar war, wo der Abgleich haengt. Je Durchlauf eine Zeile mit Phasen und
// Dauer, dazu die Entscheidung je laufendem Lead; nur wenn sich etwas aendert
// oder der Durchlauf laenger als 10 s braucht, damit das Log nicht volllaeuft.
function abgleichDiagnose(stufe, daten = {}) {
  const lauf = state.abgleichDiagnoseLauf;
  if (!lauf) return;
  lauf.stufen.push([stufe, Date.now() - lauf.start, daten]);
  state.abgleichDiagnoseStufe = { stufe, seit: Date.now(), daten };
}
async function reconcileResearchCommands({ authoritative = false } = {}) {
  if (state.reconcilingCommands) {
    const haengt = state.abgleichDiagnoseStufe;
    if (haengt && Date.now() - haengt.seit > 30_000 && Date.now() - Number(state.abgleichDiagnoseHaengtGemeldet || 0) > 60_000) {
      state.abgleichDiagnoseHaengtGemeldet = Date.now();
      console.info('[olg-abgleich] belegt', JSON.stringify({ stufe: haengt.stufe, seit_s: Math.round((Date.now() - haengt.seit) / 1000), daten: haengt.daten, offeneBefehle: [...(state.befehlsstatusOffen || new Map()).entries()].map(([id, start]) => [id.slice(-12), Math.round((Date.now() - start) / 1000)]) }));
    }
    return false;
  }
  state.reconcilingCommands = true;
  state.abgleichDiagnoseLauf = { start: Date.now(), stufen: [], entscheidungen: [] };
  let changed = false;
  try {
    abgleichDiagnose('wiederherstellen');
    if (await stelleWartendeAuftraegeWiederHer()) changed = true;
    if (await stelleUeberschriebeneErgebnisseWiederHer()) changed = true;
    // Auch failed/needs_review abgleichen: ein Lead, dessen früher Lauf
    // scheiterte, verließ sonst die Menge für immer — ein später doch noch
    // eingetroffenes completed-Ergebnis (samt gefundener Kontakte) wurde nie
    // mehr angewendet. Der Beobachtungsschlüssel verhindert Doppelarbeit.
    // Nur Leads, deren Ausgang noch offen ist: laufende immer, sonst nur solange
    // noch kein Vorgangsergebnis angewendet wurde. Fertige Leads mit
    // angewendetem Ergebnis fragten sonst alle 5 s erneut beim Server nach
    // (137 "Pruefung noetig"-Leads, Kundeninstanz 26.09.2026).
    const pendingLeads = state.leads.filter((lead) => {
      const status = String(lead.research_status || '');
      if (status === 'queued' || status === 'running') return true;
      if (!['new', 'failed', 'needs_review'].includes(status)) return false;
      return !String(lead.payload?.observed_research_command_key || '').trim();
    });
    abgleichDiagnose('befehle-laden', { offen: pendingLeads.length, laufend: pendingLeads.filter((lead) => researchInFlight(lead)).length });
    const commands = uniqueCommands(await demandResearchCommands(pendingLeads, { authoritative }));
    abgleichDiagnose('anwenden', { befehle: commands.length });
    for (const lead of pendingLeads) {
      const command = researchCommandForLead(lead, commands);
      if (researchInFlight(lead)) {
        state.abgleichDiagnoseLauf.entscheidungen.push([
          lead.id,
          String(command?.command_id || command?.id || '-').slice(-12),
          String(command ? researchCommandObservationKey(command).split(':').pop() : '-'),
          String(command?.execution_phase || '-'),
          lead.payload?.observed_research_command_key === (command ? researchCommandObservationKey(command) : '') ? 'gleich' : 'neu',
        ]);
      }
      // `new` ist nur ein Recovery-Fall: der Browser kann nach erfolgreichem
      // Dispatch geschlossen worden sein, bevor sein lokaler running-Patch
      // repliziert war. Historische Commands vor dem letzten Lead-Import duerfen
      // einen absichtlich zurueckgesetzten Lead dagegen nicht wiederbeleben.
      if (lead.research_status === 'new' && !newerResearchCommandCanRecoverLead(lead, command)) {
        continue;
      }
      // Ein Lead ohne auffindbaren Vorgang darf nicht ewig "laeuft" anzeigen.
      // ANGUS Chemie stand am 11.08.2026 ueber fuenf Stunden auf running,
      // obwohl sein Vorgang um 12:04 fertig war — fuer den Nutzer nicht von
      // einem haengenden System zu unterscheiden. Nach der Obergrenze sagt der
      // Lead ehrlich, dass die Rueckmeldung ausblieb; ein spaeter doch noch
      // gefundener Vorgang wird oben trotzdem weiter angewendet, weil failed
      // Teil der abgeglichenen Menge bleibt.
      if (!command) {
        if (lead.research_status !== 'running' && lead.research_status !== 'queued') continue;
        const startedAt = Number(
          lead.payload?.research_wait_since_ms || lead.payload?.research_started_at_ms || 0,
        );
        if (!startedAt || Date.now() - startedAt < RESEARCH_RUNNING_MAX_MS) continue;
        await patchLead(lead.id, fehlerOhneErgebnisverlust(
          lead,
          NICHT_ZURUECKGEMELDET,
          { research_finished_at_ms: Date.now() },
        ));
        changed = true;
        continue;
      }
      const observedCommandId = String(command.command_id || command.id || '').trim();
      const observationKey = researchCommandObservationKey(command);
      // Der Schluessel verhindert Doppelarbeit, darf aber keinen Lead in "Läuft"
      // festhalten, dessen Vorgang laengst beendet ist: SLM (lead_1x0tgat)
      // stand am 27.09.2026 um 13:06:37 wieder auf running, obwohl der
      // Abgleich um 13:05:58 den gescheiterten Vorgang angewendet und den
      // Schluessel gesetzt hatte; danach wurde er nie wieder abgeglichen.
      const zurueckgefallen = researchInFlight(lead) && befehlIstEndgueltig(command);
      if (lead.payload?.observed_research_command_key === observationKey && !zurueckgefallen) {
        // Nicht terminale Vorgaenge behalten ihren Schluessel (id:none), waehrend
        // die Ausfuehrungsphase wechselt (queued -> leased -> retry_wait). Die
        // Phase wird deshalb hier eigens verglichen (Codex-Review 1.0.268).
        const nurPhase = ausfuehrungsphasePatch(lead, command);
        if (nurPhase) {
          await patchLead(lead.id, nurPhase);
          changed = true;
        }
        continue;
      }
      const patch = researchCommandLeadPatch(lead, command);
      if (!patch) continue;
      patch.payload = {
        ...(patch.payload || lead.payload || {}),
        observed_research_command_key: observationKey,
        ...(lead.payload?.observed_research_command_key === observationKey
          ? {
            research_status_reapplied_at_ms: Date.now(),
            research_status_reapplied_count: Number(lead.payload?.research_status_reapplied_count || 0) + 1,
          }
          : {}),
      };
      abgleichDiagnose('schreiben', { lead: lead.id });
      await patchLead(lead.id, patch);
      changed = true;
    }
  } finally {
    state.reconcilingCommands = false;
    const lauf = state.abgleichDiagnoseLauf;
    state.abgleichDiagnoseLauf = null;
    state.abgleichDiagnoseStufe = null;
    if (lauf) {
      const dauer = Date.now() - lauf.start;
      const signatur = JSON.stringify(lauf.entscheidungen);
      if (dauer > 10_000 || changed || signatur !== state.abgleichDiagnoseSignatur) {
        state.abgleichDiagnoseSignatur = signatur;
        console.info('[olg-abgleich] durchlauf', JSON.stringify({ dauer_ms: dauer, geaendert: changed, stufen: lauf.stufen, laufend: lauf.entscheidungen }));
      }
    }
  }
  return changed;
}

function newerResearchCommandCanRecoverLead(lead, command) {
  if (!command) return false;
  const leadUpdatedAt = Number(lead?.updated_at_ms || 0);
  const commandCreatedAt = Number(
    command?.created_at_ms
    || command?.observed_at_ms
    || command?.updated_at_ms
    || 0,
  );
  return commandCreatedAt > 0 && (!leadUpdatedAt || commandCreatedAt >= leadUpdatedAt);
}

async function demandResearchCommands(leads = [], options = {}) {
  const commandIds = [...new Set(leads.flatMap((lead) => [
    lead?.command_id,
    lead?.payload?.last_research_command_id,
    lead?.payload?.campaign_command_id,
  ]).map((value) => String(value || '').trim()).filter(Boolean))];
  return uniqueCommands(await loadCommandStatuses(commandIds, { ...options, quelle: 'recherche' }));
}

// Die Kampagne speichert zunaechst die ID ihres dauerhaften Parent-Tasks auf
// jedem Lead. Die fachlichen Ergebnisse liegen jedoch in je einem
// web_stack.person_research Child-Command. Exakte getStatus-Aufrufe auf die
// Parent-ID koennen diese Childs prinzipbedingt nicht finden; deshalb liest der
// Abgleich die bereits replizierte Command-Collection gezielt per record_id.
async function loadResearchCommandsForLeads(leads = []) {
  const recordIds = [...new Set((leads || [])
    .map((lead) => String(lead?.id || '').trim())
    .filter(Boolean))];
  const bus = state.ctx?.commandBus;
  if (!recordIds.length || typeof bus?.getStatusesByRecordIds !== 'function') return [];
  try {
    const commands = await bus.getStatusesByRecordIds(recordIds, {
      commandType: 'web_stack.person_research',
    });
    if (!state.researchCommandQueryLogged) {
      state.researchCommandQueryLogged = true;
      console.info('[outbound-lead-generation] Recherche-Child-Abgleich', {
        requested_records: recordIds.length,
        returned_commands: commands.length,
        command_types: [...new Set(commands.map((command) => String(command?.command_type || '')))],
      });
    }
    return commands;
  } catch (error) {
    console.warn('[outbound-lead-generation] Recherche-Childs konnten nicht gelesen werden', error);
    return [];
  }
}

// Die drei Abgleiche (Recherche, Kampagnenlaeufe, Adapter) laufen alle 5 s und
// fragten dabei JEDEN je gesehenen Befehl erneut einzeln beim Server ab, auch
// laengst abgeschlossene, alle gleichzeitig. Bei 207 Leads waren das je Tab
// hunderte Abfragen alle 5 s; die Abfragewarteschlange des Servers lief voll
// (75 offen, 74 s je Abfrage) und jede andere Aktion der App blieb haengen
// (Kundeninstanz 26.09.2026). Abgeschlossene Befehle aendern sich nicht mehr und
// werden gemerkt; hoechstens vier Abfragen laufen gleichzeitig.
const BEFEHL_ENDZUSTAENDE = new Set([
  'completed', 'failed', 'cancelled', 'canceled', 'blocked', 'rejected', 'succeeded', 'handled', 'timed_out', 'expired',
]);
const BEFEHL_ENDGUELTIG_MERKEN_MS = 10 * 60 * 1000;
const BEFEHL_UNBEKANNT_MERKEN_MS = 30 * 1000;
const BEFEHLSSTATUS_PARALLEL = 3;
const befehlsstatusCache = new Map();
// Gemeinsame Grenze fuer ALLE Abgleiche: die Shell hat je Tab nur sechs
// Abfrage-Plaetze; belegten die Abgleiche sie, warteten Einstellungen,
// Katalog und Leads beim Start 15-26 s (26.09.2026).
let befehlsstatusLaufend = 0;
const befehlsstatusWarteschlange = [];
async function mitBefehlsstatusPlatz(arbeit) {
  // Ein freiwerdender Platz geht direkt an den naechsten Wartenden.
  if (befehlsstatusLaufend >= BEFEHLSSTATUS_PARALLEL) {
    await new Promise((resolve) => befehlsstatusWarteschlange.push(resolve));
  } else {
    befehlsstatusLaufend += 1;
  }
  try {
    return await arbeit();
  } finally {
    const naechster = befehlsstatusWarteschlange.shift();
    if (naechster) naechster();
    else befehlsstatusLaufend -= 1;
  }
}

function befehlIstEndgueltig(command) {
  const terminal = String(command?.terminal_status || '').trim().toLowerCase();
  if (terminal && terminal !== 'none') return true;
  if (String(command?.execution_phase || '').trim().toLowerCase() === 'terminal') return true;
  return BEFEHL_ENDZUSTAENDE.has(String(command?.status || '').trim().toLowerCase());
}

async function loadCommandStatuses(commandIds = [], _options = {}) {
  const bus = state.ctx?.commandBus;
  const zaehler = { quelle: String(_options?.quelle || ''), angefragt: new Set(commandIds).size, leerBeispiele: [], messzeit: new Date().toISOString(), cacheLeer: 0, cacheTreffer: 0, lokalNull: 0, lokalFehler: 0, lokalFehlerText: '', serverKeineCollection: 0, serverNull: 0, serverFehlerText: '', serverTreffer: 0, gefunden: 0 };
  if (typeof bus?.getStatus !== 'function') {
    befehlsstatusDiagnose({ ...zaehler, grund: 'kein commandBus.getStatus' });
    return [];
  }
  const jetzt = Date.now();
  const ergebnisse = new Map();
  const offen = [];
  for (const commandId of new Set(commandIds)) {
    const gemerkt = befehlsstatusCache.get(commandId);
    if (gemerkt && gemerkt.bis > jetzt) {
      if (gemerkt.command) { ergebnisse.set(commandId, gemerkt.command); zaehler.cacheTreffer += 1; } else zaehler.cacheLeer += 1;
      continue;
    }
    offen.push(commandId);
  }
  let naechster = 0;
  const arbeiter = async () => {
    while (naechster < offen.length) {
      const commandId = offen[naechster];
      naechster += 1;
      let command = null;
      if (!(state.befehlsstatusOffen instanceof Map)) state.befehlsstatusOffen = new Map();
      state.befehlsstatusOffen.set(commandId, Date.now());
      const abfrageStart = Date.now();
      let abfrageFehler = '';
      try {
        command = await mitBefehlsstatusPlatz(() => bus.getStatus(commandId));
      } catch (error) {
        abfrageFehler = String(error?.message || error).slice(0, 120);
        command = null;
      }
      const lokalMs = Date.now() - abfrageStart;
      if (abfrageFehler) { zaehler.lokalFehler += 1; zaehler.lokalFehlerText ||= abfrageFehler; } else if (!command) zaehler.lokalNull += 1;
      // getStatus liest nur den lokal replizierten Stand. Fehlt der Befehl dort,
      // blieb ein Lead mit gescheitertem Auftrag bis zu 24 h auf "laeuft" (LUZI,
      // 27.09.2026: Auftrag seit 11:19 failed, Lead weiter running). Dann
      // einmal verbindlich beim Server nachfragen.
      // Auch ein lokal vorhandener, aber nicht terminaler Stand kann veraltet
      // sein (Codex-Review: LUZI lokal "running", derselbe Serverbefehl
      // failed/terminal). Dann ebenfalls verbindlich nachfragen, je Befehl
      // hoechstens alle zwei Minuten.
      if (!command || !befehlIstEndgueltig(command)) {
        if (!(state.befehlVerbindlichGelesen instanceof Map)) state.befehlVerbindlichGelesen = new Map();
        const zuletzt = Number(state.befehlVerbindlichGelesen.get(commandId) || 0);
        if (!command || Date.now() - zuletzt > 120_000) {
          state.befehlVerbindlichGelesen.set(commandId, Date.now());
          const frisch = await befehlVerbindlichLesen(commandId, zaehler);
          if (frisch) command = frisch;
        }
      }
      state.befehlsstatusOffen.delete(commandId);
      if (Date.now() - abfrageStart > 5_000 || abfrageFehler) {
        console.info('[olg-abgleich] befehl', JSON.stringify({ id: commandId.slice(-12), lokal_ms: lokalMs, gesamt_ms: Date.now() - abfrageStart, gefunden: Boolean(command), status: command?.status || '', phase: command?.execution_phase || '', fehler: abfrageFehler }));
      }
      if (command) ergebnisse.set(commandId, command);
      const merken = command
        ? (befehlIstEndgueltig(command) ? BEFEHL_ENDGUELTIG_MERKEN_MS : 0)
        : BEFEHL_UNBEKANNT_MERKEN_MS;
      if (merken) befehlsstatusCache.set(commandId, { command, bis: Date.now() + merken });
      else befehlsstatusCache.delete(commandId);
    }
  };
  await Promise.all(Array.from({ length: Math.min(BEFEHLSSTATUS_PARALLEL, offen.length) }, arbeiter));
  zaehler.gefunden = ergebnisse.size;
  zaehler.leerBeispiele = [...new Set(commandIds)].filter((id) => !ergebnisse.has(id)).slice(0, 3);
  if (zaehler.angefragt && zaehler.gefunden < zaehler.angefragt) befehlsstatusDiagnose(zaehler);
  return [...ergebnisse.values()];
}
function befehlsstatusDiagnose(zaehler) {
  // Je Aufrufer gedrosselt, und nur Ergebnisse mit echtem Lesen (nicht die
  // 30-s-Merkliste), damit die Zeile zeigt, was getStatus/Server liefern.
  if (!(zaehler.lokalNull || zaehler.lokalFehler || zaehler.grund)) return;
  if (!(state.befehlsstatusDiagnoseAm instanceof Map)) state.befehlsstatusDiagnoseAm = new Map();
  if (Date.now() - Number(state.befehlsstatusDiagnoseAm.get(zaehler.quelle) || 0) < 60_000) return;
  state.befehlsstatusDiagnoseAm.set(zaehler.quelle, Date.now());
  console.info('[olg-abgleich] befehlsstatus', JSON.stringify(zaehler));
}

async function befehlVerbindlichLesen(commandId, zaehler = {}) {
  const collection = state.ctx?.db?.collection?.('business_commands');
  if (!collection?.find || !commandId) {
    zaehler.serverKeineCollection = Number(zaehler.serverKeineCollection || 0) + 1;
    return null;
  }
  try {
    const docs = await mitBefehlsstatusPlatz(() => collection.find({
      selector: { id: { $eq: commandId } },
      limit: 1,
      requireRevision: `olg-befehl:${commandId}:${Date.now()}`,
    }).exec());
    const doc = (docs || [])[0];
    const gefunden = doc?.toJSON?.() || doc || null;
    if (gefunden) zaehler.serverTreffer = Number(zaehler.serverTreffer || 0) + 1;
    else zaehler.serverNull = Number(zaehler.serverNull || 0) + 1;
    return gefunden;
  } catch (error) {
    zaehler.serverFehlerText ||= String(error?.message || error).slice(0, 160);
    return null;
  }
}

function uniqueCommands(commands = []) {
  const byId = new Map();
  for (const command of commands) {
    const id = String(command?.command_id || command?.id || '').trim();
    if (id) byId.set(id, command);
  }
  return [...byId.values()];
}

// Der Command-Bus kennzeichnet einen Zeitablauf beim Warten selbst als transient
// und wiederholbar (shared/command-bus.js, code 'projection_delayed'). Wir lesen
// diese Kennzeichnung, statt den Text zu vergleichen; der Text bleibt nur als
// letzter Rueckfall, falls ein aelterer Bus die Felder noch nicht mitschickt.
function isTransientResearchWaitError(error) {
  if (!error) return false;
  const code = String(error.code || error.details?.code || '').trim();
  if (code === 'projection_delayed' || code === 'projection_pending') return true;
  const status = String(error.status || error.details?.status || '').trim();
  if (status === 'projection_pending') return true;
  if (error.transient === true || error.details?.transient === true) return true;
  // Die Shell meldet nach 30 s "Die Uebergabe an die Queue wurde ... nicht
  // bestaetigt. Der Auftrag kann trotzdem angenommen worden sein". Am
  // 26.09.2026 kamen 13 von 14 so gemeldeten Starts danach an. Das ist
  // unbestaetigt, nicht gescheitert: der Lead bleibt verfolgbar, kein Dialog.
  if (/nicht best(ä|ae)tigt|kann trotzdem angenommen/i.test(String(error.message || ''))) return true;
  return /wartet noch auf die R/i.test(String(error.message || ''));
}

// Kennung, MIT DER SICH INHALTLICH ETWAS GEAENDERT HAT — bewusst ohne Zeitstempel.
//
// Vorher stand updated_at_ms mit im Schluessel. Auf der Kundeninstanz laeuft eine
// Schreibschleife im nativen Peer, die dieselben sechs Vorgangsdokumente
// unveraendert immer wieder neu schreibt (am 11.08.2026 gemessen: zeitweise ueber
// 100 Revisionen pro Minute, unabhaengig nachgewiesen bei replicationUp=false,
// also voellig ohne Browser). Jedes dieser Neuschreiben hob updated_at_ms an, damit
// aenderte sich der Schluessel, damit galt der Vorgang als neu beobachtet — und der
// Browser schrieb den Lead erneut. Die Anzeige haette die Schleife also zusaetzlich
// angeheizt, sobald jemand das Modul offen laesst.
//
// Der Status bleibt im Schluessel: der Uebergang failed -> completed muss weiterhin
// durchkommen, denn genau darauf beruht das Nachholen verspaeteter Ergebnisse.
// Die Ursache der Schleife selbst liegt im Peer und gehoert nicht hierher; dies ist
// die Bremse auf unserer Seite, keine Behebung.
function researchCommandObservationKey(command) {
  return [
    String(command?.command_id || command?.id || '').trim(),
    String(command?.terminal_status || command?.task_status || command?.status || command?.execution_phase || '').trim(),
  ].join(':');
}

function researchCommandForLead(lead, commands = []) {
  // Recherchen laufen seit Ende August als Chat-Aufgabe. Deren Ausgang kam
  // hier nie an: ein gescheiterter Lauf liess den Lead auf "Läuft" stehen
  // (Kiesow, 10.09.2026 21:48). Die Chat-Aufgabe des AKTUELLEN Auftrags hat
  // Vorrang, solange der Lead auf sie wartet.
  const aktuellerAuftrag = String(lead?.command_id || '').trim();
  if (aktuellerAuftrag && ['queued', 'running'].includes(String(lead?.research_status || ''))) {
    const chatAufgabe = commands.find((command) => command?.command_type === 'business_os.chat.task'
      && String(command.command_id || command.id || '').trim() === aktuellerAuftrag);
    if (chatAufgabe) return chatAufgabe;
  }
  const campaignRunId = String(lead?.payload?.campaign_run_id || '').trim();
  return commands
    .filter((command) => command?.command_type === 'web_stack.person_research')
    .filter((command) => String(command.record_id || '').trim() === String(lead?.id || '').trim())
    .filter((command) => {
      const commandRunId = String(
        command?.payload?.campaign_run_id
        || command?.payload?.workflow_id
        || command?.workflow_id
        || '',
      ).trim();
      return !campaignRunId || !commandRunId || commandRunId === campaignRunId;
    })
    .sort((left, right) => {
      // Ein später fehlgeschlagener oder abgebrochener Lauf darf ein früheres
      // completed-Ergebnis nicht dauerhaft verdecken: sonst gehen die dort
      // gefundenen Kontakte verloren, obwohl die Recherche sie geliefert hat.
      // Rangfolge: aktive Läufe (Live-Status) > completed > failed/cancelled,
      // innerhalb der Stufe entscheidet die Aktualität.
      const tier = (command) => {
        const status = normalizedResearchCommandStatus(command);
        if (['accepted', 'queued', 'running', 'leased', 'retry_wait', 'working'].includes(status)) return 0;
        if (status === 'completed') return 1;
        return 2;
      };
      if (tier(left) !== tier(right)) return tier(left) - tier(right);
      return (
        Number(right.updated_at_ms || right.created_at_ms || 0)
        - Number(left.updated_at_ms || left.created_at_ms || 0)
      );
    })[0] || null;
}

// Ein Feld mit genau einer fundierten Quelle darf der Nutzer bewusst
// freigeben. Die Entscheidung wird als eigener, unabhängiger Beleg mit
// source_id "operator" protokolliert — sichtbar in der Quellenliste, zählbar
// für die Zwei-Quellen-Regel, und nachvollziehbar statt versteckt.
async function approveResearchField(fieldKey) {
  const lead = selectedLead();
  if (!lead || !fieldKey) return;
  // Personenfelder gehoeren zu der Person, die der Personenreiter zeigt. Vorher
  // wurde der Wert der ERSTEN Person freigegeben, ohne Personenbezug, und galt
  // damit fuer alle Personen des Leads (Klicktest P3 REV-04f).
  const istPerson = fieldKey.startsWith('person_');
  const ansicht = istPerson ? contactTabLead(lead) : lead;
  const personKey = istPerson ? personSchluessel(ansicht.contacts?.[0]) : '';
  if (istPerson && !personKey) return;
  const value = researchFieldValue(ansicht, fieldKey);
  if (!value) return;
  const evidence = deduplicateEvidence([...(lead.evidence || []), {
    field_key: fieldKey,
    value,
    confidence: 'operator',
    source_id: 'operator',
    source_url: '',
    tier: 'O',
    via: 'manual-approval',
    label: tr('operatorApproved', 'Vom Nutzer freigegeben'),
    ...(personKey ? { person_key: personKey } : {}),
  }]);
  const draft = { ...lead, evidence };
  const researched = lead.payload?.researched_field_keys || [];
  const payload = istPerson
    ? {
      ...lead.payload,
      operator_approved_person_fields: [...new Set([
        ...(lead.payload?.operator_approved_person_fields || []),
        `${fieldKey}@${personKey}`,
      ])],
    }
    : {
      ...lead.payload,
      operator_approved_field_keys: [...new Set([
        ...(lead.payload?.operator_approved_field_keys || []),
        fieldKey,
      ])],
    };
  const approvedDraft = { ...draft, payload };
  // Eine Personen-Freigabe beweist nichts ueber die anderen Personen; der
  // Lead sprang sonst auf "Geprueft" mit 7 offenen Pruefungen (REV-04d).
  const allProven = !istPerson
    && researched.length > 0
    && researched.every((key) => fieldSourcesSatisfyRule(key, fieldEvidenceSources(approvedDraft, key))
      || operatorAttestedField(approvedDraft, key));
  const research_status = allProven && lead.research_status === 'needs_review'
    ? 'completed'
    : lead.research_status;
  // Die Freigabe wartete bisher auf den vollen Replikationsumlauf, bevor sich in
  // der Ansicht etwas bewegte — bis zu einer Minute Stille nach dem Klick. Der
  // lokale Stand wird deshalb sofort gesetzt und gezeichnet; der Schreibvorgang
  // laeuft dahinter und korrigiert bei Bedarf.
  const eintrag = state.leads.find((entry) => entry.id === lead.id);
  if (eintrag) {
    eintrag.evidence = evidence;
    eintrag.research_status = research_status;
    eintrag.payload = payload;
  }
  renderDetail();
  try {
    await patchLead(lead.id, { evidence, research_status, payload });
  } catch (error) {
    console.warn('[outbound-lead-generation] Freigabe konnte nicht gespeichert werden', error);
    await reload();
    renderDetail();
  }
}

async function validateLead(id) {
  const lead = state.leads.find((entry) => entry.id === id);
  if (!lead) return;
  if (!leadReadyForValidation(lead)) {
    showBusinessAlert(`Freigabe blockiert:\n${validationBlockers(lead).map((blocker) => `• ${blocker}`).join('\n')}`);
    return;
  }
  await patchLead(id, {
    validation_status: 'validated',
    payload: {
      ...lead.payload,
      validated_at_ms: Date.now(),
      validated_by: String(state.ctx?.user?.name || state.ctx?.user?.id || '').trim(),
      admission_decision: 'human_approved',
    },
  });
}

// The detail pane asks this before it enables either Sellify action, so the
// operator sees WHY a handoff is unavailable instead of a dead button. It was
// called at index.js:813 but never defined — every lead selection therefore
// threw a ReferenceError and the pane never rendered.
function sellifyHandoffPrecondition(lead) {
  if (!lead || lead.validation_status !== 'validated') {
    return 'Bitte den Lead vor der Übergabe validieren.';
  }
  // Ohne Auswahl gibt es nichts zu pruefen. Die Sperrpruefung laeuft seit
  // 0.8.4 nur noch fuer Leads MIT ausgewaehlten Personen (sonst stand sie fuer
  // alle 21 Leads in der Schlange). Die Folge war eine Beschriftung, die auf
  // ein Ergebnis wartete, das per Bauart nie kam: "werden noch geprueft",
  // dauerhaft, bei null ausgewaehlten Personen. Am 12.08.2026 auf CHEMOFAST
  // gemessen — vier Uebergabe-Schaltflaechen dauerhaft gesperrt.
  // Ein Zustand ohne Ereignis darf keinen Wartetext bekommen, sondern muss
  // sagen, was der Nutzer zu tun hat.
  if (!normalizeLeadRecipientShape(lead).selected_contact_ids.length) {
    return 'Bitte zuerst mindestens eine Person auswählen.';
  }
  if (wartungAktiv() && (!state.recipientEligibilityReady.has(lead.id) || state.recipientEligibilityTimedOut.has(lead.id))) {
    return 'CTOX wird gerade aktualisiert. Die Sperrvermerke werden danach automatisch geprüft.';
  }
  if (!state.recipientEligibilityReady.has(lead.id)) {
    return 'Die Sellify-Sperrvermerke werden noch geprüft.';
  }
  if (state.recipientEligibilityTimedOut.has(lead.id)) {
    return 'Sellify war gerade nicht erreichbar. Die Sperrvermerke werden in Kürze automatisch erneut geprüft.';
  }
  const plan = buildCampaignRecipientList(lead);
  if (!plan.recipients.length) {
    return plan.excluded.length
      ? 'Alle ausgewählten Personen sind gesperrt oder müssen zuerst geprüft werden.'
      : 'Bitte mindestens eine Person auswählen, bevor der Lead an Sellify übergeben wird.';
  }
  return '';
}

// The recipient checkbox writes through here. It was wired at index.js:960 and
// never defined, so no selection could ever be persisted — which is why picking
// three people still showed zero selected.
async function setContactRecipientSelection(id, contactId, selected) {
  const lead = state.leads.find((entry) => entry.id === id);
  if (!lead || !contactId) return;
  if (!state.recipientEligibilityReady.has(id)) await refreshLeadRecipientEligibility(lead);
  const normalized = normalizeLeadRecipientShape(lead);
  const contact = normalized.contacts.find((entry) => entry.id === contactId);
  if (!contact) return;
  const decision = currentContactEligibility(normalized, contact);
  if (selected && decision.status !== 'free') {
    renderDetail();
    return;
  }
  const selectedIds = new Set(normalized.selected_contact_ids);
  if (selected) selectedIds.add(contactId);
  else selectedIds.delete(contactId);
  const selected_contact_ids = [...selectedIds].filter((entry) => (
    normalized.contacts.some((candidate) => candidate.id === entry
      && currentContactEligibility(normalized, candidate).status === 'free')
  ));
  lead.selected_contact_ids = selected_contact_ids;
  await patchLead(id, { selected_contact_ids });
  renderDetail();
}

// Eine Uebergabe laeuft im Browser. Wird er waehrend der Uebergabe geschlossen,
// blieb der Lead fuer immer auf "Uebergabe laeuft" und beide Knoepfe gesperrt
// (Uebergabetest 25.09.2026). Nach 15 min ohne laufenden Vorgang in diesem
// Browser gilt sie als unterbrochen und darf erneut gestartet werden; die
// Dublettenpruefung erkennt bereits angelegte Firmen und Personen.
const UEBERGABE_UNTERBROCHEN_NACH_MS = 15 * 60_000;
function uebergabeUnterbrochen(lead) {
  if (lead?.sellify_status !== 'queued') return false;
  if (state.sellifyUebergabeVorbereitung?.has?.(lead.id)) return false;
  const start = Number(lead?.payload?.sellify_started_at_ms || 0);
  return !start || Date.now() - start > UEBERGABE_UNTERBROCHEN_NACH_MS;
}

async function sendLeadToSellify(id, { includeCampaign = false } = {}) {
  const lead = state.leads.find((entry) => entry.id === id);
  if (!lead || lead.validation_status !== 'validated') { showBusinessAlert('Bitte den Lead vor der Übergabe validieren.'); return; }
  // Sofortige Rueckmeldung: die Sperrvermerkspruefung vor dem Schreiben dauert
  // gemessen 25-45 s (25.09.2026); bis dahin sah der Klick wie ein toter Knopf aus.
  if (!(state.sellifyUebergabeVorbereitung instanceof Set)) state.sellifyUebergabeVorbereitung = new Set();
  if (state.sellifyUebergabeVorbereitung.has(id)) return;
  state.sellifyUebergabeVorbereitung.add(id);
  renderDetail();
  try {
    await sendLeadToSellifyAusfuehren(lead, id, { includeCampaign });
  } finally {
    state.sellifyUebergabeVorbereitung.delete(id);
    renderDetail();
  }
}

async function sendLeadToSellifyAusfuehren(lead, id, { includeCampaign = false } = {}) {
  // This is the canonical recipient-list boundary. Even a manipulated checkbox
  // state is rechecked against current Sellify person/company remarks before any
  // person or campaign command can be created.
  const decisions = await refreshLeadRecipientEligibility(lead, { force: true });
  const recipientPlan = buildCampaignRecipientList(lead, decisions);
  const selectedContacts = recipientPlan.recipients;
  // "Nicht pruefbar" (Sellify antwortete nicht rechtzeitig) ist kein Sperrurteil:
  // abbrechen, aber die Auswahl NICHT abwaehlen. Vorher wurde die Person bei
  // jeder Zeitueberschreitung abgewaehlt und musste neu gesetzt werden
  // (Uebergabetest 25.09.2026).
  if (recipientPlan.excluded.some(({ decision }) => decision?.pending)) {
    showBusinessAlert('Sellify hat die Sperrvermerke gerade nicht rechtzeitig bestätigt. Es wurde nichts übertragen und die Auswahl bleibt erhalten. Bitte in einer Minute erneut übergeben.');
    return;
  }
  if (recipientPlan.excluded.length) {
    const selected_contact_ids = selectedContacts.map((contact) => contact.id);
    lead.selected_contact_ids = selected_contact_ids;
    state.recipientRemovalNotices.set(id, recipientPlan.excluded);
    await patchLead(id, { selected_contact_ids });
    renderCenter();
    renderDetail();
  }
  if (!selectedContacts.length) {
    showBusinessAlert(recipientPlan.excluded.length
      ? 'Die ausgewählten Personen sind gesperrt oder müssen zuerst geprüft werden und wurden abgewählt.'
      : 'Bitte mindestens eine Person auswählen, bevor der Lead an Sellify übergeben wird.');
    return;
  }
  // Gehoert der Lead mehreren Kampagnen an, zaehlt die Kampagne, in der
  // gerade gearbeitet wird (Mitgliedschaften seit 26.09.2026).
  const gewaehlt = String(state.selectedCampaign || '').trim();
  const campaignName = (gewaehlt && leadKampagnen(lead).includes(gewaehlt) ? gewaehlt : String(lead.campaign || '')).trim();
  if (includeCampaign && !campaignName) {
    showBusinessAlert('Der Lead hat keine Kampagne. Ohne Kampagnenname kann er nicht als Kampagne übertragen werden.');
    return;
  }
  const workflowId = `leadgen-sellify-${crypto.randomUUID()}`;
  const prompt = `Übergebe den validierten Lead ${lead.name} kontrolliert an Sellify. Prüfe Dubletten, schreibe ausschließlich über typisierte SQL-Operationen und bestätige jeden Datensatz durch den synchronisierten Readback.`;
  await patchLead(id, {
    sellify_status: 'queued',
    command_id: workflowId,
    payload: { ...lead.payload, sellify_started_at_ms: Date.now() },
  });
  try {
    const duplicate = await findSellifyCompanyDuplicate(lead);
    let company = duplicate
      ? await updateSellifyCompany(duplicate, lead, workflowId, prompt)
      : await createSellifyCompany(lead, workflowId, prompt);
    // create+update im selben Lauf: die Version nach create kommt aus returned_rows
    // und wird hier an den zweiten Schreibvorgang weitergereicht. Ohne das
    // kollidiert die Uebergabe mit sich selbst.
    if (!duplicate) {
      company = await updateSellifyCompany(company, lead, workflowId, prompt, {
        expectedSourceVersion: Number(company.authoritative_source_version) || 0,
      });
    }
    const personIds = [];
    for (const contact of selectedContacts) {
      // Eine persoenliche Adresse geht nur nach Sellify, wenn ein externes
      // Zitat genau diese Adresse nennt (Owner-Regel 23.09.2026, BNT: Robert
      // Suesses Adresse stammte nur aus Sellify selbst).
      const person = await upsertSellifyPerson(company, kontaktEmailBelegt(lead, contact)
        ? contact
        : { ...contact, email: '', person_email: '' }, workflowId, prompt);
      if (person?.person_id) personIds.push(person.person_id);
    }
    // Sellify models a campaign per contact and person, so each selected
    // recipient joins the campaign in its own typed write. Only after the
    // organisation and its people exist — a campaign row pointing at a person
    // Sellify does not know is rejected by the source itself.
    const campaignIds = [];
    if (includeCampaign) {
      for (const personId of personIds) {
        const campaignId = await addSellifyCampaignMember(
          company, personId, campaignName, workflowId, prompt,
        );
        if (campaignId) campaignIds.push(campaignId);
      }
      if (!campaignIds.length) throw new Error('Sellify hat keine Kampagnen-Zuordnung bestätigt.');
    }
    await patchLead(id, {
      sellify_status: 'completed',
      command_id: workflowId,
      payload: {
        ...lead.payload,
        // Ein alter Fehlertext neben einem frischen Erfolgsstatus ist keine
        // Kosmetik, sondern eine MESSFALLE: am 12.08.2026 stand
        // sellify_status=completed und daneben unveraendert "Die
        // Sellify-Dublettenpruefung ist nicht eindeutig" von 00:02:28. Das hat in
        // dieser Nacht zwei Sitzungen in die falsche Richtung geschickt — man
        // liest den Fehler und haelt den Vorgang fuer gescheitert, obwohl er
        // durchlief. Wer erfolgreich ist, raeumt seine Fehlerspur weg.
        sellify_error: '',
        sellify_finished_at_ms: Date.now(),
        sellify_company_id: company.id,
        sellify_contact_id: company.contact_id,
        sellify_person_ids: personIds,
        sellify_campaign_ids: campaignIds,
        sellify_campaign_name: includeCampaign ? campaignName : '',
        sellify_deduplication: duplicate ? 'updated_existing' : 'created_new',
      },
    });
  } catch (error) {
    // Eine ausstehende Ruecksynchronisierung ist kein Fehlschlag: Sellify hat
    // geschrieben, nur die lokale Kopie hinkt hinterher. Diesen Zustand als
    // "gescheitert" zu melden hat den Nutzer wiederholt in die Irre gefuehrt.
    const pending = error?.readbackPending === true;
    await patchLead(id, {
      sellify_status: pending ? 'pending_readback' : 'failed',
      command_id: workflowId,
      payload: {
        ...lead.payload,
        [pending ? 'sellify_pending_reason' : 'sellify_error']: String(error?.message || error),
        sellify_finished_at_ms: Date.now(),
      },
    });
    showBusinessAlert(pending
      ? `${String(error?.message || error)} Der Lead wird als „Übergeben, Bestätigung ausstehend“ geführt.`
      : uebergabeFehlerText(error));
  }
}

// Technische Abbruchcodes in eine Handlungsanweisung uebersetzen; der Rohtext
// bleibt in Klammern fuer die Fehlersuche. "QUERY_CANCELLED: peer-peer-not-open"
// allein sagte dem Nutzer nicht, dass ein erneuter Klick sicher ist.
function uebergabeFehlerText(error) {
  const roh = String(error?.message || error || 'Unbekannter Fehler');
  if (/peer-not-open|peer-peer|QUERY_CANCELLED|webrtc|RATE_LIMITED|timed? ?out|nicht innerhalb|Rückmeldung steht noch aus/i.test(roh)) {
    return `Die Verbindung zu CTOX war während der Übergabe kurz unterbrochen. Bitte erneut übergeben: bereits in Sellify angelegte Firmen und Personen werden erkannt und nicht doppelt angelegt.\n\n(${roh})`;
  }
  if (/wird aktualisiert|schreibgesch|read-?only|MAINTENANCE/i.test(roh)) {
    return `CTOX wird gerade aktualisiert. Bitte nach dem Update erneut übergeben.\n\n(${roh})`;
  }
  return roh;
}


// Namensvarianten, die ein CRM ueblicherweise fuehrt — als GEZIELTE Abfragen.
//
// Der Lead heisst "CHEMOFAST Anchoring GmbH", die CRM-Organisation
// "CHEMOFAST® Anchoring GmbH". Ein normalisierter Vergleich braeuchte alle 17.520
// Organisationen; ueber die Bedarfsabfrage geladen friert das die Seite ein (am
// 12.08.2026 gemessen und wieder zurueckgenommen). Statt zu scannen leiten wir
// aus dem Leadnamen wenige plausible Schreibweisen ab und fragen sie einzeln
// exakt ab. Das kostet eine Handvoll Punktabfragen, unabhaengig davon, wie gross
// das CRM ist.
// Rechtsformfreier Namenskern fuer die unscharfe CRM-Suche ("BNT Chemicals
// GmbH" -> "BNT Chemicals"). Spiegel der nativen Logik.
function firmenKernName(name) {
  const rechtsformen = new Set([
    'gmbh', 'mbh', 'ag', 'se', 'kg', 'kgaa', 'ohg', 'ug', 'co', 'cokg', 'ev',
    'eg', 'inc', 'ltd', 'llc', 'sa', 'srl', 'bv', 'nv',
  ]);
  const kern = String(name || '')
    .split(/\s+/)
    .filter((wort) => {
      const norm = wort.replace(/[^\p{L}\p{N}]/gu, '').toLowerCase();
      return norm && !rechtsformen.has(norm);
    })
    .join(' ')
    .trim();
  return kern.length >= 3 ? kern : '';
}

function firmenNamensvarianten(name) {
  const roh = String(name || '').trim();
  if (!roh) return [];
  const varianten = new Set([roh]);
  const woerter = roh.split(/\s+/);
  // Schutzzeichen direkt hinter einem der vorderen Woerter — so fuehren CRMs
  // Marken ueblicherweise: "CHEMOFAST® Anchoring GmbH".
  for (const zeichen of ['®', '™']) {
    for (let i = 0; i < Math.min(woerter.length, 3); i += 1) {
      const kopie = [...woerter];
      kopie[i] = `${kopie[i]}${zeichen}`;
      varianten.add(kopie.join(' '));
      const mitLeerraum = [...woerter];
      mitLeerraum.splice(i + 1, 0, zeichen);
      varianten.add(mitLeerraum.join(' '));
    }
  }
  varianten.delete(roh);
  return [...varianten];
}

// Lokale Dublettensuche ueber das replizierte sellify_companies. Gibt
// `undefined` zurueck, wenn das Replikat (noch) nicht nutzbar ist — dann
// entscheidet der Kanalpfad. `null` heisst: sicher NICHT im CRM.
async function findSellifyCompanyDuplicateLokal(lead) {
  const collection = state.sellifyCompaniesLocal;
  if (!collection) return undefined;
  let repliziert = 0;
  try {
    const stichprobe = await collection.find({ limit: 1 }).exec();
    repliziert = stichprobe.length;
  } catch (error) {
    console.warn('[olg-sellify-lokal] Replikat nicht lesbar', error);
    return undefined;
  }
  if (!repliziert) {
    console.info('[olg-sellify-lokal] Replikat noch leer — Kanalpfad entscheidet');
    return undefined;
  }
  const domain = normalizedDomain(lead.website || lead.domain || lead.data?.website || '');
  const selektoren = [
    { name: { $eq: String(lead.name || '').trim() } },
    ...firmenNamensvarianten(lead.name).map((variante) => ({ name: { $eq: variante } })),
    ...(domain ? [
      `https://www.${domain}`, `https://${domain}`, `http://www.${domain}`,
      `http://${domain}`, `www.${domain}`, domain,
    ].map((kandidat) => ({ website_url: { $eq: kandidat } })) : []),
  ];
  for (const selector of selektoren) {
    const t0 = Date.now();
    try {
      const treffer = await collection.find({ selector }).exec();
      const lebend = treffer
        .map((doc) => (typeof doc?.toJSON === 'function' ? doc.toJSON() : doc))
        .filter((entry) => entry && !entry.is_deleted);
      if (lebend.length) {
        console.info('[olg-sellify-lokal] Treffer', lebend.length, 'ms', Date.now() - t0);
        return waehleGepflegtesteFirma(lebend, lead, domain);
      }
    } catch (error) {
      console.warn('[olg-sellify-lokal] Abfrage fehlgeschlagen', error);
      return undefined;
    }
  }
  console.info('[olg-sellify-lokal] kein Treffer — Firma nicht im CRM');
  return null;
}

// Gemeinsame Auswahllogik: bei mehreren CRM-Treffern gewinnt der Datensatz
// mit den meisten belastbaren Angaben, dann die kleinere contact_id.
function waehleGepflegtesteFirma(kandidatenListe, lead, domain) {
  if (kandidatenListe.length <= 1) return kandidatenListe[0] || null;
  const postalCode = String(lead.data?.postal_code || lead.data?.plz || '').trim();
  const city = String(lead.city || lead.data?.city || lead.data?.ort || '').trim().toLowerCase();
  const strong = kandidatenListe.filter((entry) => {
    const sameDomain = domain && normalizedDomain(entry.website_url) === domain;
    const sameAddress = postalCode && city
      && String(entry.postal_code || '').trim() === postalCode
      && String(entry.city || '').trim().toLowerCase() === city;
    return sameDomain || sameAddress;
  });
  if (strong.length === 1) return strong[0];
  const kandidaten = strong.length ? strong : kandidatenListe;
  const gehalt = (firma) => [
    firma?.street || firma?.address, firma?.postal_code, firma?.city,
    firma?.website_url, firma?.phone, firma?.industry,
  ].filter((wert) => String(wert || '').trim()).length;
  const sortiert = [...kandidaten].sort((a, b) => (
    gehalt(b) - gehalt(a) || (Number(a?.contact_id) || 0) - (Number(b?.contact_id) || 0)
  ));
  return sortiert[0] || null;
}

async function findSellifyCompanyDuplicate(lead) {
  // Schnellster Weg: das lokal replizierte Firmenverzeichnis. Punktabfragen
  // laufen dort in Millisekunden und haengen an keiner Netzleitung.
  // Ein Treffer im lokalen Replikat gilt. Ein FEHLENDER Treffer beweist nichts:
  // das Replikat haelt nur die Seiten, die die Sellify-App gerade geladen hat,
  // und vergleicht Namen exakt. Am 11.09.2026 galt "Weicon GmbH & Co. KG"
  // deshalb als neu, obwohl Sellify "WEICON GmbH & Co. KG" (5661) fuehrt - die
  // Uebergabe legte eine Dublette an. Ohne lokalen Treffer entscheidet die
  // serverseitige Suche (exakt, dann unscharf ohne Gross-/Kleinschreibung).
  const lokal = await findSellifyCompanyDuplicateLokal(lead);
  if (lokal) return lokal;
  const collection = sellifyReadCollection('company');
  // WENIGE serielle Punktabfragen, Prioritaet: exakter Name, beste
  // Namensvariante, kanonische Domain-Schreibweise. Der parallele Ansatz
  // saturierte den Query-Kanal des Browsers (STREAM_LIMIT_EXCEEDED) und
  // liess damit ALLE Sellify-Abfragen verhungern.
  const domain = normalizedDomain(lead.website || lead.domain || lead.data?.website || '');
  // Unter verstopften Sync-Kanaelen kostet JEDE Probe bis zu ~50s — deshalb
  // nur zwei: der exakte Name und die unscharfe Kernnamen-Suche (die faengt
  // Varianten, Umfirmierungen und Schutzzeichen gleich mit).
  const probes = [
    { name: 'exakt', selector: { name: { $eq: String(lead.name || '').trim() } } },
    ...(firmenKernName(lead.name) ? [{ name: 'fuzzy', fuzzy: firmenKernName(lead.name) }] : []),
  ];
  let active = [];
  for (const probe of probes) {
    const t0 = Date.now();
    try {
      let treffer;
      if (probe.fuzzy) {
        const antwort = await sellifyNativeLookup({
          entity: 'company',
          fuzzy_selectors: [{ field: 'name', value: probe.fuzzy }],
          limit: 10,
        });
        treffer = Array.isArray(antwort?.records) ? antwort.records : [];
      } else {
        treffer = await collection.find({ selector: probe.selector }).exec();
      }
      const lebend = treffer.filter((entry) => !entry.is_deleted);
      console.info('[olg-sellify-probe]', probe.name, 'hits', lebend.length, 'ms', Date.now() - t0);
      if (lebend.length) { active = lebend; break; }
    } catch (error) {
      console.warn('[olg-sellify-probe]', probe.name, 'FEHLER', String(error?.message || error).slice(0, 160), 'ms', Date.now() - t0);
    }
  }
  if (active.length <= 1) return active[0] || null;
  const postalCode = String(lead.data?.postal_code || lead.data?.plz || '').trim();
  const city = String(lead.city || lead.data?.city || lead.data?.ort || '').trim().toLowerCase();
  const strong = active.filter((entry) => {
    const sameDomain = domain && normalizedDomain(entry.website_url) === domain;
    const sameAddress = postalCode && city
      && String(entry.postal_code || '').trim() === postalCode
      && String(entry.city || '').trim().toLowerCase() === city;
    return sameDomain || sameAddress;
  });
  if (strong.length === 1) return strong[0];
  // Mehrere Treffer sind im CRM der Normalfall, nicht die Ausnahme: CHEMOFAST
  // wird unter ZWEI contact_ids gefuehrt (17714 und 18255), nur 17714 traegt die
  // Ansprechpartner mit ihren Adressen. Bis zum 12.08.2026 warf die Pruefung
  // hier und brach damit die gesamte Uebergabe ab — der Nutzer sah "Die
  // Sellify-Dublettenpruefung ist nicht eindeutig" und hatte keinen Weg weiter.
  //
  // Statt abzubrechen waehlen wir den GEPFLEGTESTEN Datensatz: den mit den
  // meisten belastbaren Angaben. Das ist genau der, an dem die Ansprechpartner
  // haengen, und damit der, den ein Mensch auch nehmen wuerde. Bleibt es
  // gleichstaendig, entscheidet die kleinere contact_id — der aeltere, historisch
  // gewachsene Eintrag. Die Entscheidung ist damit reproduzierbar statt zufaellig.
  const kandidaten = strong.length ? strong : active;
  const gehalt = (firma) => [
    firma?.street || firma?.address, firma?.postal_code, firma?.city,
    firma?.website_url, firma?.phone, firma?.industry,
  ].filter((wert) => String(wert || '').trim()).length;
  const sortiert = [...kandidaten].sort((a, b) => (
    gehalt(b) - gehalt(a) || (Number(a?.contact_id) || 0) - (Number(b?.contact_id) || 0)
  ));
  return sortiert[0] || null;
}

// Das CRM ist die erste Quelle, nicht die letzte Ablage.
//
// Bis zum 11.08.2026 wurde Sellify vor einer Recherche nur gefragt, OB die Firma
// existiert — die dort gefuehrten Ansprechpartner (im Schnitt 3,5 je Firma, in
// diesem Mandanten 60.639 Personen zu 17.516 Firmen) blieben unbeachtet. Der
// Auftrag ging dann los und liess dieselben Namen, Anschriften und Telefonnummern
// im offenen Netz neu zusammensuchen, die zwei Handgriffe entfernt schon
// vorlagen. Das ist nicht nur Verschwendung: extern Erratenes ist schlechter
// belegt als ein gepflegter CRM-Eintrag.
//
// Was hier eingesammelt wird, geht als Vorwissen in den Auftrag. Es ersetzt die
// externe Pruefung nicht — der Zwei-Quellen-Nachweis bleibt unangetastet —, aber
// die Recherche weiss ab jetzt, was das Haus bereits kennt.
async function sellifyVorwissen(lead) {
  let firma = null;
  try {
    firma = await findSellifyCompanyDuplicate(lead);
  } catch (error) {
    merkeSellifyProjektion(false, lead, null);
    throw error;
  }
  merkeSellifyProjektion(true, lead, firma);
  if (!firma) return null;
  let personen = [];
  try {
    const gefunden = await sellifyReadCollection('person')
      .find({ selector: { contact_id: { $eq: firma.contact_id } } })
      .exec();
    personen = gefunden
      .filter((entry) => !entry.is_deleted)
      .map((entry) => ({
        id: String(entry.id || entry.person_id || '').trim(),
        vorname: String(entry.first_name || '').trim(),
        nachname: String(entry.last_name || '').trim(),
        // Ausgeschiedene bleiben im Vorwissen (sonst legt die Recherche sie aus
        // dem Netz neu an), tragen die Marke aber in der Funktion - so sieht sie
        // der Worker, und der Kern uebernimmt sie an den Kontakt.
        funktion: [sellifyDeutsch(entry.position || entry.function || ''), istAusgeschieden(entry) ? '(ausgeschieden)' : '']
          .filter(Boolean).join(' '),
        email: String(entry.email || '').trim(),
        telefon: String(entry.phone || entry.telephone || '').trim(),
      }))
      .filter((p) => p.vorname || p.nachname);
  } catch (error) {
    // Eine Firma ohne lesbare Kontakte ist immer noch Vorwissen.
    console.warn('[outbound-lead-generation] Sellify-Kontakte nicht lesbar', error);
  }
  return sellifyVorwissenAusFirma(firma, personen);
}

function sellifyVorwissenAusFirma(firma, personen = []) {
  return {
    contact_id: firma.contact_id,
    name: String(firma.name || '').trim(),
    // Der Stammsatz fuehrt die Strasse als address_line; street/address lasen
    // bisher ins Leere, die Anschrift kam nie beim Worker an.
    anschrift: String(firma.address_line || firma.street || firma.address || '').trim(),
    plz: String(firma.postal_code || '').trim(),
    ort: String(firma.city || '').trim(),
    land: sellifyLaendercode(firma.country),
    domain: normalizedDomain(firma.website_url || ''),
    email: String(firma.email || '').trim(),
    telefon: String(firma.phone || '').trim(),
    fax: String(firma.fax || '').trim(),
    wz_code: String(firma.wz_code || '').trim(),
    mitarbeiter: sellifyZahl(firma.employees),
    umsatz: sellifyZahl(firma.revenue_mio) ? `${sellifyZahl(firma.revenue_mio)} Mio. €` : '',
    personen,
  };
}

// Sellify fuehrt Texte mehrsprachig: 'GE:"Einkaufsleitung";US:"Head of
// procurement"'. Der Worker zitierte "Einkaufsleitung", der Server fand im
// Rohtext keinen Treffer, und die Empfaengerliste zeigte den Rohtext.
function sellifyDeutsch(value) {
  const text = String(value || '').trim();
  const deutsch = text.match(/GE:"([^"]*)"/)?.[1];
  if (deutsch !== undefined) return deutsch.trim();
  return text.match(/US:"([^"]*)"/)?.[1]?.trim() ?? text;
}
// Sellify fuehrt das Land als 'GE:"Deutschland";US:"Germany";BA:"DE"'.
function sellifyLaendercode(value) {
  const text = String(value || '');
  const code = text.match(/BA:"([A-Z]{2})"/)?.[1] || (/^[A-Z]{2}$/.test(text.trim()) ? text.trim() : '');
  return code;
}
function sellifyZahl(value) {
  const number = Number(String(value ?? '').replace(',', '.'));
  if (!Number.isFinite(number) || number <= 0) return '';
  return String(Math.round(number * 100) / 100);
}

// Der Sellify-Stammsatz, wie ihn der Vorabgleich gefunden hat. Er ist nach
// Rechercheablauf Regel 0 der Ausgangswert jedes Feldes und zugleich eine
// Quelle. Bis 10.09.2026 wurde er nach dem Start verworfen: der Worker bekam
// nur die Personen, die Feldansicht nie eine Quelle "Sellify".
function sellifySnapshotAusVorwissen(vorwissen) {
  if (!vorwissen) return null;
  const { personen, ...stammsatz } = vorwissen;
  return { ...stammsatz, contact_id: String(vorwissen.contact_id || ''), checked_at_ms: Date.now() };
}

function sellifyVorwissenAlsText(vorwissen) {
  if (!vorwissen) return '';
  const zeilen = [
    'BEKANNT AUS DEM EIGENEN CRM (Sellify) — diese Angaben sind gepflegt und haben Vorrang',
    'vor extern Gefundenem. Pruefe sie, widerlege sie wenn noetig, aber erfinde sie nicht neu:',
    `- Organisation: ${vorwissen.name}${vorwissen.contact_id ? ` [contact_id ${vorwissen.contact_id}]` : ''}`,
  ];
  if (vorwissen.anschrift || vorwissen.plz || vorwissen.ort) {
    zeilen.push(`- Anschrift: ${[vorwissen.anschrift, [vorwissen.plz, vorwissen.ort].filter(Boolean).join(' ')].filter(Boolean).join(', ')}`);
  }
  if (vorwissen.domain) zeilen.push(`- Domain: ${vorwissen.domain}`);
  if (vorwissen.telefon) zeilen.push(`- Telefon: ${vorwissen.telefon}`);
  if (vorwissen.fax) zeilen.push(`- Fax: ${vorwissen.fax}`);
  if (vorwissen.email) zeilen.push(`- E-Mail: ${vorwissen.email}`);
  if (vorwissen.wz_code) zeilen.push(`- WZ-Code: ${vorwissen.wz_code}`);
  if (vorwissen.mitarbeiter) zeilen.push(`- Mitarbeiter: ${vorwissen.mitarbeiter}`);
  if (vorwissen.umsatz) zeilen.push(`- Umsatz: ${vorwissen.umsatz}`);
  if (vorwissen.personen.length) {
    zeilen.push(`- Bereits gefuehrte Ansprechpartner (${vorwissen.personen.length}):`);
    for (const p of vorwissen.personen.slice(0, 25)) {
      const teile = [[p.vorname, p.nachname].filter(Boolean).join(' '), p.funktion, p.email, p.telefon].filter(Boolean);
      zeilen.push(`  · ${teile.join(' | ')}${p.id ? ` [${p.id}]` : ''}`);
    }
    zeilen.push('  Diese Personen NICHT erneut erraten. Ergaenze fehlende Angaben und suche zusaetzliche Ansprechpartner.');
  } else {
    zeilen.push('- Im CRM sind zu dieser Organisation noch keine Ansprechpartner gefuehrt.');
  }
  // Sasol 11.09.2026: drei Laeufe, kein einziger Sellify-Beleg - WZ-Code und
  // Umsatz blieben no_match, obwohl Sellify sie fuehrt und der Server die
  // Zitate seit 12509a3ad annimmt. Das Format steht jetzt dort, wo der Worker
  // den Bestand liest, nicht nur tief im Skill.
  if (vorwissen.contact_id) {
    zeilen.push(
      'SELLIFY ALS BELEG: Jeder Wert oben ist eine Quelle. Stimmt dein Wert mit Sellify ueberein und bestaetigt',
      '  EINE externe Quelle ihn, melde das Feld verified mit dieser einen externen Quelle - der Server',
      `  ergaenzt Sellify (sellify://company/${vorwissen.contact_id}) selbst als zweite Quelle. Nie no_match wegen "nur einer Quelle".`,
      '  Fuer Personen: person_key = die Sellify-id in eckigen Klammern. Sellify allein belegt nichts.',
    );
  }
  return zeilen.join('\n');
}

function sellifyVorwissenAlsPersonRecords(vorwissen) {
  return (vorwissen?.personen || []).slice(0, 100).map((person) => ({
    sellify_person_id: String(person.id || '').trim(),
    person_vorname: String(person.vorname || '').trim(),
    person_nachname: String(person.nachname || '').trim(),
    person_funktion: String(person.funktion || '').trim(),
    person_position: String(person.funktion || '').trim(),
    person_email: String(person.email || '').trim(),
    person_telefon: String(person.telefon || '').trim(),
  })).filter((person) => person.person_vorname || person.person_nachname);
}

async function createSellifyCompany(lead, workflowId, prompt) {
  const values = sellifyCompanyValues(lead);
  const result = await dispatchExternalSqlWrite('company_create', lead.id, values, workflowId, prompt);
  const contactId = returnedInteger(result, 'contact_id');
  if (!contactId) throw new Error('Sellify hat keine Organisations-ID bestätigt.');
  // Die Quellversion nach dem Create kommt aus der massgeblichen SQL-Antwort
  // (returned_rows.source_version), nicht aus der nachhinkenden Browserprojektion.
  // So kann der anschliessende company_update mit der echten Version starten.
  const authoritativeSourceVersion = returnedSourceVersion(result);
  const projected = await waitForProjectedRecord(
    sellifyReadCollection('company'),
    `sellify-company-${contactId}`,
    (entry) => entry.contact_id === contactId && entry.name === values.name,
  );
  return withAuthoritativeSourceVersion(
    projected,
    authoritativeSourceVersion || Number(projected.updated_at_ms) || 0,
  );
}

async function updateSellifyCompany(company, lead, workflowId, prompt, options = {}) {
  const values = sellifyCompanyValues(lead);
  const mutableFields = ['name', 'short_name', 'number1', 'number2', 'email', 'phone', 'fax', 'website_url', 'address_line', 'postal_code', 'city'];
  const changedFields = mutableFields.filter((field) => field === 'name' || values[field] !== '');
  const expectedSourceVersion = await resolveExpectedSourceVersion({
    kind: 'company',
    entityId: company.contact_id,
    hintedVersion: options.expectedSourceVersion,
    record: company,
    workflowId,
    prompt,
  });
  const patch = {
    contact_id: company.contact_id,
    expected_source_version: expectedSourceVersion,
    changed_fields: changedFields,
    ...values,
    ...sellifyCommunicationIds(company),
  };
  delete patch.country_code;
  const result = await dispatchExternalSqlWriteWithSourceVersion(
    'company_update',
    company.id,
    patch,
    workflowId,
    prompt,
    { kind: 'company', entityId: company.contact_id },
  );
  const nextVersion = returnedSourceVersion(result);
  // Die Recherche fuehrt Kennzahlen als Text ("70 Mio. €", "rund 1.500
  // Mitarbeitende"); die Uebergabe las nur die alten Zahlschluessel und schrieb
  // deshalb 0 Mio. Umsatz nach Sellify (Weicon, 11.09.2026).
  // Kennzahlen nur, wenn belegt: "30 Vollzeitmitarbeiter" stand bei Carbosulf
  // als "offen – Prüfung nötig" und waere trotzdem nach Sellify gegangen.
  const employees = finiteNumber(firstValue(lead.data, ['employees']))
    || metricOrZero(numericBusinessMetric(belegterWert(lead, 'mitarbeiter')));
  const revenueMio = finiteNumber(firstValue(lead.data, ['revenue_mio', 'umsatz_mio']))
    || metricOrZero(numericBusinessMetric(belegterWert(lead, 'umsatz'), { millionScale: true }));
  if (employees || revenueMio) {
    await dispatchExternalSqlWrite('company_metrics_update', company.id, {
      contact_id: company.contact_id,
      cust_contact_id: Number(company.payload?.sql?.cust_contact_id) || undefined,
      employees,
      revenue_mio: revenueMio,
    }, workflowId, prompt);
  }
  const wzCode = belegterWert(lead, 'wz_code', ['wzcode']);
  if (wzCode) {
    await dispatchExternalSqlWrite('company_wz_update', company.id, {
      contact_id: company.contact_id,
      contact_wzcode_id: Number(company.payload?.sql?.contact_wzcode_id) || undefined,
      wz_code: wzCode,
    }, workflowId, prompt);
  }
  const projected = await waitForProjectedRecord(
    sellifyReadCollection('company'),
    company.id,
    (entry) => entry.contact_id === company.contact_id && entry.name === values.name,
  );
  return withAuthoritativeSourceVersion(
    projected,
    nextVersion || Number(projected.updated_at_ms) || expectedSourceVersion,
  );
}

// Adds one selected recipient to the Sellify campaign named after the lead's
// campaign. `campaign_create` is the source's own typed operation (it creates
// the selection and its member in one transaction and is idempotent through the
// write receipt), so no SQL is written from here directly.
async function addSellifyCampaignMember(company, personId, campaignName, workflowId, prompt) {
  const result = await dispatchExternalSqlWrite('campaign_create', `${company.id}:${personId}`, {
    contact_id: company.contact_id,
    person_id: personId,
    name: campaignName,
    note_text: '',
    // Sellify classifies a selection by the table it targets; 0 keeps the
    // source's own default instead of inventing a classification here.
    target_table_number: 0,
    search_category_id: 0,
    group_id: 0,
  }, workflowId, prompt);
  return returnedInteger(result, 'selection_id') || returnedInteger(result, 'selectionmember_id') || 0;
}

async function upsertSellifyPerson(company, contact, workflowId, prompt) {
  const values = sellifyPersonValues(contact);
  if (!values.first_name && !values.last_name) return null;
  const existing = await findSellifyPersonDuplicate(company.contact_id, values);
  if (!existing) {
    const result = await dispatchExternalSqlWrite('person_create', company.id, {
      contact_id: company.contact_id,
      ...values,
    }, workflowId, prompt);
    const personId = returnedInteger(result, 'person_id');
    if (!personId) throw new Error(`Sellify hat für ${[values.first_name, values.last_name].filter(Boolean).join(' ')} keine Personen-ID bestätigt.`);
    const authoritativeSourceVersion = returnedSourceVersion(result);
    const created = await waitForProjectedRecord(
      sellifyReadCollection('person'),
      `sellify-person-${personId}`,
      (entry) => entry.person_id === personId && entry.contact_id === company.contact_id,
    );
    return updateSellifyPersonCommunication(
      withAuthoritativeSourceVersion(
        created,
        authoritativeSourceVersion || Number(created.updated_at_ms) || 0,
      ),
      values,
      workflowId,
      prompt,
      { expectedSourceVersion: authoritativeSourceVersion },
    );
  }
  return updateSellifyPersonCommunication(existing, values, workflowId, prompt);
}

async function findSellifyPersonDuplicate(contactId, values) {
  const matches = values.email
    ? await sellifyReadCollection('person').find({ selector: { email: { $eq: values.email } } }).exec()
    : await sellifyReadCollection('person').find({ selector: { contact_id: { $eq: contactId } } }).exec();
  return matches.find((entry) => !entry.is_deleted
    && entry.contact_id === contactId
    && (values.email
      ? String(entry.email || '').trim().toLowerCase() === values.email.toLowerCase()
      : String(entry.first_name || '').trim().toLowerCase() === values.first_name.toLowerCase()
        && String(entry.last_name || '').trim().toLowerCase() === values.last_name.toLowerCase())) || null;
}

async function updateSellifyPersonCommunication(person, values, workflowId, prompt, options = {}) {
  const changedFields = ['salutation', 'title', 'first_name', 'last_name', 'department', 'function', 'number', 'email', 'phone', 'mobile', 'social_media']
    .filter((field) => values[field] !== '');
  if (!changedFields.length) return person;
  const expectedSourceVersion = await resolveExpectedSourceVersion({
    kind: 'person',
    entityId: person.person_id,
    hintedVersion: options.expectedSourceVersion,
    record: person,
    workflowId,
    prompt,
  });
  const result = await dispatchExternalSqlWriteWithSourceVersion(
    'person_update',
    person.id,
    {
      person_id: person.person_id,
      contact_id: person.contact_id,
      expected_source_version: expectedSourceVersion,
      changed_fields: changedFields,
      ...values,
      ...sellifyCommunicationIds(person),
    },
    workflowId,
    prompt,
    { kind: 'person', entityId: person.person_id },
  );
  const nextVersion = returnedSourceVersion(result);
  const projected = await waitForProjectedRecord(
    sellifyReadCollection('person'),
    person.id,
    (entry) => entry.person_id === person.person_id
      && (!values.email || String(entry.email || '').trim().toLowerCase() === values.email.toLowerCase()),
  );
  return withAuthoritativeSourceVersion(
    projected,
    nextVersion || Number(projected.updated_at_ms) || expectedSourceVersion,
  );
}

function isSourceVersionConflict(error) {
  return /source version changed/i.test(String(error?.message || error || ''));
}

// Die erwartete Quellversion darf NICHT aus der nachhinkenden Browserprojektion
// aus dem serverseitigen Sellify-Lesezugriff kommen. Maßgeblich ist die SQL-Quelle:
// 1. vom vorherigen Schreibvorgang in returned_rows weitergereicht,
// 2. optional per company_source_version / person_source_version gelesen,
// 3. erst dann die Projektion als Notbehelf.
//
// Bei einem Versionskonflikt wird die Version EINMAL aus der massgeblichen
// Quelle gelesen und der Schreibvorgang mit dieser Version wiederholt. Scheitert
// auch das, ist es eine echte Fremdaenderung und wird als solche gemeldet.
// Die Sperre bleibt; blinde Wiederholungen und Projektions-Warte-Reparaturen
// sind absichtlich entfernt.
function withAuthoritativeSourceVersion(record, version) {
  const authoritative = Number(version) || 0;
  if (!record || !authoritative) return record;
  return { ...record, authoritative_source_version: authoritative };
}

async function resolveExpectedSourceVersion({
  kind, entityId, hintedVersion, record, workflowId, prompt,
}) {
  const hinted = Number(hintedVersion) || 0;
  if (hinted > 0) return hinted;
  const carried = Number(record?.authoritative_source_version) || 0;
  if (carried > 0) return carried;
  const authoritative = await fetchAuthoritativeSourceVersion(kind, entityId, workflowId, prompt);
  if (authoritative > 0) return authoritative;
  return Number(record?.updated_at_ms) || 0;
}

async function fetchAuthoritativeSourceVersion(kind, entityId, workflowId, prompt) {
  const id = Number(entityId) || 0;
  if (!id) return 0;
  const operationId = kind === 'person' ? 'person_source_version' : 'company_source_version';
  const idField = kind === 'person' ? 'person_id' : 'contact_id';
  try {
    const result = await dispatchExternalSqlWrite(
      operationId,
      String(id),
      { [idField]: id },
      workflowId,
      prompt,
    );
    return returnedSourceVersion(result);
  } catch {
    // Operation noch nicht auf dem Mandanten registriert, oder Quelle nicht
    // erreichbar: kein harter Abbruch, der Aufrufer greift auf den Hinweis oder
    // die Projektion zurueck.
    return 0;
  }
}

// Die serverseitige Sellify-Projektion wird von jedem Schreibvorgang synchron
// nachgezogen (refresh der Schreiboperation); ihr updated_at_ms ist damit die
// Version nach dem letzten Schreiben. Das lokale Browser-Replikat hinkt dagegen
// hinterher: die zweite Uebergabe derselben Firma scheiterte am 25.09.2026 mit
// "source version changed", weil die Dublettenpruefung die Firma im lokalen
// Replikat mit dem Stand von vor dem letzten Update fand. Auf Kundeninstanz ist
// company_source_version nicht registriert, daher dieser zweite Weg.
async function serverseitigeQuellversion(kind, entityId) {
  const id = Number(entityId) || 0;
  if (!id) return 0;
  try {
    const entity = kind === 'person' ? 'person' : 'company';
    const antwort = await sellifyNativeLookup({ entity, selectors: [], ids: [`sellify-${entity}-${id}`], limit: 1 });
    const record = Array.isArray(antwort?.records) ? antwort.records[0] : null;
    return Number(record?.updated_at_ms) || 0;
  } catch {
    return 0;
  }
}

async function dispatchExternalSqlWriteWithSourceVersion(
  operationId, recordId, values, workflowId, prompt, entity,
) {
  try {
    return await dispatchExternalSqlWrite(operationId, recordId, values, workflowId, prompt);
  } catch (error) {
    if (!isSourceVersionConflict(error)) throw error;
    const actual = await fetchAuthoritativeSourceVersion(
      entity.kind, entity.entityId, workflowId, prompt,
    ) || await serverseitigeQuellversion(entity.kind, entity.entityId);
    if (!actual || actual === (Number(values.expected_source_version) || 0)) {
      throw new Error(
        'Sellify hat den Schreibvorgang wegen einer Versionspruefung abgelehnt. '
        + 'Die erwartete Quellversion weicht vom aktuellen Stand in Sellify ab '
        + '(echte Fremdaenderung oder Quellversion nicht massgeblich lesbar).',
      );
    }
    try {
      return await dispatchExternalSqlWrite(
        operationId,
        recordId,
        { ...values, expected_source_version: actual },
        workflowId,
        prompt,
      );
    } catch (second) {
      if (!isSourceVersionConflict(second)) throw second;
      throw new Error(
        'Sellify hat den Schreibvorgang wegen einer Versionspruefung abgelehnt. '
        + 'Die erwartete Quellversion weicht vom aktuellen Stand in Sellify ab '
        + '(echte Fremdaenderung).',
      );
    }
  }
}

async function dispatchExternalSqlWrite(operationId, recordId, values, workflowId, prompt) {
  const commandId = `cmd_${workflowId}_${operationId}_${crypto.randomUUID()}`;
  const result = await sendeBefehl({
    id: commandId,
    command_id: commandId,
    module: 'outbound-lead-generation',
    command_type: 'external_sql.write',
    record_id: recordId,
    inbound_channel: 'business_os.outbound_lead_generation',
    payload: {
      source_id: 'primary-crm',
      operation_id: operationId,
      ...values,
      title: `Sellify-Übergabe: ${selectedLead()?.name || recordId}`,
      prompt,
      user_message: prompt,
      response_channel: 'business_os_chat',
      outbound_channel: 'business_os_chat',
      thread_key: `business-os/outbound-lead-generation/${workflowId}`,
    },
    client_context: {
      source_module: 'outbound-lead-generation',
      record_id: recordId,
      workflow_id: workflowId,
      response_channel: 'business_os_chat',
      writeback_required: true,
    },
  }, { until: 'terminal', timeoutMs: 90_000 });
  if (result?.status !== 'completed') {
    throw new Error(result?.error_message || result?.result?.error || `Sellify-Operation ${operationId} ist fehlgeschlagen.`);
  }
  return result;
}

// Der Schreibvorgang nach Sellify ist zu diesem Zeitpunkt bereits bestaetigt
// (`external_sql.write` -> completed). Was hier noch aussteht, ist allein die
// Rückprojektion in die browserseitige CRM-Ansicht — einer
// Collection mit ueber 17000 Zeilen. Ein Zeitueberlauf hier bedeutet also
// "noch nicht bestaetigt", nicht "fehlgeschlagen". Frueher wurde daraus ein
// harter Fehler, und der Nutzer sah "gescheitert" fuer einen Vorgang, der in
// Sellify erfolgreich war.
class SellifyReadbackPending extends Error {
  constructor(message) {
    super(message);
    this.name = 'SellifyReadbackPending';
    this.readbackPending = true;
  }
}

async function waitForProjectedRecord(collection, id, predicate, options = {}) {
  const deadline = Date.now() + Math.max(5_000, Number(options.timeoutMs) || 25_000);
  while (Date.now() < deadline) {
    const record = await collection.findOne(id).exec();
    if (record && predicate(record)) return record;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new SellifyReadbackPending(
    'Sellify hat den Schreibvorgang bestaetigt. Die Ruecksynchronisierung in die lokale Kopie steht noch aus.',
  );
}

// Nach Sellify geht, was die Recherche BELEGT hat. Bis 23.09.2026 las die
// Uebergabe nur die alten Importschluessel (email, phone, address_line, city …):
// ein recherchierter Lead traegt die aber nicht, nur firma_* mit field_status.
// Carbosulf haette Ort "Hamburg" (Import) statt des belegten "Köln" nach
// Sellify geschrieben und Anschrift, PLZ, Telefon, E-Mail gar nicht.
// Regel: belegter Recherchewert > (Feld nie recherchiert) Importwert > leer.
// Ein Feld mit Status kein Treffer/offen/Widerspruch schreibt nichts —
// leere Werte laesst updateSellifyCompany ohnehin aus (changed_fields).
function belegterWert(lead, fieldKey, importKeys = []) {
  const status = lead?.field_status?.[fieldKey];
  if (status && typeof status === 'object') {
    if (status.status !== 'verified') return '';
    return String(status.value ?? firstValue(lead.data, [fieldKey]) ?? '').trim();
  }
  return String(firstValue(lead.data, [fieldKey, ...importKeys]) || '').trim();
}

function sellifyCompanyValues(lead) {
  const domain = belegterWert(lead, 'firma_domain', ['website_url', 'website', 'internet']);
  return {
    name: String(lead.name || '').trim(),
    short_name: String(firstValue(lead.data, ['short_name', 'kurzname']) || '').trim(),
    number1: String(firstValue(lead.data, ['number1', 'nummer']) || '').trim(),
    number2: String(firstValue(lead.data, ['number2', 'nummer2']) || '').trim(),
    email: belegterWert(lead, 'firma_email', ['email', 'company_email', 'e_mail']),
    phone: belegterWert(lead, 'firma_telefon', ['phone', 'company_phone', 'telefon']),
    fax: belegterWert(lead, 'firma_fax', ['fax']),
    website_url: String(domain || lead.website || '').trim(),
    address_line: belegterWert(lead, 'firma_anschrift', ['address_line', 'address', 'street', 'strasse']),
    postal_code: belegterWert(lead, 'firma_plz', ['postal_code', 'postcode', 'plz']),
    city: belegterWert(lead, 'firma_ort', ['city', 'ort']) || (lead?.field_status?.firma_ort ? '' : String(lead.city || '').trim()),
    country_code: normalizedResearchCountry(lead.country),
  };
}

function sellifyPersonValues(contact) {
  const names = String(contact.name || '').trim().split(/\s+/);
  return {
    salutation: String(contact.salutation || contact.person_anrede || '').trim(),
    title: String(contact.title || contact.person_titel || '').trim(),
    first_name: String(contact.first_name || contact.person_vorname || names.shift() || '').trim(),
    last_name: String(contact.last_name || contact.person_nachname || names.join(' ') || '').trim(),
    department: String(contact.department || contact.person_abteilung || '').trim(),
    function: String(contact.role || contact.function || contact.person_funktion || contact.position || '').trim(),
    number: String(contact.number || '').trim(),
    email: String(contact.email || contact.person_email || '').trim(),
    phone: String(contact.phone || contact.person_telefon || '').trim(),
    mobile: String(contact.mobile || contact.person_mobil || '').trim(),
    social_media: String(contact.social_media || contact.linkedin || contact.xing || '').trim(),
  };
}

function sellifyCommunicationIds(record) {
  const sql = record?.payload?.sql || {};
  return {
    email_id: Number(sql.primary_email_id || sql.email_id) || undefined,
    phone_id: Number(sql.primary_phone_id || sql.phone_id) || undefined,
    fax_id: Number(sql.primary_fax_id || sql.fax_id) || undefined,
    mobile_id: Number(sql.primary_mobile_id || sql.mobile_id) || undefined,
    url_id: Number(sql.primary_url_id || sql.url_id) || undefined,
    address_id: Number(sql.primary_address_id || sql.address_id) || undefined,
  };
}

function returnedInteger(result, field) {
  const values = [
    result?.result?.returned_rows?.[0]?.[field],
    result?.returned_rows?.[0]?.[field],
    result?.result?.[field],
    result?.[field],
  ];
  for (const value of values) {
    const number = Number(value);
    if (Number.isInteger(number) && number > 0) return number;
  }
  return 0;
}

// source_version kommt als BIGINT (ms seit Epoch) aus der SQL-Antwort. Im
// Gegensatz zu IDs kann 0 formal gueltig sein; massgeblich ist hier > 0.
function returnedSourceVersion(result) {
  const values = [
    result?.result?.returned_rows?.[0]?.source_version,
    result?.returned_rows?.[0]?.source_version,
    result?.result?.source_version,
    result?.source_version,
  ];
  for (const value of values) {
    const number = Number(value);
    if (Number.isFinite(number) && number > 0) return number;
  }
  return 0;
}

function firstValue(record, keys) {
  for (const key of keys) {
    const value = record?.[key];
    if (value !== undefined && value !== null && String(value).trim() !== '') return value;
  }
  return '';
}

function finiteNumber(value) {
  const number = Number(String(value ?? '').replace(',', '.'));
  return Number.isFinite(number) ? number : 0;
}

// Firmennamen vergleichen, wie ein Mensch sie vergleicht.
//
// Der Lead heisst "CHEMOFAST Anchoring GmbH", die CRM-Organisation
// "CHEMOFAST® Anchoring GmbH". Ein exakter Vergleich findet das nicht, und damit
// blieb am 11.08.2026 der gesamte CRM-Pfad wirkungslos: keine Dublette, kein
// Vorwissen im Auftrag, keine uebernommenen Kontaktdaten, keine Serien-E-Mail —
// wegen eines Registerzeichens. Der Import konnte es laengst besser; der
// Firmenabgleich vor der Recherche nicht.
//
// Entfernt werden nur Zeichen ohne Unterscheidungskraft: Schutzzeichen,
// Satzzeichen, Mehrfach-Leerraum. Die Rechtsform bleibt drin — "Muster GmbH" und
// "Muster AG" sind verschiedene Firmen und muessen es bleiben.
function firmenSchluessel(value) {
  return String(value || '')
    .toLowerCase()
    .replace(/[®™©]/g, ' ')
    .replace(/[.,;:!?"'`´\/\\()\[\]{}]/g, ' ')
    .replace(/[-–—_+]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

// Der Server nimmt Sellify seit 11.09.2026 als geprueften Beleg an:
// sellify://company/<contact_id> bzw. sellify://person/<id>. Als URL gelesen
// waere das der Host "company" - eine zweite, erfundene Quelle neben Sellify.
function isSellifyCitation(value) {
  return /^sellify:\/\//i.test(String(value || '').trim());
}
// Nur diese Formen nimmt der Server an (Kern e69c27fd0). Vorher gespeicherte
// Zitate mit Import-Schluessel ("sellify://person/person_breitenfelder_mark",
// Sasol 11.09.2026) sind keine Sellify-Bestaetigung und zaehlen nicht.
function isCheckedSellifyCitation(value) {
  return /^sellify:\/\/(company\/(\d+|lead_[a-z0-9]+)|person\/sellify-person-\d+)\/?$/i.test(String(value || '').trim());
}
function normalizedDomain(value) {
  const raw = String(value || '').trim().toLowerCase();
  if (!raw) return '';
  if (isSellifyCitation(raw)) return SELLIFY_SOURCE_ID;
  try { return new URL(raw.includes('://') ? raw : `https://${raw}`).hostname.replace(/^www\./, ''); } catch { return raw.replace(/^www\./, '').split('/')[0]; }
}

function sellifyReadCollection(entity) {
  const collection = entity === 'company' ? state.sellifyCompanies : state.sellifyPeople;
  if (!collection) {
    throw new Error('Der serverseitige Sellify-Lesezugriff ist noch nicht verfügbar.');
  }
  return collection;
}

function createSellifyLookupFacade(entity) {
  const lookup = async ({ selector = {}, ids = [], limit = 25 } = {}) => {
    const selectors = [];
    for (const [field, condition] of Object.entries(selector || {})) {
      const values = condition && typeof condition === 'object'
        ? ('$in' in condition ? condition.$in : [condition.$eq])
        : [condition];
      for (const value of values || []) {
        if (value == null || String(value).trim() === '') continue;
        selectors.push({ field, value: String(value) });
      }
    }
    const normalizedIds = [...new Set(ids.map((id) => String(id || '').trim()).filter(Boolean))];
    const cacheKey = JSON.stringify([entity, selectors, normalizedIds, limit]);
    const cached = state.sellifyLookupCache.get(cacheKey);
    if (cached && Date.now() - cached.at < 5_000) return cached.records;
    const pending = state.sellifyLookupInflight.get(cacheKey);
    if (pending) return pending;
    const request = (async () => {
      const payload = { entity, selectors, ids: normalizedIds, limit };
      const antwort = await sellifyNativeLookup(payload, { commandTimeoutMs: 50_000 });
      const records = Array.isArray(antwort?.records) ? antwort.records : [];
      state.sellifyLookupCache.set(cacheKey, { at: Date.now(), records });
      return records;
    })();
    state.sellifyLookupInflight.set(cacheKey, request);
    try {
      return await request;
    } finally {
      state.sellifyLookupInflight.delete(cacheKey);
    }
  };
  return {
    lookup,
    find(query = {}) {
      return { exec: () => lookup({ selector: query?.selector || {}, limit: query?.limit || 25 }) };
    },
    findOne(id) {
      return { exec: async () => (await lookup({ ids: [String(id || '')], limit: 1 }))[0] || null };
    },
  };
}

function sellifyWritebackContract() {
  return {
    system: 'sellify-sqlite-sync', source_of_truth: 'sellify-original-sql', direct_sql_write_allowed: false, deduplicate_before_create: true,
    allowed_commands: ['external_sql.write'],
    allowed_operations: [
      'company_create', 'company_update', 'company_metrics_update', 'company_wz_update',
      'company_source_version',
      'person_create', 'person_update', 'person_source_version',
      'campaign_create',
    ],
    required_result: ['contact_id', 'person_ids', 'campaign_ids', 'command_ids', 'sync_status', 'deduplication_decision'],
  };
}

function enabledSourcePolicy() {
  return {
    skill: 'outbound-lead-generation-research', min_independent_sources: 1, verification_scope: 'per_field', validation_only_sources: ['experte.de'],
    sources: state.sources
      .filter((item) => item.enabled && !isInternalResearchSource(item)
        && !isDocumentationSourceKey(evidenceSourceKey({ source_id: item.id, source_url: item.url })))
      // Der native Lauf benutzt `url` NUR fuer dynamisch entdeckte Adapter;
      // eingebaute Quellen ueberspringt er ohnehin. Er parst die URL aber
      // VOR diesem Ueberspringen und bricht dann den gesamten, bereits
      // fertigen Rechercheauftrag ab. Genau das passierte ab dem 30.08.
      // 21:16, als `seedSources` die eingebaute, input-getriebene Quelle
      // `impressum` (Start-URL = Lead-Domain, also bewusst ohne feste URL)
      // anlegte: seitdem starb JEDER Lauf mit
      // "invalid runtime source URL for `impressum`" und verwarf sein
      // Ergebnis. Wir schicken solche Quellen erst gar nicht mit - das ist
      // exakt das Ergebnis, das das native `continue` erzeugt haette.
      .filter((item) => /^https?:\/\//i.test(String(item.url || '').trim()))
      .map((item) => ({ id: item.id, url: item.url, field_keys: item.field_keys, target_key: item.target_key, credential_secret_name: item.credential_secret_name, operator_instructions: String(item.payload?.instructions || ''), secret_value_in_payload: false })),
  };
}

function enabledPrivateResearchSources(sources = state.sources) {
  return sources
    .filter((item) => item.enabled
      && (item.requires_credential || String(item.credential_secret_name || '').trim())
      && !isInternalResearchSource(item))
    .map((item) => item.id)
    .filter(Boolean)
    .sort();
}

function normalizedResearchCountry(value) {
  const country = String(value || 'DE').trim().toUpperCase();
  if (['A', 'AT', 'AUT', 'AUSTRIA', 'ÖSTERREICH', 'OESTERREICH'].includes(country)) return 'AT';
  if (['CH', 'CHE', 'SCHWEIZ', 'SUISSE', 'SVIZZERA', 'SWITZERLAND'].includes(country)) return 'CH';
  return 'DE';
}

function researchOutcomeWriteback(lead, commandResult) {
  const outcome = commandResult?.result && typeof commandResult.result === 'object'
    ? commandResult.result
    : commandResult;
  const fields = outcome?.fields && typeof outcome.fields === 'object' ? outcome.fields : {};
  const data = { ...(lead.data || {}) };
  const contact = { ...(lead.contacts?.[0] || {}) };
  const evidence = [];
  const researchedFieldKeys = [];
  for (const [fieldKey, field] of Object.entries(fields)) {
    if (field?.value === null || field?.value === undefined || String(field.value).trim() === '') continue;
    researchedFieldKeys.push(fieldKey);
    if (fieldKey.startsWith('person_')) contact[fieldKey] = field.value;
    else data[fieldKey] = field.value;
    for (const candidate of field.candidates || []) {
      if (!candidate?.source_id && !candidate?.source_url) continue;
      evidence.push({
        field_key: fieldKey,
        value: candidate.value,
        confidence: candidate.confidence || '',
        source_id: candidate.source_id || '',
        source_url: candidate.source_url || '',
        tier: candidate.tier || '',
        via: candidate.via || '',
        label: candidate.source_id || candidate.source_url || 'Quelle',
      });
    }
  }
  const mergedEvidence = deduplicateEvidence([...(lead.evidence || []), ...evidence]);
  const normalizedContact = normalizeResearchedContact(contact);
  const contacts = withStableContactIds(
    lead.id || 'lead',
    normalizedContact ? [normalizedContact, ...(lead.contacts || []).slice(1)] : (lead.contacts || []),
  );
  const contactIds = new Set(contacts.map((entry) => entry.id));
  const selectedContactIds = (lead.selected_contact_ids || []).filter((id) => contactIds.has(id));
  const draft = {
    ...lead,
    data,
    contacts,
    selected_contact_ids: selectedContactIds,
    evidence: mergedEvidence,
    payload: {
      ...lead.payload,
      research_error: '',
      researched_field_keys: researchedFieldKeys,
      research_finished_at_ms: Date.now(),
      research_tool: outcome?.tool || '',
      browser_assist_tasks: outcome?.browser_assist_tasks || [],
    },
  };
  const unverifiedFieldKeys = researchedFieldKeys.filter((fieldKey) => !fieldSourcesSatisfyRule(fieldKey, fieldEvidenceSources(draft, fieldKey)));
  return {
    data: draft.data,
    contacts: draft.contacts,
    selected_contact_ids: draft.selected_contact_ids,
    evidence: draft.evidence,
    research_status: researchedFieldKeys.length > 0 && unverifiedFieldKeys.length === 0 ? 'completed' : 'needs_review',
    research_error: '',
    research_updated_at_ms: Date.now(),
    payload: {
      ...draft.payload,
      verified_field_keys: researchedFieldKeys.filter((fieldKey) => !unverifiedFieldKeys.includes(fieldKey)),
      unverified_field_keys: unverifiedFieldKeys,
    },
  };
}

// Ein gescheiterter Task darf ein bereits geschriebenes Ergebnis NICHT
// ueberschreiben. Genau das hat am 03./04.09.2026 die erfolgreichen Recherchen
// unsichtbar gemacht: der Agent schrieb sein Ergebnis (needs_review) zurueck,
// lief danach in eine unerfuellbare Review-Bedingung, der Task endete failed -
// und das failed setzte den Lead auf "Unvollstaendig" zurueck.
function hatRechercheErgebnis(lead) {
  if (!lead) return false;
  if (Array.isArray(lead.payload?.researched_field_keys) && lead.payload.researched_field_keys.length) return true;
  if (['completed', 'needs_review'].includes(String(lead.research_status || ''))) return true;
  return false;
}

// Fehlertext festhalten, Ergebnisstatus behalten.
function fehlerOhneErgebnisverlust(lead, fehlertext, zusatzPayload = {}) {
  const payload = { ...(lead?.payload || {}), ...zusatzPayload, research_error: String(fehlertext || '') };
  if (hatRechercheErgebnis(lead)) {
    return {
      research_status: String(lead.research_status || 'needs_review') === 'completed' ? 'completed' : 'needs_review',
      research_updated_at_ms: Date.now(),
      payload: { ...payload, research_last_failure: String(fehlertext || ''), research_last_failure_at_ms: Date.now() },
    };
  }
  return { research_status: 'failed', research_updated_at_ms: Date.now(), payload };
}

function researchCommandLeadPatch(lead, command) {
  if (command?.command_type === 'business_os.chat.task') return chatResearchTaskLeadPatch(lead, command);
  if (command?.command_type !== 'web_stack.person_research') return null;
  const status = normalizedResearchCommandStatus(command);
  const commandId = String(command.command_id || command.id || lead.command_id || '').trim();
  if (['accepted', 'queued', 'running', 'leased', 'retry_wait', 'working'].includes(status)) {
    return {
      research_status: status === 'queued' ? 'queued' : 'running',
      command_id: commandId,
      task_id: String(command.task_id || lead.task_id || '').trim(),
      payload: {
        ...lead.payload,
        last_research_command_id: commandId,
        research_started_at_ms: Number(lead.payload?.research_started_at_ms || Date.now()),
      },
    };
  }
  if (['failed', 'blocked', 'cancelled', 'canceled', 'error'].includes(status)) {
    return fehlerOhneErgebnisverlust(
      lead,
      String(command.error_message || command.error || command.result?.error || 'Recherche fehlgeschlagen.'),
      { reconciled_research_command_id: commandId, research_finished_at_ms: Date.now() },
    );
  }
  if (!['completed', 'handled', 'success', 'done', 'passed'].includes(status)) return null;
  if (!command.result || command.result.ok === false) return null;
  const writeback = researchOutcomeWriteback(lead, command);
  return {
    ...writeback,
    research_error: '',
    command_id: commandId,
    task_id: '',
    payload: {
      ...writeback.payload,
      reconciled_research_command_id: commandId,
      last_research_command_id: commandId,
      research_mode: String(command.result?.mode || ''),
    },
  };
}

// Die Chat-Aufgabe liefert ihr Ergebnis ueber das Rueckschreiben, das den Lead
// selbst aktualisiert. Hier zaehlt nur ihr Scheitern: dann steht der Lead auf
// "Unvollständig" mit dem Grund, den der Worker genannt hat, statt auf "Läuft".
function chatResearchTaskLeadPatch(lead, command) {
  const status = normalizedResearchCommandStatus(command);
  if (!['failed', 'blocked', 'cancelled', 'canceled', 'error'].includes(status)) return ausfuehrungsphasePatch(lead, command);
  if (['cancelled', 'canceled'].includes(status)) {
    return fehlerOhneErgebnisverlust(lead, 'Recherche abgebrochen.', {
      reconciled_research_command_id: String(command.command_id || command.id || ''),
      research_finished_at_ms: Date.now(),
    });
  }
  const worker = String(command?.result?.user_message || command?.result?.user_reply || '').trim();
  const grund = worker
    ? worker.split(/(?<=[.!?])\s+/).slice(0, 2).join(' ').slice(0, 400)
    : 'Die Recherche endete ohne Ergebnis.';
  return fehlerOhneErgebnisverlust(lead, `Recherche ohne Ergebnis beendet: ${grund}`, {
    reconciled_research_command_id: String(command.command_id || command.id || ''),
    research_finished_at_ms: Date.now(),
  });
}
// "Läuft" hiess bisher nur "abgeschickt". LUZI (lead_10blqvp) zeigte am
// 27.09.2026 ueber eine Stunde "Läuft", waehrend sein Queue-Task seit 12:01
// unveraendert queued stand (attempt 0, kein Lease). Die Anzeige folgt deshalb
// der tatsaechlichen Ausfuehrungsphase des Vorgangs; der Status bleibt running.
const WARTENDE_AUSFUEHRUNGSPHASEN = new Map([
  ['queued', 'Wartet auf Ausführung'],
  ['retry_wait', 'Wartet auf Wiederholung'],
  ['blocked', 'Wartet · blockiert'],
]);
function ausfuehrungsphasePatch(lead, command) {
  if (!['queued', 'running'].includes(String(lead?.research_status || ''))) return null;
  const commandId = String(command?.command_id || command?.id || '').trim();
  if (!commandId || commandId !== String(lead?.command_id || '').trim()) return null;
  const phase = String(command?.execution_phase || '').trim().toLowerCase();
  if (!phase || phase === 'none' || phase === 'terminal') return null;
  if (String(lead?.payload?.research_execution_phase || '') === phase
    && String(lead?.payload?.research_execution_phase_command_id || '') === commandId) return null;
  return {
    payload: {
      ...(lead?.payload || {}),
      research_execution_phase: phase,
      research_execution_phase_command_id: commandId,
      research_execution_phase_at_ms: Date.now(),
    },
  };
}
// Anzeige, Filter, Kampagnenzaehler und Excel verwenden denselben effektiven
// Status: ein abgeschickter, aber noch nicht gestarteter Vorgang wartet.
function effektiverRechercheStatus(lead) {
  return wartendeAusfuehrungsphase(lead) ? 'queued' : String(lead?.research_status || 'new');
}
function wartendeAusfuehrungsphase(lead) {
  if (String(lead?.research_status || '') !== 'running') return '';
  const payload = lead?.payload || {};
  if (String(payload.research_execution_phase_command_id || '') !== String(lead?.command_id || '').trim()) return '';
  return WARTENDE_AUSFUEHRUNGSPHASEN.get(String(payload.research_execution_phase || '')) || '';
}
function normalizedResearchCommandStatus(command) {
  const terminal = String(command?.terminal_status || '').trim().toLowerCase();
  if (['completed', 'handled', 'success', 'done', 'passed', 'failed', 'blocked', 'cancelled', 'canceled', 'error'].includes(terminal)) {
    return terminal;
  }
  return String(
    command?.status
    || command?.execution_phase
    || command?.result?.status
    || command?.task_status
    || '',
  ).trim().toLowerCase();
}

function normalizeResearchedContact(contact) {
  const firstName = String(contact.person_vorname || '').trim();
  const lastName = String(contact.person_nachname || '').trim();
  const name = [firstName, lastName].filter(Boolean).join(' ');
  const profile = String(contact.person_linkedin || contact.person_xing || contact.linkedin || contact.xing || '').trim();
  if (!name && !contact.person_email && !contact.person_telefon && !contact.person_position && !profile) return null;
  return {
    ...contact,
    name: name || contact.name || '',
    role: contact.person_funktion || contact.person_position || contact.role || '',
    position: contact.person_position || contact.position || '',
    email: contact.person_email || contact.email || '',
    phone: contact.person_telefon || contact.phone || '',
  };
}

function deduplicateEvidence(entries) {
  const seen = new Set();
  return entries.filter((entry) => {
    // Die Person gehoert zum Schluessel: vier gleichlautende E-Mail-Pruefungen
    // ("valid", dieselbe Quelle) fuer vier Personen fielen sonst auf eine
    // zusammen — die Freigabe eines Feldes loeschte so 6 Belege anderer
    // Personen (Klicktest P3 REV-04f).
    const key = [
      entry?.field_key || entry?.field || '',
      entry?.source_id || '',
      entry?.source_url || entry?.url || '',
      String(entry?.value ?? ''),
      entry?.person_key || entry?.person_id || entry?.contact_ref || '',
    ].join('|').toLowerCase();
    if (!key.replace(/\|/g, '') || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

// Nach dem Speichern zeigte die Detailansicht teils noch den alten Stand
// ("Freigabe: noch 10 offene Punkte", obwohl alle Felder gespeichert waren):
// reload() liest die Datenbank, und die eigene Aenderung ist dort erst nach der
// Bestaetigung lesbar. Die Aenderung gilt deshalb sofort in der Anzeige und
// bleibt, bis die Datenbank einen mindestens so neuen Stand liefert.
// `payloadMerge`: nur die genannten payload-Schluessel setzen, den Rest aus dem
// AKTUELLEN Datensatz nehmen. Hintergrundpruefungen (Sellify-Vorabgleich, bis
// zu 20 s) schrieben sonst die ganze payload aus einem alten Schnappschuss
// zurueck und loeschten zwischenzeitlich Geschriebenes (Klicktest P2 V15).
async function patchLead(id, patch, { payloadMerge = false, dataMerge = false } = {}) {
  const doc = await state.collections.leads.findOne(id).exec();
  if (!doc) return;
  const current = doc.toJSON?.() || doc;
  if (payloadMerge && patch?.payload) patch = { ...patch, payload: { ...(current.payload || {}), ...patch.payload } };
  if (dataMerge && patch?.data) patch = { ...patch, data: { ...(current.data || {}), ...patch.data } };
  const normalized = normalizeLeadRecipientShape({ ...current, ...patch, id });
  const updatedAt = Date.now();
  const applied = {
    ...patch,
    contacts: normalized.contacts,
    selected_contact_ids: normalized.selected_contact_ids,
    updated_at_ms: updatedAt,
  };
  state.pendingLeadPatches.set(id, { patch: applied, at: updatedAt });
  const index = state.leads.findIndex((lead) => lead.id === id);
  const before = index >= 0 ? state.leads[index] : null;
  if (index >= 0) state.leads[index] = normalizeLeadRecipientShape({ ...state.leads[index], ...applied });
  try {
    await doc.incrementalPatch(applied);
  } catch (error) {
    // Nicht gespeichert: die Anzeige darf keinen Stand behalten, den es nicht gibt.
    state.pendingLeadPatches.delete(id);
    const again = state.leads.findIndex((lead) => lead.id === id);
    if (before && again >= 0) state.leads[again] = before;
    throw error;
  }
}
function applyPendingLeadPatches(leads) {
  return leads.map((lead) => {
    const pending = state.pendingLeadPatches.get(lead.id);
    if (!pending) return lead;
    if (Number(lead.updated_at_ms || 0) >= pending.at) {
      state.pendingLeadPatches.delete(lead.id);
      return lead;
    }
    return normalizeLeadRecipientShape({ ...lead, ...pending.patch });
  });
}

// Unveraenderte Bereiche NICHT neu schreiben: jedes innerHTML ersetzt die
// Knoepfe unter dem Zeiger; ein Klick, der in den Austausch faellt, geht
// verloren, Hover und Fokus springen. Rundgang 28.09.2026: die Kopfleiste
// wurde etwa alle 12 s ersetzt, obwohl sich nichts geaendert hatte.
function setzeHtmlWennGeaendert(element, html) {
  if (!element) return false;
  const text = String(html ?? '');
  if (element.__olgHtml === text && element.innerHTML !== '') return false;
  element.innerHTML = text;
  element.__olgHtml = text;
  return true;
}

// Solange die Synchronisation noch Sammlungen nachlaedt, ist eine leere Liste
// kein Befund. Rundgang 28.09.2026: ~50 s lang stand „Noch keine Kampagne“ und
// „Noch keine Leads importiert“ da, obwohl 12 Kampagnen gleich kamen.
function datenLadenNoch() {
  return Boolean(state.syncPending) || (state.syncWaitingCollections?.size || 0) > 0;
}

function selectedLead() { return state.leads.find((lead) => lead.id === state.selectedLeadId) || null; }
function adapterReady(value) { return ['adapter_ready', 'test_ok'].includes(value?.status || value?.adapter_status) && ['registered', 'test_executed'].includes(value?.scrape_status); }
// Zusammengesetzte Endungen, bei denen die vorletzte Marke nicht der Anbieter
// ist (northdata.co.uk waere sonst "co").
const COMPOUND_TLDS = new Set([
  'co.uk', 'org.uk', 'ac.uk', 'gov.uk', 'co.at', 'or.at', 'ac.at', 'com.au',
  'net.au', 'org.au', 'co.jp', 'com.br', 'com.tr', 'co.nz', 'com.pl',
]);
// Der Anbieter, nicht der Hostname, ist die Quelle. northdata.de und
// northdata.com sind dasselbe Haus, ebenso de.linkedin.com und linkedin.com.
// Zwei Seiten derselben Quelle sind eine Quelle (Recherchevorgabe 5) — wer
// nach Hostnamen zaehlt, macht aus einem Anbieter zwei Belege. Gemessen am
// 09.09.2026 bei AKEMI: "Ort" galt mit northdata.de und northdata.com als
// dreifach belegt.
function evidenceSourceProvider(host) {
  const labels = String(host || '').trim().toLowerCase().split('.').filter(Boolean);
  if (labels.length < 2) return labels.join('.');
  const lastTwo = labels.slice(-2).join('.');
  return COMPOUND_TLDS.has(lastTwo) ? labels[labels.length - 3] || lastTwo : labels[labels.length - 2];
}
function evidenceSourceKey(entry) {
  const rawUrl = entry?.source_url || entry?.url || '';
  if (isSellifyCitation(rawUrl)) return isCheckedSellifyCitation(rawUrl) ? SELLIFY_SOURCE_ID : '';
  const rawKey = rawUrl || entry?.source_id || '';
  if (!rawKey) return '';
  const raw = String(rawKey).trim().toLowerCase();
  let host = raw;
  try { host = new URL(raw).hostname; } catch {
    // Belege ohne Schema ("northdata.de/Firma/...") sind haeufig; sonst zaehlt
    // derselbe Anbieter mit und ohne Pfad doppelt.
    try { host = new URL(`https://${raw}`).hostname; } catch { /* Source IDs are valid evidence keys too. */ }
  }
  const provider = evidenceSourceProvider(host.replace(/^www\./, ''));
  return provider || raw;
}
// Dokumentations-Hosts (example.com, .invalid) belegen nichts; ein Worker, der
// ein Ziel ausprobiert, hinterliess am 10.09.2026 "example" als Quelle am
// Firmennamen von Aeroxon.
function isDocumentationSourceKey(key) {
  const text = String(key || '').trim().toLowerCase();
  return /^example(\.|$)/.test(text) || /\.(example|invalid|test)$/.test(text);
}
function evidenceCountsForTwoSourceRule(source) {
  const key = String(source?.key || '').trim().toLowerCase();
  if (!key || key === 'operator' || isDocumentationSourceKey(key)) return false;
  if (key === SELLIFY_SOURCE_ID && source?.sellifyAgrees === false) return false;
  if (source?.countsAsExternal === false) return false;
  const eligible = source?.evidenceEligible;
  return eligible !== false;
}
function independentEvidenceCount(lead) {
  const sourceKeys = new Set();
  for (const entry of lead?.evidence || []) {
    const key = evidenceSourceKey(entry);
    const source = {
      key,
      evidenceEligible: entry?.evidence_gate?.evidence_eligible !== false
        && entry?.evidence_eligible !== false
        && !['blocked', 'unreachable', 'failed'].includes(String(entry?.verification_status || entry?.status || '').toLowerCase()),
    };
    if (evidenceCountsForTwoSourceRule(source)) sourceKeys.add(key);
  }
  return sourceKeys.size;
}
function sourceEvidenceGroups(lead) {
  const groups = new Map();
  for (const entry of lead?.evidence || []) {
    const rawUrl = String(entry?.source_url || entry?.url || '').trim();
    const rawId = String(entry?.source_id || '').trim();
    let host = '';
    try { host = new URL(rawUrl).hostname.replace(/^www\./, '').toLowerCase(); } catch { /* A source ID can be used without a URL. */ }
    const sellify = isSellifyCitation(rawUrl);
    if (sellify && !isCheckedSellifyCitation(rawUrl)) continue;
    const key = (sellify ? SELLIFY_SOURCE_ID : (rawId || host || rawUrl)).toLowerCase();
    if (!key) continue;
    const current = groups.get(key) || {
      key,
      label: sellify ? SELLIFY_SOURCE_LABEL : (entry?.label || rawId || host || rawUrl),
      url: sellify ? '' : rawUrl,
      fields: new Set(),
    };
    if (!current.url && rawUrl) current.url = rawUrl;
    const fieldKey = entry?.field_key || entry?.field;
    if (fieldKey) current.fields.add(String(fieldKey));
    groups.set(key, current);
  }
  return [...groups.values()]
    .map((group) => ({ ...group, fieldCount: group.fields.size || 1 }))
    .sort((left, right) => left.label.localeCompare(right.label, 'de'));
}
function independentFieldEvidenceCount(lead, fieldKey) {
  return independentEvidenceCount({
    evidence: (lead?.evidence || []).filter((entry) => (entry?.field_key || entry?.field) === fieldKey),
  });
}
// "ungueltig" enthaelt "gueltig", "invalid" enthaelt "valid", "unzustellbar"
// enthaelt "zustellbar". Die fruehere Pruefung per includes() haette jede als
// ungueltig gepruefte Adresse als freigabefaehig durchgewunken. Deshalb zuerst
// die Verneinung.
// Owner-Entscheidung 23.09.2026: Eine persoenliche Kontaktadresse ist auch
// dann freigabefaehig, wenn sie in einer direkt belegten offiziellen
// Primaerquelle steht — der eigenen Website des Unternehmens. SMTP-Pruefungen
// sind dafuer nicht zwingend: MailTester nannte eine erfundene Adresse auf
// einer Catch-all-Domain "Deliverable" und ein reales Exchange-Postfach
// "does not exist". Primaerquelle heisst: Beleg-URL auf der Domain der Adresse
// oder der belegten Firmendomain, und das Zitat nennt genau diese Adresse
// (auch als "(at)"/"[at]" geschrieben). Sellify und Fremdseiten zaehlen nicht.
// Sperrvermerke bleiben davon unberuehrt und zwingend.
function registrierbareDomain(host) {
  const teile = String(host || '').toLowerCase().replace(/^www\./, '').split('.').filter(Boolean);
  return teile.slice(-2).join('.');
}
function zitatNenntAdresse(quote, email) {
  const text = String(quote || '').toLowerCase()
    .replace(/\s*[([{]\s*(?:at|ät|@)\s*[)\]}]\s*/g, '@')
    .replace(/\s+at\s+/g, '@')
    .replace(/\s*[([{]\s*(?:dot|punkt)\s*[)\]}]\s*/g, '.');
  return text.includes(String(email || '').toLowerCase());
}
function kontaktAdresseAusPrimaerquelle(lead, contact) {
  const email = String(contact?.email || contact?.person_email || '').trim().toLowerCase();
  const at = email.lastIndexOf('@');
  if (at < 1) return false;
  const adressDomain = registrierbareDomain(email.slice(at + 1));
  const firmenDomain = registrierbareDomain(String(researchFieldValue(lead, 'firma_domain') || '').replace(/^https?:\/\//, '').split('/')[0]);
  const personKey = String(contact?.person_key || '').trim();
  const quellen = [
    ...(Array.isArray(contact?.sources) ? contact.sources : []),
    ...(Array.isArray(contact?.evidence) ? contact.evidence : []),
    ...(Array.isArray(lead?.evidence) ? lead.evidence : []).filter((entry) => personKey && String(entry?.person_key || '') === personKey),
  ];
  return quellen.some((quelle) => {
    const url = String(quelle?.url || quelle?.source_url || '').trim();
    if (!/^https?:\/\//i.test(url)) return false;
    let host = '';
    try { host = new URL(url).hostname; } catch { return false; }
    const quellDomain = registrierbareDomain(host);
    if (!quellDomain || (quellDomain !== adressDomain && quellDomain !== firmenDomain)) return false;
    return zitatNenntAdresse(quelle?.quote, email);
  });
}

// Owner-Regel 23.09.2026: Eine persoenliche Adresse ist belegt, wenn EINE
// externe Quelle (http(s), nicht Sellify, nicht Doku-Host) sie woertlich
// nennt. Die Quelle muss an diese Person gebunden sein (person_key) oder die
// Adresse selbst nennen; ein Adressmuster oder die Adresse eines Kollegen
// belegt nichts. Sellify allein belegt nie.
function kontaktEmailQuellen(lead, contact) {
  const personKey = String(contact?.person_key || '').trim();
  // Nur E-Mail-Belege oder feldlose Personenquellen: ein Pruefurteil
  // ("EXPERTE.de verdict: <adresse> | Unbekannt", Carbosulf 23.09.2026) nennt
  // die Adresse, belegt aber nicht, dass sie der Person gehoert.
  const emailBeleg = (entry) => ['', 'person_email', 'email'].includes(String(entry?.field_key || entry?.field || ''));
  return [
    ...(Array.isArray(contact?.sources) ? contact.sources : []),
    ...(Array.isArray(contact?.evidence) ? contact.evidence : []),
    ...(Array.isArray(lead?.evidence) ? lead.evidence : []).filter((entry) => {
      const key = String(entry?.person_key || '').trim();
      return (personKey && key === personKey) || (!key && String(entry?.field_key || entry?.field || '') === 'person_email');
    }),
  ].filter(emailBeleg);
}
// Identitaet je Person: Die Quelle muss den gespeicherten Namen nennen.
// Carbosulf 23.09.2026: person_key thomas-schauzu, Zitate "Thomas Schauzu",
// gespeichert aber "Hans-Robert Schauzu" - der Vorname war vom Geschaefts-
// fuehrer Jacob uebernommen. Solche Kontakte gehen weder nach Sellify noch in
// den Serienbrief, bis der Name korrigiert ist.
function kontaktIdentitaet(contact) {
  const vorname = String(contact?.person_vorname || contact?.first_name || '').trim();
  const nachname = String(contact?.person_nachname || contact?.last_name || '').trim();
  const quellen = (Array.isArray(contact?.sources) ? contact.sources : [])
    .filter((quelle) => /^https?:\/\//i.test(String(quelle?.url || quelle?.source_url || '')) && String(quelle?.quote || '').trim());
  if (!nachname || !quellen.length) return { status: 'unbelegt' };
  const voll = normalizeProtectionText(`${vorname} ${nachname}`);
  const zitate = quellen.map((quelle) => normalizeProtectionText(quelle.quote));
  if (vorname && zitate.some((zitat) => zitat.includes(voll))) return { status: 'belegt' };
  const nach = normalizeProtectionText(nachname);
  for (const quelle of quellen) {
    const roh = String(quelle.quote || '');
    const treffer = roh.match(new RegExp(`([A-ZÄÖÜ][\\p{L}.-]+)\\s+${nachname.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}])`, 'u'));
    const eigeneVornamen = [normalizeProtectionText(vorname), ...normalizeProtectionText(vorname).split(' ')];
    if (treffer && !eigeneVornamen.includes(normalizeProtectionText(treffer[1]))
      && !/^(herr|frau|dr|prof|geschäftsführer|geschäftsführerin|prokurist|prokuristin)$/i.test(treffer[1].replace(/\.$/, ''))) {
      return { status: 'widerspruch', quelleName: `${treffer[1]} ${nachname}` };
    }
  }
  return zitate.some((zitat) => zitat.includes(nach)) && !vorname ? { status: 'belegt' } : { status: 'unbelegt' };
}

function kontaktEmailBelegt(lead, contact) {
  const email = String(contact?.email || contact?.person_email || '').trim().toLowerCase();
  if (!email.includes('@')) return false;
  return kontaktEmailQuellen(lead, contact).some((quelle) => {
    const url = String(quelle?.url || quelle?.source_url || '').trim();
    if (!/^https?:\/\//i.test(url) || isSellifyCitation(url)) return false;
    const key = evidenceSourceKey(quelle);
    if (!key || key === SELLIFY_SOURCE_ID || isDocumentationSourceKey(key)) return false;
    return zitatNenntAdresse(quelle?.quote, email);
  });
}

function emailVerdictIsDeliverable(value) {
  const status = normalizeProtectionText(value || '');
  if (!status) return false;
  if (['ungueltig', 'ungültig', 'invalid', 'unzustellbar', 'nicht', 'unbekannt', 'unknown', 'fehler'].some((term) => status.includes(term))) return false;
  return ['valid', 'gueltig', 'gültig', 'zustellbar', 'bestaetigt', 'bestätigt'].some((term) => status.includes(term));
}
// Sagt, WARUM keine gepruefte Adresse vorliegt: keine gefunden, Pruefung
// ausstehend (CTOX prueft selbst), fehlgeschlagen oder unzustellbar. Die
// Pruefung schreibt ihren letzten Durchlauf nach payload.email_validation_pass.
function emailValidationBlocker(lead) {
  const addresses = (lead?.contacts || [])
    .map((contact) => ({
      email: String(contact?.email || contact?.person_email || '').trim(),
      verdict: String(contact?.email_validation || contact?.person_email_validation || '').trim(),
    }))
    .filter((entry) => entry.email.includes('@'));
  if (!addresses.length) return 'keine Ansprechpartner-E-Mail gefunden';
  const failed = (lead?.payload?.email_validation_pass?.failed || [])
    .map((entry) => String(entry?.email || '').trim())
    .filter(Boolean);
  const checked = addresses.filter((entry) => /invalid|ungueltig|ungültig|unzustellbar/i.test(entry.verdict));
  if (checked.length === addresses.length) {
    return `Ansprechpartner-E-Mail als unzustellbar geprüft: ${checked.map((entry) => entry.email).join(', ')}`;
  }
  if (failed.length) return `E-Mail-Prüfung fehlgeschlagen, CTOX versucht es erneut: ${failed.join(', ')}`;
  // CTOX prueft neue Adressen im stuendlichen Durchlauf (contact_email_validation
  // sweep); "prueft selbst" ohne Zeitangabe las sich wie ein Haenger (Nachtest P3 N-3).
  return 'E-Mail-Prüfung ausstehend – CTOX prüft neue Adressen automatisch, spätestens innerhalb einer Stunde';
}
function leadReadyForValidation(lead) {
  return validationBlockers(lead).length === 0;
}
function researchSubmissionPending(lead, pendingIds = state.pendingResearchIds) {
  return Boolean(lead?.id && pendingIds?.has?.(lead.id));
}
function researchInFlight(lead) {
  return ['queued', 'running'].includes(String(lead?.research_status || ''))
    && Boolean(String(lead?.command_id || '').trim());
}

async function cancelResearch(id) {
  const lead = state.leads.find((entry) => entry.id === id);
  if (!lead || !researchInFlight(lead)) return;
  const commandId = String(lead.command_id || '').trim();
  const confirmed = await showBusinessConfirm(
    `Die laufende Recherche für „${lead.name || 'diesen Lead'}“ wird abgebrochen. Bereits gespeicherte Werte und Belege bleiben erhalten.`,
    { title: 'Recherche abbrechen', confirmLabel: 'Recherche abbrechen', cancelLabel: 'Weiterlaufen lassen', kind: 'danger' },
  );
  if (!confirmed) return;
  const bus = state.ctx?.commandBus;
  if (typeof bus?.cancel !== 'function') {
    await showBusinessAlert('Abbrechen ist in dieser Sitzung nicht verfügbar.');
    return;
  }
  // Bis zur Rueckmeldung sichtbar "wird abgebrochen" statt 15 s Stille
  // (Klicktest-Nachtest P2).
  state.researchCancelling = state.researchCancelling || new Set();
  state.researchCancelling.add(id);
  zeigeHinweis(`Recherche für „${lead.name || 'diesen Lead'}“ wird abgebrochen …`, 0);
  renderDetail();
  try {
    await bus.cancel(commandId, { reason: 'Recherche in der Outbound-App abgebrochen', until: 'terminal' });
  } catch (error) {
    state.researchCancelling.delete(id);
    const message = String(error?.message || error);
    // Kennt der Server den Auftrag nicht mehr oder ist er schon beendet, laeuft
    // nichts mehr - dann nur den Lead aus "Läuft" holen, sonst bliebe er dort
    // ohne jeden Ausweg stehen.
    if (!/not found|no cancellable|already terminal|invalid_transition|nicht gefunden/i.test(message)) {
      await showBusinessAlert(`Die Recherche konnte nicht abgebrochen werden: ${message}`);
      return;
    }
  }
  const aktuell = state.leads.find((entry) => entry.id === id) || lead;
  await patchLead(id, fehlerOhneErgebnisverlust(aktuell, 'Recherche abgebrochen.', {
    reconciled_research_command_id: commandId,
    research_finished_at_ms: Date.now(),
  }));
  state.researchCancelling.delete(id);
  // Laeuft im Kampagnenlauf nichts mehr, ist der Lauf zu Ende; vorher blieb
  // "Task öffnen" / "Laufend 1/1" bis zum Neuladen stehen (Nachtest P1 BG-06a).
  const kampagne = String(aktuell.campaign || lead.campaign || '').trim();
  const lauf = state.campaignRuns.get(kampagne);
  if (lauf && ['queued', 'running'].includes(String(lauf.status || ''))
    && !campaignListLeads(kampagne).some((entry) => researchInFlight(entry))) {
    Object.assign(lauf, { status: 'cancelled', finishedAtMs: Date.now() });
  }
  zeigeHinweis('Recherche abgebrochen.');
  render();
}
// Excel-Export (Owner 22.09.2026): die Recherche als formatierte .xlsx, neben
// der Sellify-Uebergabe. Der Exporter liegt in xlsx-export.js und laedt JSZip
// aus dem Business-OS-Vendor-Ordner erst beim ersten Export.
async function exportResearchXlsx(leads, label) {
  if (state.exportXlsxBusy) return;
  const liste = (Array.isArray(leads) ? leads : []).filter(Boolean);
  if (!liste.length) {
    await showBusinessAlert('Keine Leads zum Exportieren.');
    return;
  }
  state.exportXlsxBusy = true;
  zeigeHinweis(liste.length > 1 ? `Excel mit ${liste.length} Leads wird erstellt …` : 'Excel wird erstellt …');
  render();
  try {
    const snapshot = captureResearchExport(liste, (lead, contact) => {
        const normalized = normalizeLeadRecipientShape(lead || {});
        const identity = (entry) => [String(entry?.person_key || entry?.sellify_person_id || ''), personDisplayName(entry) || '', String(entry?.person_email || entry?.email || '').toLowerCase()];
        const [key, name, mail] = identity(contact);
        const match = normalized.contacts.find((entry) => (contact?.id && entry.id === contact.id)
          || (key && identity(entry)[0] === key)
          || (name && identity(entry)[1] === name && identity(entry)[2] === mail));
        if (!match) return null;
        const current = currentContactEligibility(normalized, match);
        const saved = lead?.recipient_eligibility;
        const signature = recipientSignatureHash(recipientEligibilitySignature(normalized));
        const stored = saved?.signature === signature && Array.isArray(saved?.decisions)
          ? saved.decisions.find(pair => pair?.[0] === match.id)?.[1] : null;
        const urteil = current?.pending ? (stored?.pending ? null : stored || null) : current;
        const identitaet = kontaktIdentitaet(match);
        if (identitaet.status === 'widerspruch' && urteil?.status === 'free') {
          return personEligibilityDecision('review', { label: `Name widerspricht Quelle („${identitaet.quelleName}“)` }, '', '', '');
        }
        return urteil;
      });
    const labels = new Map(snapshot.leads.map(lead => [lead, { research: researchLabel(lead), sellify: sellifyLabel(lead) }]));
    const exporter = await import(new URL('./xlsx-export.js', import.meta.url).href);
    const blob = await exporter.buildResearchWorkbook(snapshot.leads, {
      title: String(label || 'Recherche'),
      groups: RESEARCH_FIELD_GROUPS,
      fieldLabel: researchFieldLabel,
      sourceProvider: evidenceSourceProvider,
      // Rohe Leads tragen keine Kontakt-IDs; ohne Normalisierung stand bei
      // jedem Empfaenger "Sperrvermerk nicht geprueft", auch beim gesperrten
      // Jacob (Carbosulf 23.09.2026).
      recipientStatus: snapshot.recipientStatus,
      emailBelegt: (lead, contact) => kontaktEmailBelegt(lead, contact),
      identitaet: (lead, contact) => kontaktIdentitaet(contact),
      // Leeres Personenfeld: "nicht gefunden" nur, wenn fuer genau diese
      // Person ein Rechercheurteil gespeichert ist; sonst "nicht recherchiert".
      personFeldStatus: (lead, contact, feld) => {
        const eigen = contact?.field_status?.[feld];
        if (eigen?.status) return ['no_match', 'unsupported'].includes(eigen.status) ? 'nicht gefunden' : 'offen';
        const eintrag = lead?.field_status?.[feld];
        const key = String(contact?.person_key || '').trim();
        const gebunden = key && (String(eintrag?.person_key || '') === key
          || (eintrag?.sources || []).some((quelle) => String(quelle?.person_key || '') === key));
        if (!gebunden) return 'nicht recherchiert';
        return ['no_match', 'unsupported'].includes(eintrag?.status) ? 'nicht gefunden' : 'offen';
      },
      researchLabel: (lead) => labels.get(lead)?.research || 'Offen',
      sellifyLabel: (lead) => labels.get(lead)?.sellify || '—',
      jszipUrl: new URL('../../vendor/jszip/jszip.mjs', import.meta.url).href,
    });
    const fileName = exporter.exportFileName(label);
    exporter.downloadBlob(blob, fileName, eigenesDialogZiel() || document.body);
    zeigeHinweis(liste.length > 1 ? `Excel mit ${liste.length} Leads heruntergeladen.` : 'Excel heruntergeladen.');
    // The download has already happened; a failed receiver cannot discard it.
    try {
      await openResearchSnapshot(state.ctx?.actions, blob, fileName, snapshot);
    } catch (error) {
      console.warn('[outbound-lead-generation] Spreadsheet konnte nicht geöffnet werden', error);
      zeigeHinweis(`Excel heruntergeladen. Spreadsheet nicht geöffnet: ${error?.message || error}`);
    }
  } catch (error) {
    console.warn('[outbound-lead-generation] Excel-Export fehlgeschlagen', error);
    zeigeHinweis('');
    await showBusinessAlert(`Der Excel-Export ist fehlgeschlagen: ${error?.message || error}`);
  } finally {
    state.exportXlsxBusy = false;
    render();
  }
}

function researchLabel(lead, pendingIds = state.pendingResearchIds) {
  if (researchSubmissionPending(lead, pendingIds)) return 'Wird gestartet';
  if (lead.validation_status === 'validated') return 'Validiert';
  if (lead.research_status === 'queued') return 'Wartet';
  if (lead.research_status === 'running') return wartendeAusfuehrungsphase(lead) || 'Läuft';
  if (lead.research_status === 'completed') return 'Geprüft';
  if (lead.research_status === 'needs_review') return 'Prüfung nötig';
  if (lead.research_status === 'failed') return 'Unvollständig';
  return 'Offen';
}
function sellifyLabel(lead) {
  if (lead.sellify_status === 'queued') return uebergabeUnterbrochen(lead) ? 'Übergabe unterbrochen' : 'Übergabe läuft';
  if (lead.sellify_status === 'completed') return 'Übergeben';
  // Sellify hat geschrieben, nur die lokale Kopie steht noch aus.
  if (lead.sellify_status === 'pending_readback') return 'Übergeben · Bestätigung ausstehend';
  if (lead.sellify_status === 'failed') return 'Fehlgeschlagen';
  // Vor der Uebergabe sagt die Spalte, was der Vorabgleich weiss; bisher
  // stand bei allen Leads "—", auch bei den 13 bekannten Firmen.
  const precheck = lead?.payload?.sellify_precheck;
  if (precheck?.known && precheck.contact_id) return `bekannt · ${precheck.contact_id}`;
  if (precheck?.known === false) return 'neu';
  return '—';
}
function feldImEditor(fieldKey) {
  return Boolean(EDITOR_KEY_FOR_FIELD[fieldKey])
    || GOVERNANCE_FIELDS.includes(fieldKey)
    || EXTRA_COMPANY_EDIT_FIELDS.includes(fieldKey)
    || PERSON_EDIT_FIELDS.includes(fieldKey);
}

function researchFieldValue(lead, fieldKey) {
  const aliases = RESEARCH_FIELD_VALUE_KEYS[fieldKey] || [fieldKey];
  if (fieldKey.startsWith('person_')) {
    return String(firstValue(lead?.contacts?.[0], aliases) || '').trim();
  }
  const value = String(firstValue(lead?.data, aliases) || '').trim();
  if (value) return value;
  if (fieldKey === 'firma_name') return String(lead?.name || '').trim();
  if (fieldKey === 'firma_domain') return String(lead?.domain || lead?.website || '').trim();
  if (fieldKey === 'firma_ort') return String(lead?.city || '').trim();
  return sellifySnapshotValue(lead, fieldKey);
}
function fieldEvidenceSources(lead, fieldKey) {
  const sourcesByKey = new Map();
  const evidence = [
    ...(Array.isArray(lead?.evidence) ? lead.evidence : []),
    ...(Array.isArray(lead?.contacts?.[0]?.evidence) ? lead.contacts[0].evidence : []),
  ];
  for (const entry of evidence) {
    if ((entry?.field_key || entry?.field) !== fieldKey) continue;
    const key = evidenceSourceKey(entry);
    if (!key || isDocumentationSourceKey(key)) continue;
    const checkedAt = String(entry?.evidence_gate?.checked_at || '').trim();
    const sellifyBeleg = key === SELLIFY_SOURCE_ID;
    const current = sourcesByKey.get(key) || {
      key,
      label: sellifyBeleg ? SELLIFY_SOURCE_LABEL : String(entry?.label || entry?.source_id || key).trim(),
      // Ein Sellify-Datensatz hat keine Webadresse; kein toter Link.
      url: sellifyBeleg ? '' : String(entry?.source_url || entry?.url || '').trim(),
      confidence: String(entry?.confidence || '').trim(),
      note: String(entry?.note || '').trim(),
      checkedAt,
      countsAsExternal: entry?.counts_as_external !== false,
      evidenceEligible: entry?.evidence_gate?.evidence_eligible !== false
        && entry?.evidence_eligible !== false
        && !['blocked', 'unreachable', 'failed'].includes(String(entry?.verification_status || entry?.status || '').toLowerCase()),
    };
    if (!current.url && entry?.source_url && key !== SELLIFY_SOURCE_ID) current.url = String(entry.source_url).trim();
    if (!current.confidence && entry?.confidence) current.confidence = String(entry.confidence).trim();
    if (!current.note && entry?.note) current.note = String(entry.note).trim();
    // Ein echter Sellify-Beleg traegt die Kennung nur in der sellify://-Adresse;
    // die Anzeige nannte deshalb keinen Datensatz (Klicktest P3 REV-05b).
    if (sellifyBeleg && !current.note) {
      const adresse = String(entry?.source_url || entry?.url || '').trim();
      const treffer = adresse.match(/^sellify:\/\/(company|person)\/([^/?#]+)/i);
      if (treffer) current.note = treffer[1].toLowerCase() === 'company' ? `contact_id ${treffer[2]}` : `Person ${treffer[2]}`;
    }
    if (checkedAt && (!current.checkedAt || checkedAt > current.checkedAt)) current.checkedAt = checkedAt;
    sourcesByKey.set(key, current);
  }
  const crm = sellifySnapshotValue(lead, fieldKey);
  if (crm && !sourcesByKey.has(SELLIFY_SOURCE_ID)) {
    const current = researchFieldValue(lead, fieldKey);
    const agrees = !current || sellifyValuesAgree(fieldKey, crm, current);
    const contactId = String(lead?.payload?.sellify_snapshot?.contact_id || '').trim();
    sourcesByKey.set(SELLIFY_SOURCE_ID, {
      key: SELLIFY_SOURCE_ID,
      label: SELLIFY_SOURCE_LABEL,
      url: '',
      confidence: 'crm',
      note: [contactId ? `contact_id ${contactId}` : '', agrees ? '' : `führt abweichend: ${crm}`].filter(Boolean).join(' · '),
      checkedAt: '',
      countsAsExternal: agrees,
      evidenceEligible: agrees,
      sellifyAgrees: agrees,
    });
  }
  return [...sourcesByKey.values()].sort((left, right) => left.label.localeCompare(right.label, 'de'));
}
const SELLIFY_SNAPSHOT_FIELDS = Object.freeze({
  firma_name: 'name',
  firma_anschrift: 'anschrift',
  firma_plz: 'plz',
  firma_ort: 'ort',
  firma_land: 'land',
  firma_email: 'email',
  firma_telefon: 'telefon',
  firma_fax: 'fax',
  firma_domain: 'domain',
  wz_code: 'wz_code',
  mitarbeiter: 'mitarbeiter',
  umsatz: 'umsatz',
  sellify_nummer: 'contact_id',
});
function sellifySnapshotValue(lead, fieldKey) {
  const snapshot = lead?.payload?.sellify_snapshot;
  const key = SELLIFY_SNAPSHOT_FIELDS[fieldKey];
  if (!snapshot || !key) return '';
  return String(snapshot[key] ?? '').trim();
}
function comparableFieldValue(fieldKey, value) {
  const text = String(value || '').trim();
  if (['firma_telefon', 'firma_fax'].includes(fieldKey)) {
    // +49 (0) 7151 17155 und +49715117155 sind dieselbe Nummer.
    return text.replace(/\(0\)/g, '').replace(/\D/g, '').replace(/^0049/, '').replace(/^49/, '').replace(/^0/, '');
  }
  if (fieldKey === 'firma_domain') return normalizedDomain(text);
  if (fieldKey === 'mitarbeiter') return String(Math.round(numericBusinessMetric(text)));
  if (fieldKey === 'umsatz') return String(numericBusinessMetric(text, { millionScale: true }));
  return normalizeProtectionText(text)
    .replace(/ß/g, 'ss').replace(/ä/g, 'ae').replace(/ö/g, 'oe').replace(/ü/g, 'ue')
    .replace(/str\b/g, 'strasse')
    .replace(/\s+/g, '');
}
function sellifyValuesAgree(fieldKey, crmValue, currentValue) {
  const left = comparableFieldValue(fieldKey, crmValue);
  const right = comparableFieldValue(fieldKey, currentValue);
  if (!left || !right) return false;
  // Sellify fuehrt Kennzahlen gerundet: 104 Mio. € bestaetigt 104,8 Mio. €,
  // 104 Mitarbeitende bestaetigen 114 nicht.
  if (['mitarbeiter', 'umsatz'].includes(fieldKey)) {
    const a = Number(left);
    const b = Number(right);
    if (!Number.isFinite(a) || !Number.isFinite(b)) return false;
    const diff = Math.abs(a - b);
    return diff < 1 || diff / Math.max(a, b) <= 0.01;
  }
  // Die Recherche schreibt die Anschrift oft samt PLZ und Ort, Sellify nur
  // die Strasse: "Bahnhofstraße 35, 71332 Waiblingen" bestaetigt "Bahnhofstraße 35".
  if (fieldKey === 'firma_anschrift') return right.includes(left) || left.includes(right);
  return left === right;
}
// Regel 5: Sellify ist eine Quelle, belegt aber nie allein. Es zaehlt nur,
// wenn es den Wert bestaetigt, und mindestens eine externe Quelle muss dabei sein.
function fieldSourcesSatisfyRule(key, sources) {
  const counting = sources.filter(evidenceCountsForTwoSourceRule);
  return counting.length >= requiredIndependentSources(key)
    && counting.some((source) => source.key !== SELLIFY_SOURCE_ID);
}
// Ein Mensch, der ein Feld ausdruecklich freigibt oder den Wert selbst
// eintraegt, uebernimmt die Verantwortung dafuer. Bisher zaehlte das nicht:
// die Freigabe schrieb einen Beleg mit source_id "operator", und genau diese
// Quelle ist von der Quellenzaehlung ausgenommen. Der Knopf "freigeben" hob
// damit keine einzige Sperre auf, und ein von Hand eingetragener WZ-Code blieb
// ein "uebernommenes Feld ohne zwei externe Quellen" (gemessen 10.09.2026).
function personSchluessel(person) {
  return String(person?.person_key || person?.sellify_person_id || person?.id || '').trim();
}

function operatorAttestedField(lead, key) {
  const fieldKey = String(key || '').trim();
  if (!fieldKey) return false;
  const istPerson = fieldKey.startsWith('person_');
  const personKey = istPerson ? personSchluessel(lead?.contacts?.[0]) : '';
  // Personenfelder nur je Person; ein alter Eintrag "person_vorname" in
  // operator_approved_field_keys galt faelschlich fuer alle Personen.
  if (istPerson) {
    if (personKey && (lead?.payload?.operator_approved_person_fields || []).includes(`${fieldKey}@${personKey}`)) return true;
  } else if ((lead?.payload?.operator_approved_field_keys || []).includes(fieldKey)) {
    return true;
  }
  const current = String(researchFieldValue(lead, fieldKey) || '').trim();
  if (!current) return false;
  return (lead?.evidence || []).some((entry) => (entry?.field_key || entry?.field) === fieldKey
    && String(entry?.source_id || '').trim() === 'operator'
    && String(entry?.value ?? '').trim() === current
    && (!istPerson || !entry?.person_key || String(entry.person_key) === personKey));
}
function researchFieldReview(lead) {
  const groups = RESEARCH_FIELD_GROUPS.map((group) => ({
    id: group.id,
    label: group.label,
    fields: group.fields.map(([key, label]) => {
      const value = researchFieldValue(lead, key);
      const sources = fieldEvidenceSources(lead, key);
      const independentSources = sources.filter(evidenceCountsForTwoSourceRule);
      const contactConflicts = Array.isArray(lead?.contacts?.[0]?.conflicts) ? lead.contacts[0].conflicts : [];
      const conflict = (lead?.payload?.conflicting_field_keys || []).includes(key)
        || contactConflicts.some((entry) => (entry?.field_key || entry?.field) === key);
      return {
        key,
        label,
        value,
        filled: value !== '',
        sources,
        independentCount: independentSources.length,
        // Externe Anbieter getrennt von einem uebereinstimmenden Sellify-Eintrag:
        // Sellify allein belegt nie (Owner 23.09.2026).
        externalCount: independentSources.filter((source) => source.key !== SELLIFY_SOURCE_ID).length,
        sellifyAgrees: independentSources.some((source) => source.key === SELLIFY_SOURCE_ID),
        evidenceRequired: group.evidenceRequired !== false && !NON_EVIDENCE_REVIEW_FIELDS.has(key),
        conflict,
        operatorAttested: operatorAttestedField(lead, key),
        sufficient: group.evidenceRequired === false
          || NON_EVIDENCE_REVIEW_FIELDS.has(key)
          || (!conflict && fieldSourcesSatisfyRule(key, sources))
          || (!conflict && operatorAttestedField(lead, key)),
      };
    }),
  }));
  const optional = optionalResearchFields();
  for (const field of groups.flatMap((group) => group.fields)) {
    field.optional = optional.has(field.key);
    field.releasedEmpty = !field.filled && leerFreigegeben(lead, field.key);
    field.notFound = !field.filled && (researchAnsweredNotFound(lead, field.key) || field.releasedEmpty);
  }
  const fields = groups.flatMap((group) => group.fields);
  const researchFields = fields.filter((field) => RESEARCH_FIELD_SET.has(field.key));
  const researchedKeys = new Set((Array.isArray(lead?.payload?.researched_field_keys)
    ? lead.payload.researched_field_keys : []).map((key) => String(key || '')));
  const loadedEvidenceCount = (Array.isArray(lead?.evidence) ? lead.evidence.length : 0)
    + (Array.isArray(lead?.contacts?.[0]?.evidence) ? lead.contacts[0].evidence.length : 0);
  return {
    groups,
    fields,
    total: researchFields.length,
    maintenanceCount: fields.length - researchFields.length,
    filledCount: researchFields.filter((field) => field.filled).length,
    // Owner-Befund 07.09.2026: "nichts ist recherchiert obwohl Felder belegt
    // angegeben ist". "Belegt" zaehlt auch die Importfelder (Name, Adresse,
    // Domain). Recherchiert ist nur, was der Worker per Writeback geliefert hat.
    researchedCount: researchFields.filter((field) => field.filled
      && researchedKeys.has(field.key)).length,
    sufficientCount: researchFields.filter((field) => field.filled && field.sufficient).length,
    // Offen ist nur, was weder beantwortet noch optional ist.
    missingValueKeys: researchFields.filter((field) => !field.filled && !field.notFound && !field.optional).map((field) => field.key),
    notFoundKeys: researchFields.filter((field) => field.notFound).map((field) => field.key),
    missingEvidenceKeys: researchFields.filter((field) => !field.optional && field.evidenceRequired && field.filled && !field.sufficient).map((field) => field.key),
    conflictingKeys: researchFields.filter((field) => !field.optional && field.conflict).map((field) => field.key),
    optionalOpenKeys: researchFields.filter((field) => field.optional && ((!field.filled && !field.notFound) || field.conflict || (field.evidenceRequired && field.filled && !field.sufficient))).map((field) => field.key),
    // Traegt der Lead Belege, findet die Feldauswertung aber KEINE einzige Quelle,
    // ist der Datenstand unvollstaendig geladen — dann ist auch die Zusammenfassung
    // keine Messung. Siehe renderReviewFieldRow: am 11.08.2026 meldete die
    // Oberflaeche "0 mit mindestens zwei unabhaengigen Quellen belegt", waehrend
    // zwoelf Belege auf dem Server lagen.
    evidenceUnloaded: loadedEvidenceCount > 0
      && researchFields.every((field) => !field.sources.length),
  };
}
// Felder, deren eigene Beschriftung „oder explizit ,keine bekannt'" sagt, sind
// beantwortet, wenn die Recherche belegt KEINE gefunden hat. Der Rechercheweg
// drückt das als terminales `no_match` mit Begründung aus — nicht als Text im
// Feld. Gemessen am 09.09.2026: Carbosulf hatte WZ-Code, Umsatz, Prokura und
// Domain belegt und hing an „frühere Firmennamen fehlt", obwohl die Recherche
// dokumentiert festgestellt hatte, dass es keine gibt. Ein sauber belegtes
// „gibt es nicht" darf keine Freigabe blockieren.
const FELDER_MIT_LEERER_ANTWORT = new Set(['firma_fruehere_namen', 'firma_prokura']);
function fieldAnsweredAsNone(lead, key) {
  if (!FELDER_MIT_LEERER_ANTWORT.has(key)) return false;
  const status = lead?.field_status?.[key];
  if (String(status?.status || '').trim() !== 'no_match') return false;
  // Nur eine BEGRÜNDETE Fehlanzeige zählt; ein leeres no_match bleibt offen.
  return Boolean(String(status?.reason || '').trim());
}

function validationBlockers(lead) {
  return validationBlockerDetails(lead).map((entry) => entry.text);
}
function validationBlockerDetails(lead) {
  const blockers = [];
  if (researchSubmissionPending(lead)) blockers.push('Recherche wird gestartet');
  if (['queued', 'running'].includes(String(lead?.research_status || ''))) blockers.push('Recherche läuft noch');
  // Dieselbe Personensicht wie der Personenreiter (erste Person nach
  // Prioritaet): Freigaben gelten je Person, sonst wirkte eine Freigabe im
  // Standardreiter nicht auf die Freigabe-Blocker (Review 4).
  const review = researchFieldReview(contactTabLead(lead));
  const requiredFields = [
    ['firma_name', 'Firmenname'],
    ['firma_fruehere_namen', 'frühere Firmennamen oder explizit „keine bekannt“'],
    ['firma_aktivitaetsstatus', 'Aktivitätsstatus'],
    ['firma_domain', 'Domain'],
    ['firma_besucheranschrift', 'Besucheradresse'],
    ['firma_postanschrift', 'Postadresse'],
    ['firma_geschaeftstaetigkeit', 'Geschäftstätigkeit'],
    ['firma_homepage_fact_sheet', 'Homepage-Fact-Sheet'],
    ['firma_geschaeftsfuehrung', 'Geschäftsführung'],
    ['firma_prokura', 'Prokura oder explizit „keine gefunden“'],
    ['firma_email_domain_konflikt', 'E-Mail-/Domain-Konsistenz'],
    ['wz_code', 'WZ-Code'],
    ['firma_aufnahmeeignung', 'fachliche Eignung'],
    ['firma_ausschlussgrund', 'Ausschlussprüfung'],
    ['herkunft_import', 'Importherkunft'],
    ['adressquelle', 'Adressquelle'],
    ['verantwortlicher', 'Verantwortlicher'],
    ['listenstatus', 'Listenstatus'],
    ['aenderungsart', 'Änderungsart'],
    ['bearbeiter_initialen', 'Bearbeiterinitialen'],
    ['statistische_kampagne', 'statistische Kampagne'],
    ['fachliche_entscheidung_begruendung', 'Entscheidungsbegründung'],
  ];
  const optional = optionalResearchFields();
  const feldZuBlocker = new Map();
  for (const [key, label] of requiredFields) {
    if (optional.has(key)) continue;
    if (leerFreigegeben(lead, key)) continue;
    // Ein recherchierbares Feld, das die Recherche begruendet nicht gefunden
    // hat, ist beantwortet; Pflege- und Entscheidungsfelder bleiben Pflicht.
    const recherchierbar = RESEARCH_FIELD_SET.has(key);
    if (!researchFieldValue(lead, key) && !fieldAnsweredAsNone(lead, key)
      && !(recherchierbar && researchAnsweredNotFound(lead, key))) {
      blockers.push(`${label} fehlt`);
      feldZuBlocker.set(`${label} fehlt`, key);
    }
  }
  const activity = normalizeProtectionText(researchFieldValue(lead, 'firma_aktivitaetsstatus'));
  if (activity && !activity.includes('aktiv')) blockers.push('Unternehmen ist nicht als aktiv bestätigt');
  // "nicht geeignet" enthaelt "geeignet" und ging bisher als geeignet durch;
  // "keine Ausschlussgruende" wurde dagegen als nicht bestanden gesperrt
  // (Rundgang 11.09.2026). Verneinung zuerst, Ausschluss nach Wortanfang.
  const fit = normalizeProtectionText(researchFieldValue(lead, 'firma_aufnahmeeignung'));
  const fitNegated = /\b(nicht|ungeeignet|kein|keine|nein)\b/.test(fit);
  if (fit && (fitNegated || !['geeignet', 'passend', 'ja'].some((term) => fit.includes(term)))) blockers.push('fachliche Eignung ist nicht bestätigt');
  const exclusion = normalizeProtectionText(researchFieldValue(lead, 'firma_ausschlussgrund'));
  const exclusionPassed = /^(kein|nein|bestanden|nicht ausgeschlossen)/.test(exclusion);
  if (exclusion && !exclusionPassed) blockers.push('Ausschlussliste ist nicht bestanden');
  const domainConflict = normalizeProtectionText(researchFieldValue(lead, 'firma_email_domain_konflikt'));
  if (domainConflict && !['konsistent', 'kein konflikt', 'keine abweichung'].some((term) => domainConflict.includes(term))) blockers.push('E-Mail-/Domain-Konflikt ist nicht geklärt');
  const decision = normalizeProtectionText(researchFieldValue(lead, 'fachliche_aufnahmeentscheidung'));
  if (!optional.has('fachliche_aufnahmeentscheidung') && !['aufnehmen', 'freigeben', 'angenommen'].some((term) => decision.includes(term))) blockers.push('menschliche Aufnahmeentscheidung fehlt');
  const revenue = numericBusinessMetric(researchFieldValue(lead, 'umsatz'), { millionScale: true });
  const employees = numericBusinessMetric(researchFieldValue(lead, 'mitarbeiter'));
  if (!(revenue >= 35 || employees >= 210)) blockers.push('Schwelle 35 Mio. EUR oder 210 Mitarbeitende nicht belegt');
  // Ein Beleg, der die Art selbst nennt ("Northdata Kennzahlen: Umsatz 2024:
  // 50,9 Mio. EUR (Ist-Wert; Quelle: Jahresabschluss ...)"), kennzeichnet den
  // Wert; das Pflegefeld muss dann nicht zusaetzlich von Hand gesetzt werden.
  const artImBeleg = (feld) => (Array.isArray(lead?.evidence) ? lead.evidence : []).some((eintrag) => (eintrag?.field_key || eintrag?.field) === feld
    && /\b(Ist-Wert|Schätzung|Schaetzung)\b/i.test(String(eintrag?.quote || eintrag?.note || '')));
  if (revenue >= 35 && !researchFieldValue(lead, 'firma_umsatz_schaetzung') && !artImBeleg('umsatz')) blockers.push('Umsatz ist nicht als Ist-Wert oder Schätzung gekennzeichnet');
  if (employees >= 210 && !researchFieldValue(lead, 'firma_mitarbeiter_schaetzung') && !artImBeleg('mitarbeiter')) blockers.push('Mitarbeiterzahl ist nicht als Ist-Wert oder Schätzung gekennzeichnet');
  const suitableContact = (lead?.contacts || []).some((contact) => {
    const name = String(contact?.name || `${contact?.person_vorname || ''} ${contact?.person_nachname || ''}`).trim();
    const role = String(contact?.role || contact?.function || contact?.person_funktion || contact?.position || '').trim();
    return Boolean(name && role);
  });
  if (!suitableContact) blockers.push('geeigneter Ansprechpartner fehlt');
  // Eine Adresse zaehlt nur, wenn ein externes Zitat genau sie nennt; die
  // Zustellpruefung allein belegt nicht, dass sie zur Person gehoert. Der
  // fruehere Rueckfall auf das Lead-Feld person_email gab BNT mit Robert
  // Suesses nur aus Sellify stammender Adresse frei (23.09.2026).
  const validatedContact = (lead?.contacts || []).some((contact) => {
    const email = String(contact?.email || contact?.person_email || '').trim();
    return Boolean(email && kontaktEmailBelegt(lead, contact)
      && (emailVerdictIsDeliverable(contact?.email_validation || contact?.person_email_validation || '')
        || kontaktAdresseAusPrimaerquelle(lead, contact)));
  });
  // Der Serienbrief geht per Post; ist die persoenliche E-Mail optional,
  // blockiert eine fehlende gepruefte Adresse die Uebergabe nicht.
  // "ohne Wert freigeben" am Lead gilt auch fuer die Ansprechpartner-E-Mail;
  // vorher wurde die Freigabe gespeichert, der Punkt blieb aber offen.
  if (!validatedContact && !optional.has('person_email') && !leerFreigegeben(lead, 'person_email')) {
    const unbelegt = (lead?.contacts || []).filter((contact) => String(contact?.email || contact?.person_email || '').includes('@') && !kontaktEmailBelegt(lead, contact));
    const text = unbelegt.length
      ? `${emailValidationBlocker(lead)}; ${unbelegt.length} Kontakt-E-Mail(s) ohne externes Zitat, das die Adresse nennt`
      : emailValidationBlocker(lead);
    blockers.push(text);
    feldZuBlocker.set(text, 'person_email');
  }
  if (review.conflictingKeys.length) blockers.push(`${review.conflictingKeys.length} Feldkonflikt(e) ungeklärt`);
  if (review.missingEvidenceKeys.length) blockers.push(`${review.missingEvidenceKeys.length} übernommene Felder ohne belegte Quelle`);
  return blockers.map((text) => ({ text, key: feldZuBlocker.get(text) || '' }));
}
function metricOrZero(value) {
  return Number.isFinite(value) && value > 0 ? Math.round(value * 100) / 100 : 0;
}
function numericBusinessMetric(value, { millionScale = false } = {}) {
  const raw = String(value ?? '').trim().toLowerCase();
  if (!raw) return Number.NaN;
  // "rund 1.500 Mitarbeitende" wurde als 1,5 gelesen und fiel damit unter die
  // Schwelle von 210 (Zschimmer & Schwarz, Sasol; 11.09.2026). Ein Punkt vor
  // genau drei Ziffern trennt Tausender; bei Kopfzahlen auch ein Komma.
  const match = raw.replace(/\s/g, '').match(/-?\d{1,3}(?:\.\d{3})+(?:,\d+)?|-?\d+(?:[.,]\d+)?/);
  if (!match) return Number.NaN;
  let text = match[0];
  if (/^-?\d{1,3}(?:\.\d{3})+(?:,\d+)?$/.test(text)) text = text.replace(/\./g, '').replace(',', '.');
  else if (!millionScale && /^-?\d{1,3},\d{3}$/.test(text)) text = text.replace(',', '');
  else text = text.replace(',', '.');
  let number = Number(text);
  if (!Number.isFinite(number)) return Number.NaN;
  if (millionScale && /(?:mrd|milliard)/.test(raw)) number *= 1000;
  if (millionScale && !/(?:mio|million|mrd|milliard)/.test(raw) && number >= 1_000_000) number /= 1_000_000;
  return number;
}
function confidenceLabel(value) {
  const key = String(value || '').trim().toLowerCase();
  if (key === 'high') return 'hoch';
  if (key === 'medium') return 'mittel';
  if (key === 'low') return 'niedrig';
  return String(value || '').trim();
}
function formatEvidenceCheckedAt(value) {
  const time = Date.parse(String(value || ''));
  if (!Number.isFinite(time)) return '';
  return new Date(time).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });
}
// A lead that has never been researched has nothing to complain about yet.
// Red is reserved for "looked and found it wanting" — not for "not looked".
function leadIsUnresearched(lead) {
  if (!lead) return false;
  if (['queued', 'running', 'completed', 'needs_review', 'failed'].includes(lead.research_status)) return false;
  return !(lead.evidence || []).length;
}
// Belegampel (Owner-Vorgabe 09.09.2026): keine Quelle rot, ein bis zwei
// Quellen orange, ab drei Quellen gruen. Sie beurteilt, wie gut ein Wert
// getragen ist, und ist bewusst strenger als die Freigabe-Regel: die verlangt
// zwei unabhaengige Quellen, bei Selbstauskuenften eine. Ein Pflegefeld traegt
// keine Belegpflicht und bleibt deshalb neutral gruen.
const EVIDENCE_STRONG_MIN = 3;
// Belegampel ab 23.09.2026 (Owner): 0 unabhaengige Quellen = unbelegt (rot),
// 1 Quelle = akzeptiert (belegt), 2+ Quellen = staerker belegt. Nur ein
// Feld, dessen Quellen die Regel nicht erfuellen (z. B. nur Sellify), bleibt
// orange.
function evidenceToneClass(field) {
  if (field.evidenceRequired === false) return 'is-sufficient';
  const extern = Number(field.externalCount ?? field.independentCount) || 0;
  if (extern === 0 && !field.operatorAttested) return 'is-unsourced';
  if (!field.sufficient) return 'is-insufficient';
  return extern >= 2 ? 'is-sufficient is-strong' : 'is-sufficient is-single';
}

// Maschinenwerte in Lesefassung: der Worker liefert das Geschlecht als
// "maennlich"; angezeigt wird "männlich".
function displayFieldValue(key, value) {
  if (key === 'person_geschlecht') {
    const text = String(value || '').trim().toLowerCase();
    if (['maennlich', 'männlich', 'm', 'male', 'herr'].includes(text)) return 'männlich';
    if (['weiblich', 'w', 'f', 'female', 'frau'].includes(text)) return 'weiblich';
    if (['divers', 'd'].includes(text)) return 'divers';
  }
  return String(value ?? '');
}
// Ohne Wert freigegebene Felder bleiben in der Uebersicht sichtbar und
// ruecknehmbar. Die Ansprechpartner-E-Mail hat ohne Kontakt keine Feldzeile;
// ihre Freigabe liess sich sonst nicht mehr zuruecknehmen.
function renderReleasedEmptyFields(lead) {
  const keys = (lead?.payload?.operator_released_empty_field_keys || []).filter(Boolean);
  if (!keys.length) return '';
  const items = keys.map((key) => `<li>${escapeHtml(researchFieldLabel(key))} <button class="leadgen-approve-link" data-action="unrelease-empty-field" data-field="${escapeHtml(key)}">zurücknehmen</button></li>`).join('');
  return `<details class="leadgen-validation-details" data-disclosure="released-empty-fields"><summary>${keys.length} Feld(er) ohne Wert freigegeben</summary><ul>${items}</ul></details>`;
}
// Aus der Liste der offenen Punkte direkt handeln: ein Feld ohne gefundene
// Information fuer diesen Lead freigeben oder fuer alle Leads optional machen.
function renderBlockerActions(key) {
  if (!key) return '';
  const leer = RESEARCH_FIELD_SET.has(key) && leerFreigebbar(key)
    ? ` <button class="leadgen-approve-link" data-action="release-empty-field" data-field="${escapeHtml(key)}">ohne Wert freigeben</button>`
    : '';
  const optional = optionalResearchFields().has(key)
    ? ''
    : ` <button class="leadgen-approve-link" data-action="make-field-optional" data-field="${escapeHtml(key)}">für alle Leads optional</button>`;
  return leer + optional;
}

function renderReviewFieldRow(field, untouched = false, lead = null) {
  const stateClass = untouched
    ? 'is-untouched'
    : field.conflict ? 'is-conflict' : !field.filled ? 'is-missing' : evidenceToneClass(field);
  // Owner-Vorgabe: Wert, Quellen, fertig. Ein Badge als einziges Signal,
  // Quellen als eine kompakte Zeile ohne Vertrauens-/Meta-Prosa. Die
  // Nutzerfreigabe erscheint als Haekchen-Chip, nicht als Pseudo-Quelle.
  const realSources = field.sources.filter((source) => source.key !== 'operator');
  // Freigabe und Handeingabe unterscheiden: beide sind operator-Belege, aber
  // nur die Freigabe ist ein "✓ freigegeben" (Klicktest P3 #16).
  const operatorQuelle = field.sources.some((source) => source.key === 'operator');
  const anzeigePerson = field.key.startsWith('person_') ? personSchluessel(lead?.contacts?.[0]) : '';
  const approved = operatorQuelle && (anzeigePerson
    ? (lead?.payload?.operator_approved_person_fields || []).includes(`${field.key}@${anzeigePerson}`)
    : (lead?.payload?.operator_approved_field_keys || []).includes(field.key));
  const vonHandGepflegt = operatorQuelle && !approved;
  // "0 Quellen" ist eine AUSSAGE UEBER DIE WELT und darf nur fallen, wenn wir sie
  // treffen koennen. Am 11.08.2026 hing der Dienst 33 Stunden mit 4,2 GB fest,
  // die Replikation lieferte nichts mehr, und die Feldkarten meldeten fuer ANGUS
  // Chemie durchgaengig "0 Quellen" — waehrend serverseitig zwoelf Belege lagen.
  // Der Nutzer trifft auf solchen Zahlen Entscheidungen: er haette die Firma als
  // unbelegt verworfen. Ein stiller Anzeigefehler dieser Art ist schaedlicher als
  // ein sichtbarer Absturz.
  //
  // Solange der Lead nachweislich Belege traegt, die Feldauswertung aber keine
  // einzige Quelle findet, ist das ein unvollstaendiger Ladezustand und kein
  // Messergebnis. Dann sagt die Karte das auch.
  const leadEvidence = Array.isArray(lead?.evidence) ? lead.evidence : [];
  const belegeAmLead = leadEvidence.length;
  const anyMappedEvidence = leadEvidence.some((entry) => (
    evidenceSourceKey(entry) && String(entry?.field_key || entry?.field || '').trim()
  ));
  const auswertungLeer = field.evidenceRequired !== false && belegeAmLead > 0 && !anyMappedEvidence;
  const badge = field.conflict
    ? tr('conflictNeedsReview', 'Konflikt – Wert bleibt offen')
    : auswertungLeer
      ? tr('sourcesNotLoaded', 'Quellen nicht geladen')
      : field.releasedEmpty
      ? 'ohne Wert freigegeben'
      : untouched && !field.filled
      ? tr('notResearchedYet', 'noch nicht recherchiert')
      : !field.filled
        ? tr('missing', 'fehlt')
        : field.evidenceRequired === false
          ? tr('maintainedField', 'Pflegefeld')
          // Ein befuelltes Feld ohne einen einzigen Beleg ist kein Zaehlerstand,
          // sondern ein Mangel. Am 03.09.2026 gemessen: KUKA trug 49 Belege,
          // ALLE fuer Firmenfelder, null fuer Personen - jedes Personenfeld
          // zeigte ein neutrales "0 Quellen", obwohl der Wert unbelegt ist und
          // damit nicht freigabefaehig.
          // Gruen markiert und "unbelegt – nicht freigabefähig" zugleich war
          // widerspruechlich (Klicktest P3 REV-02a / V-03).
          : field.independentCount === 0 && operatorQuelle && field.sufficient
            ? (approved ? 'vom Nutzer freigegeben' : 'von Hand gepflegt')
          : field.independentCount === 0
            ? tr('unsourced', 'unbelegt – nicht freigabefähig')
            : field.externalCount === 0
              ? 'nur Sellify – unbelegt'
              : `${field.externalCount} ${field.externalCount === 1 ? 'externe Quelle' : 'externe Quellen'}${field.sellifyAgrees ? ' + Sellify' : ''}${field.sufficient
                ? (field.externalCount >= 2 ? ' · stärker belegt' : ' · belegt')
                : ` · ${tr('needsExternal', 'externe Quelle fehlt')}`}`;
  const sourceLinks = realSources.map((source) => {
    const name = source.url
      ? `<a href="${escapeHtml(source.url)}" target="_blank" rel="noopener">${escapeHtml(source.label)}</a>`
      : escapeHtml(source.label);
    if (source.key === SELLIFY_SOURCE_ID) {
      const kennung = String(source.note || '').replace(/\s*·\s*eigene gepflegte Angabe$/, '');
      const sichtbar = kennung ? `${name} · ${escapeHtml(kennung)}` : name;
      return `${sichtbar} <span class="leadgen-approved-chip">${tr('ownCrmValue', 'eigene Angabe')}</span>`;
    }
    return name;
  });
  if (approved) sourceLinks.push(`<span class="leadgen-approved-chip">✓ ${tr('approved', 'freigegeben')}</span>`);
  else if (vonHandGepflegt) sourceLinks.push('<span class="leadgen-approved-chip">✎ von Hand gepflegt</span>');
  // Frueher hatte ein GEFUELLTES Feld ueberhaupt keinen Bearbeitungsweg — nur
  // "freigeben". Ein falscher Wert liess sich nicht korrigieren. Jetzt ist der
  // Wert selbst anklickbar, und daneben steht der Weg ausdruecklich.
  // Nur Felder, die der Editor kennt, bekommen "ändern"/"eintragen" und einen
  // anklickbaren Wert; sonst oeffnete der Klick einen Editor ohne dieses Feld
  // (Klicktest P3 REV-01d: E-Mail-Pruefung).
  const editierbar = feldImEditor(field.key);
  const aendern = editierbar
    ? `<button class="leadgen-approve-link" data-action="edit-lead" data-field="${escapeHtml(field.key)}">${tr('changeValue', 'ändern')}</button>`
    : '';
  const leerAktion = !field.filled && RESEARCH_FIELD_SET.has(field.key) && leerFreigebbar(field.key)
    ? (field.releasedEmpty
      ? `<button class="leadgen-approve-link" data-action="unrelease-empty-field" data-field="${escapeHtml(field.key)}">zurücknehmen</button>`
      : `<button class="leadgen-approve-link" data-action="release-empty-field" data-field="${escapeHtml(field.key)}">ohne Wert freigeben</button>`)
    : '';
  const action = !field.filled
    ? `${editierbar ? `<button class="leadgen-approve-link" data-action="edit-lead" data-id="" data-field="${escapeHtml(field.key)}">${tr('enterValue', 'eintragen')}</button>` : ''}${leerAktion}`
    // Bei einem Konflikt loest "freigeben" nichts (der Konflikt bleibt offen);
    // der Weg ist "ändern" (Klicktest-Befund P3 #5).
    : (!field.sufficient && field.evidenceRequired !== false && !field.conflict
      ? `${aendern}<button class="leadgen-approve-link" data-action="approve-field" data-field="${escapeHtml(field.key)}">${tr('approveField', 'freigeben')}</button>`
      : aendern);
  return `<li class="leadgen-review-field ${stateClass}" data-review-field="${escapeHtml(field.key)}">
    <div class="leadgen-review-field-head">
      <span class="leadgen-review-label">${escapeHtml(field.label)}</span>
      ${editierbar
        ? `<button type="button" class="leadgen-review-value${field.filled ? '' : ' is-empty'}" data-action="edit-lead" data-field="${escapeHtml(field.key)}" title="${escapeHtml(tr('clickToEdit', 'Klicken zum Bearbeiten'))}">${field.filled ? escapeHtml(displayFieldValue(field.key, field.value)) : '—'}</button>`
        : `<span class="leadgen-review-value${field.filled ? '' : ' is-empty'}" title="Ergebnis der Recherche – nicht von Hand änderbar">${field.filled ? escapeHtml(displayFieldValue(field.key, field.value)) : '—'}</span>`}
      ${!field.filled && field.key === 'firma_land' && String(lead?.country || '').trim()
        ? `<span class="leadgen-muted" title="Land aus dem Import, noch nicht recherchiert">(Import: ${escapeHtml(String(lead.country).trim().toUpperCase())})</span>`
        : ''}
      <span class="leadgen-review-badge">${escapeHtml(badge)}</span>
      ${action}
    </div>
    ${sourceLinks.length ? `<div class="leadgen-review-sources-line">${sourceLinks.join(' · ')}</div>` : ''}
  </li>`;
}
// Der Grund eines gescheiterten Laufs wurde gespeichert, aber nirgends gezeigt.
function letzterRechercheFehler(lead) {
  const payload = lead?.payload || {};
  const text = String(payload.research_last_failure || (lead?.research_status === 'failed' ? payload.research_error : '') || '').trim();
  if (!text) return '';
  const fehlerAm = Number(payload.research_last_failure_at_ms || 0);
  const erfolgAm = Number(payload.research_finished_at_ms || 0);
  if (payload.research_last_failure && fehlerAm && erfolgAm > fehlerAm + 1000) return '';
  const datum = fehlerAm ? new Date(fehlerAm).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }) : '';
  return `Letzter Rechercheversuch${datum ? ` (${datum})` : ''}: ${text}`;
}
function renderResearchReviewSummary(review, lead) {
  const untouched = leadIsUnresearched(lead);
  const openCount = review.missingValueKeys.length + review.missingEvidenceKeys.length + review.conflictingKeys.length;
  const nichtGefunden = (review.notFoundKeys || []).length;
  const optionalOffen = (review.optionalOpenKeys || []).length;
  return `<section class="leadgen-detail-section leadgen-review-summary">
    <div class="leadgen-summary-heading"><h3>${tr('reviewTitle', 'Recherche')}</h3><span class="leadgen-status-pill">${escapeHtml(untouched ? tr('researchNotStartedShort', 'Nicht gestartet') : researchLabel(lead))}</span></div>
    ${review.evidenceUnloaded ? `<p class="leadgen-review-notice">${escapeHtml(tr('evidenceUnloadedTitle', 'Belege werden noch geladen'))}</p>` : ''}
    ${letzterRechercheFehler(lead) ? `<p class="leadgen-review-notice">${escapeHtml(letzterRechercheFehler(lead))}</p>` : ''}
    <div class="leadgen-review-metrics" aria-label="Recherchefortschritt">
      <div><strong>${untouched ? 0 : review.researchedCount}<span> / ${review.total}</span></strong><small>recherchiert</small></div>
      <div><strong>${review.filledCount}</strong><small>Werte vorhanden (inkl. Import)</small></div>
      <div><strong>${review.sufficientCount}</strong><small>ausreichend belegt</small></div>
      <div><strong>${untouched ? '—' : openCount}</strong><small>offene Prüfungen</small></div>
    </div>
    ${untouched ? '' : `<p class="leadgen-muted">${nichtGefunden} Feld(er) von der Recherche begründet „nicht gefunden“${optionalOffen ? ` · ${optionalOffen} optionale(s) Feld(er) ohne Wert` : ''} – blockieren die Freigabe nicht.</p>`}
    <p class="leadgen-muted">${escapeHtml(tr('campaignLabel', 'Kampagne'))}: ${escapeHtml(lead?.campaign || tr('annualResearch', 'Jahresrecherche'))}</p>
  </section>`;
}
function renderContactRecipientSelection(lead) {
  const normalized = normalizeLeadRecipientShape(lead || {});
  const selectedIds = new Set(normalized.selected_contact_ids);
  // Doppelte Kontakte aus mehreren Rechercheläufen zusammenfassen: gleicher
  // Name = ein Empfänger. Sonst erscheint "Arndt Schlosser" mehrfach.
  const seen = new Map();
  for (const contact of normalized.contacts) {
    const name = (contact.name || [contact.first_name || contact.person_vorname, contact.last_name || contact.person_nachname].filter(Boolean).join(' ')).trim();
    const key = name.toLowerCase() || contact.id;
    if (!seen.has(key)) seen.set(key, { ...contact, _name: name || tr('contact', 'Kontakt') });
  }
  const contacts = [...seen.values()];
  const selectedCount = contacts.filter((contact) => (
    selectedIds.has(contact.id) && currentContactEligibility(normalized, contact).status === 'free'
  )).length;
  const countLabel = `${selectedCount} / ${contacts.length}`;
  const rows = contacts.map((contact) => {
    const role = sellifyDeutsch(contact.role || contact.function || contact.position || contact.person_funktion || '');
    // Recherchierte Kontakte tragen die Adresse teils als Feldobjekt
    // ({value, status, sources}); als Text erschien "[object Object]" (22.09.2026).
    const alsText = (value) => (value && typeof value === 'object'
      ? String(value.value ?? value.email ?? value.address ?? '')
      : String(value ?? '')).trim();
    const email = alsText(contact.email) || alsText(contact.person_email);
    const detail = [alsText(role), email].filter(Boolean).join(' · ');
    const decision = currentContactEligibility(normalized, contact);
    const excluded = decision.status !== 'free';
    return `<label class="leadgen-recipient ${excluded ? `is-${escapeHtml(decision.status)}` : ''}">
      <input type="checkbox" data-action="toggle-contact-recipient" data-id="${escapeHtml(normalized.id || '')}" data-contact-id="${escapeHtml(contact.id)}" ${selectedIds.has(contact.id) && !excluded ? 'checked' : ''} ${excluded ? 'disabled' : ''}>
      <span class="leadgen-recipient-text">
        <span class="leadgen-recipient-name">${escapeHtml(contact._name)}</span>
        ${detail ? `<span class="leadgen-recipient-detail">${escapeHtml(detail)}</span>` : ''}
        ${excluded ? `<span class="leadgen-recipient-protection"><strong>${escapeHtml(decision.label)}</strong>${decision.reason && decision.reason !== decision.label ? ` · ${escapeHtml(decision.reason)}` : ''}</span>` : ''}
        ${!excluded && decision.hinweis ? `<span class="leadgen-recipient-detail">${escapeHtml(decision.reason)}</span>` : ''}
        ${decision.originalRemark ? `<q class="leadgen-recipient-remark">${escapeHtml(decision.originalRemark)}</q>` : ''}
      </span>
    </label>`;
  }).join('');
  return `<div class="leadgen-recipient-selection">
    <div class="leadgen-recipient-selection-head">
      <strong>${tr('sellifyRecipients', 'Empfänger für Sellify')}</strong>
      <span class="leadgen-recipient-count" data-selected-contact-count>${escapeHtml(countLabel)}</span>
    </div>
    ${rows || `<p class="leadgen-muted">${tr('noContacts', 'Noch keine Ansprechpartner.')}</p>`}
  </div>`;
}

function contactTabLead(lead) {
  const normalized = normalizeLeadRecipientShape(lead || {});
  if (!normalized.contacts.length) return normalized;
  const orderedContacts = normalized.contacts
    .map((contact, index) => ({ contact, index }))
    .sort((left, right) => {
      const priority = (entry) => {
        const role = String(entry?.role || entry?.function || entry?.person_funktion || entry?.position || entry?.person_position || '').toLowerCase();
        const exact = PERSON_RESEARCH_PRIORITIES.findIndex((value) => role.includes(value.toLowerCase()));
        if (exact >= 0) return exact;
        if (/gesch[aä]ftsführ|managing director|\bceo\b|gesamtverantwort|vorstand/.test(role)) return 0;
        if (/prokur|vertretungsberechtigt/.test(role)) return 1;
        if (/finanz|\bcfo\b|controlling/.test(role)) return 2;
        if (/einkauf|procurement|purchas/.test(role)) return 3;
        if (/supply chain|\bscm\b|logistik/.test(role)) return 4;
        if (/operations|betriebsleit|\bcoo\b/.test(role)) return 5;
        if (/technik|technisch|\bcto\b|technical/.test(role)) return 6;
        if (/entwicklung|f&e|r&d/.test(role)) return 7;
        return PERSON_RESEARCH_PRIORITIES.length;
      };
      return priority(left.contact) - priority(right.contact) || left.index - right.index;
    })
    .map(({ contact }) => contact);
  // Kontakt-IDs werden bei jeder Normalisierung neu vergeben; schreibt gerade
  // eine Recherche neue Ansprechpartner in den Lead, verschwindet die geklickte
  // ID und die Ansicht fiel auf den ersten Kontakt zurueck — der Wechsel wirkte
  // wie kaputt. Deshalb zusaetzlich ueber die Identitaet (Name + E-Mail)
  // aufloesen und den Rueckfall NICHT als Auswahl speichern, damit die Wahl des
  // Nutzers ueberlebt, sobald der Kontakt wieder auftaucht.
  const identityOf = (contact) => [
    personDisplayName(contact) || '',
    String(contact?.person_email || contact?.email || '').toLowerCase(),
  ].join('|').trim();
  const requested = state.activeContactTabs.get(normalized.id);
  const requestedId = String((requested && requested.id) || requested || '').trim();
  const requestedIdentity = String((requested && requested.identity) || '').trim();
  const requestedPersonKey = String((requested && requested.personKey) || '').trim();
  // Der Personenschluessel ueberlebt, was die Kontakt-Kennung NICHT ueberlebt:
  // die Sperrvermerkspruefung schreibt die Kontakte mit CRM-Daten neu, dabei
  // wechseln die aus Namen und Adresse gebildeten Kennungen und die Auswahl
  // fiel auf die erste Person zurueck (Owner-Befund 03.09.2026: "überall steht
  // weiterhin Hui Zhang"). Deshalb zuerst nach dem Schluessel suchen.
  const personKeyOf = (contact) => String(contact?.person_key || contact?.sellify_person_id || '').trim();
  let activeIndex = requestedPersonKey
    ? orderedContacts.findIndex((contact) => personKeyOf(contact) === requestedPersonKey)
    : -1;
  if (activeIndex < 0) activeIndex = orderedContacts.findIndex((contact) => contact.id === requestedId);
  if (activeIndex < 0 && requestedIdentity) {
    activeIndex = orderedContacts.findIndex((contact) => identityOf(contact) === requestedIdentity);
  }
  const fallback = activeIndex < 0;
  if (fallback) activeIndex = 0;
  const activeContact = orderedContacts[activeIndex];
  if (!fallback) {
    state.activeContactTabs.set(normalized.id, {
      id: activeContact.id,
      identity: identityOf(activeContact),
      personKey: personKeyOf(activeContact),
    });
  }
  // Personenbelege gehoeren zu genau einer Person. Bisher standen unter der
  // gezeigten Person auch die Belege der anderen: bei ANGUS hing unter David
  // Andrew Neubergers leerer E-Mail eine Sellify-Adresse einer anderen Person.
  const bindingKeys = (contact) => [contact?.person_key, contact?.sellify_person_id, contact?.id]
    .map((value) => String(value || '').trim()).filter(Boolean);
  const activeKeys = new Set(bindingKeys(activeContact));
  const otherKeys = new Set(orderedContacts
    .filter((_, index) => index !== activeIndex)
    .flatMap(bindingKeys)
    .filter((key) => !activeKeys.has(key)));
  const activeEmail = String(activeContact?.person_email || activeContact?.email || '').trim().toLowerCase();
  const scopedEvidence = (Array.isArray(normalized.evidence) ? normalized.evidence : []).filter((entry) => {
    const fieldKey = String(entry?.field_key || entry?.field || '');
    if (!fieldKey.startsWith('person_')) return true;
    // Ein E-Mail-Beleg, dessen Zitat eine andere Adresse nennt (info@, ein
    // Kollege, ein Muster), belegt die Adresse dieser Person nicht (BNT,
    // 23.09.2026). Sellify-Belege bleiben sichtbar, zaehlen aber nie extern.
    if (fieldKey === 'person_email' && evidenceSourceKey(entry) !== SELLIFY_SOURCE_ID
      && !(activeEmail && zitatNenntAdresse(entry?.quote, activeEmail))) return false;
    const entryKeys = [entry?.person_key, entry?.person_id, entry?.contact_ref]
      .map((value) => String(value || '').trim()).filter(Boolean);
    if (entryKeys.some((key) => activeKeys.has(key))) return true;
    if (entryKeys.some((key) => otherKeys.has(key))) return false;
    const own = String(firstValue(activeContact, RESEARCH_FIELD_VALUE_KEYS[fieldKey] || [fieldKey]) || '').trim().toLowerCase();
    return Boolean(own) && String(entry?.value ?? '').trim().toLowerCase() === own;
  });
  // Personenstatus gehoert ebenfalls zu genau einer Person (Codex-Review
  // 27.09.2026, zwei Gegenbeispiele): Kontakt B erbte das "no_match" von
  // person_email, das ausdruecklich an Person A gebunden war, und sein eigenes
  // contact.field_status wurde ignoriert. Jetzt: der eigene Status der Person
  // zuerst; ein Lead-Status, der an eine andere Person gebunden ist, gilt hier
  // nicht; ein ungebundener Alt-Status gilt weiter wie bisher.
  const fieldStatus = {};
  for (const [key, status] of Object.entries(normalized.field_status || {})) {
    if (key.startsWith('person_')) {
      const bindung = [status?.person_key, status?.person_id, status?.contact_ref]
        .map((value) => String(value || '').trim()).filter(Boolean);
      if (bindung.length && !bindung.some((value) => activeKeys.has(value))) continue;
    }
    fieldStatus[key] = status;
  }
  const eigenerStatus = activeContact?.field_status && typeof activeContact.field_status === 'object' ? activeContact.field_status : {};
  for (const [key, status] of Object.entries(eigenerStatus)) {
    if (key.startsWith('person_') && status && typeof status === 'object') fieldStatus[key] = status;
  }
  return {
    ...normalized,
    field_status: fieldStatus,
    evidence: [...scopedEvidence, ...kontaktQuellenAlsBelege(activeContact)],
    _contactTabOrder: orderedContacts,
    _activeContactId: activeContact.id,
    contacts: [
      activeContact,
      ...orderedContacts.filter((_, index) => index !== activeIndex),
    ],
  };
}

// Personenrecherchen schreiben ihre Belege als contact.sources ohne
// field_key ("Stephan Kreitz Prokura 09.07.2001", Carbosulf 23.09.2026). Die
// Ansicht las nur lead.evidence und zeigte solche Personen als unbelegt. Eine
// Quelle belegt hier genau die Felder, deren Wert ihr Zitat nennt.
function kontaktQuellenAlsBelege(contact) {
  const quellen = Array.isArray(contact?.sources) ? contact.sources : [];
  const personKey = String(contact?.person_key || '').trim();
  const text = (value) => String(value || '').trim();
  const nennt = (quote, value) => {
    const wert = normalizeProtectionText(value);
    return Boolean(wert) && normalizeProtectionText(quote).includes(wert);
  };
  const ziffern = (value) => String(value || '').replace(/\D/g, '');
  const belege = [];
  for (const quelle of quellen) {
    const quote = text(quelle?.quote);
    const url = text(quelle?.url || quelle?.source_url);
    if (!quote || !/^https?:\/\//i.test(url)) continue;
    const felder = [];
    if (nennt(quote, contact?.person_vorname || contact?.first_name)) felder.push('person_vorname');
    if (nennt(quote, contact?.person_nachname || contact?.last_name)) felder.push('person_nachname');
    for (const feld of ['person_funktion', 'person_position', 'person_titel']) {
      if (nennt(quote, contact?.[feld])) felder.push(feld);
    }
    const email = text(contact?.person_email || contact?.email).toLowerCase();
    if (email && zitatNenntAdresse(quote, email)) felder.push('person_email');
    const telefon = ziffern(contact?.person_telefon || contact?.phone);
    if (telefon.length >= 6 && ziffern(quote).includes(telefon.replace(/^0+/, '').slice(-8))) felder.push('person_telefon');
    for (const feld of ['person_linkedin', 'person_xing']) {
      const profil = text(contact?.[feld]).toLowerCase().replace(/^https?:\/\/(www\.)?/, '').replace(/\/$/, '');
      if (profil && url.toLowerCase().includes(profil)) felder.push(feld);
    }
    for (const feld of felder) {
      belege.push({
        field_key: feld,
        person_key: personKey,
        source_id: text(quelle?.source_id),
        source_url: url,
        quote,
        confidence: text(quelle?.confidence),
      });
    }
  }
  return belege;
}

function renderContactTabs(lead) {
  const contacts = Array.isArray(lead?._contactTabOrder) ? lead._contactTabOrder : (Array.isArray(lead?.contacts) ? lead.contacts : []);
  const activeId = lead?._activeContactId || contacts[0]?.id || '';
  // Owner-Vorgabe 30.08.: Die Recherche zielt auf MEHRERE Personen entlang
  // der Prioritaetenliste. Der Reiterblock zeigt deshalb pro Prioritaet einen
  // Slot - gefunden ODER ausdruecklich noch offen. Eine einzelne Person ohne
  // diesen Rahmen liess offen, ob der Rest fehlt oder nie gesucht wurde.
  const priorityOf = (contact) => {
    const role = String(contact?.role || contact?.function || contact?.person_funktion || contact?.position || contact?.person_position || '').toLowerCase();
    const exact = PERSON_RESEARCH_PRIORITIES.findIndex((value) => role.includes(value.toLowerCase()));
    if (exact >= 0) return exact;
    if (/gesch[aä]ftsführ|managing director|\bceo\b|gesamtverantwort|vorstand/.test(role)) return 0;
    if (/prokur|vertretungsberechtigt/.test(role)) return 1;
    if (/finanz|\bcfo\b|controlling/.test(role)) return 2;
    if (/einkauf|procurement|purchas/.test(role)) return 3;
    if (/supply chain|\bscm\b|logistik/.test(role)) return 4;
    if (/operations|betriebsleit|\bcoo\b/.test(role)) return 5;
    if (/technik|technisch|\bcto\b|technical/.test(role)) return 6;
    if (/entwicklung|f&e|r&d/.test(role)) return 7;
    return PERSON_RESEARCH_PRIORITIES.length;
  };
  const groups = PERSON_RESEARCH_PRIORITIES.map((label) => ({ label, contacts: [] }));
  const rest = { label: tr('otherContacts', 'Weitere'), contacts: [] };
  for (const contact of contacts) {
    const index = priorityOf(contact);
    (index < groups.length ? groups[index] : rest).contacts.push(contact);
  }
  // Jede Gruppe ist eine eigene Reiterleiste und braucht einen per Tab
  // erreichbaren Eintrag (die aktive Person oder die erste); sonst waren
  // andere Gruppen per Tastatur unerreichbar (Klicktest P2 KEY-04 / V12).
  const chip = (contact, tabbar = false) => {
    const name = personDisplayName(contact) || tr('contact', 'Kontakt');
    const role = String(contact.role || contact.function || contact.person_funktion || contact.position || contact.person_position || '').trim();
    const active = contact.id === activeId;
    return `<button type="button" class="leadgen-contact-tab${active ? ' is-active' : ''}" role="tab" aria-selected="${active}" tabindex="${active || tabbar ? '0' : '-1'}" data-action="select-contact-tab" data-person-key="${escapeHtml(String(contact.person_key || contact.sellify_person_id || ''))}" data-lead-id="${escapeHtml(lead.id || '')}" data-contact-id="${escapeHtml(contact.id || '')}" data-contact-identity="${escapeHtml([personDisplayName(contact) || '', String(contact.person_email || contact.email || '').toLowerCase()].join('|').trim())}">
        <span>${escapeHtml(name)}</span>${role ? `<small>${escapeHtml(role)}</small>` : ''}
      </button>`;
  };
  // Owner-Vorgabe 03.09.: Der Block darf nicht den halben Bildschirm fuellen.
  // Gefundene Personen stehen in EINER umbrechenden Reihe, gruppiert nach
  // Prioritaet; die noch offenen Prioritaeten stehen als eine einzige Zeile
  // darunter, damit weiterhin sichtbar bleibt, was fehlt.
  const filled = [...groups, rest].filter((group) => group.contacts.length);
  const openLabels = [...groups, rest].filter((group) => !group.contacts.length).map((group) => group.label);
  const filledMarkup = filled.map((group) => `
    <span class="leadgen-person-group" role="tablist" aria-label="${escapeHtml(group.label)}">
      <span class="leadgen-person-group-label">${escapeHtml(group.label)}</span>
      ${group.contacts.map((contact, index) => chip(contact, index === 0 && !group.contacts.some((entry) => entry.id === activeId))).join('')}
    </span>`).join('');
  const openMarkup = openLabels.length
    ? `<div class="leadgen-person-open">${tr('slotOpen', 'offen')}: ${escapeHtml(openLabels.join(' · '))}</div>`
    : '';
  return `<div class="leadgen-contact-tabs leadgen-person-slots" aria-label="Ansprechpartner nach Priorität">
    <div class="leadgen-person-row">${filledMarkup || `<span class="leadgen-person-slot-empty">${escapeHtml(tr('noContacts', 'Noch keine Person recherchiert.'))}</span>`}</div>
    ${openMarkup}
  </div>`;
}

const DETAIL_TAB_IDS = ['overview', 'company', 'contact', 'classification'];
// Eine Funktion statt einer const: restoreActiveDetailTab laeuft schon bei
// der Initialisierung von `state` (weit oben); die const stand dort noch in der
// "temporal dead zone", der ReferenceError fiel ins catch, und der gespeicherte
// Reiter wurde nie wiederhergestellt (Klicktest P3 DET-02b).
function detailTabSpeicherSchluessel() { return 'ctox.olg.detail-tab.v1'; }
function restoreActiveDetailTab() {
  try {
    const stored = globalThis.localStorage?.getItem(detailTabSpeicherSchluessel()) || '';
    return ['overview', 'company', 'contact', 'classification'].includes(stored) ? stored : 'overview';
  } catch {
    return 'overview';
  }
}
function setActiveDetailTab(tabId) {
  state.activeDetailTab = tabId;
  try {
    globalThis.localStorage?.setItem(detailTabSpeicherSchluessel(), tabId);
  } catch {
    // Ohne Browserspeicher gilt der Reiter nur bis zum Neuladen.
  }
}
function renderDetailTabs(lead, activeTab) {
  const tabs = [
    ['overview', 'Übersicht'],
    ['company', 'Unternehmen'],
    ['contact', 'Personen'],
    ['classification', 'Einordnung'],
  ];
  return `<nav class="leadgen-detail-tabs" role="tablist" aria-label="Lead-Details">
    ${tabs.map(([id, label]) => `<button type="button" class="leadgen-detail-tab${activeTab === id ? ' is-active' : ''}" role="tab" aria-selected="${activeTab === id}" tabindex="${activeTab === id ? '0' : '-1'}" data-action="select-detail-tab" data-lead-id="${escapeHtml(lead.id || '')}" data-detail-tab="${id}">${escapeHtml(label)}</button>`).join('')}
  </nav>`;
}

function renderResearchReviewGroups(review, lead, activeGroup = '') {
  const untouched = leadIsUnresearched(lead);
  // Der fruehere Entscheidungs-Tab ist aufgeloest: seine Pflegefelder
  // erscheinen unter Einordnung, die Aktions-Buttons in der Uebersicht.
  const groupMatches = (group) => !activeGroup
    || group.id === activeGroup
    || (activeGroup === 'classification' && group.id === 'governance');
  return review.groups.filter(groupMatches).map((group) => {
    // Owner-Vorgabe 31.08.: Die Sellify-Empfaengerauswahl steht in der Uebersicht.
    const recipientSelection = '';
    return `<section class="leadgen-detail-section leadgen-review-group" data-review-group="${escapeHtml(group.id)}">
      <h3>${escapeHtml(group.label)}</h3>
      ${group.id === 'contact' ? renderContactTabs(lead) : ''}
      <ul class="leadgen-review-fields">${group.fields.map((field) => renderReviewFieldRow(field, untouched, lead)).join('')}</ul>
      ${recipientSelection}
    </section>`;
  }).join('');
}
function renderResearchReview(lead) {
  const contactScopedLead = contactTabLead(lead);
  const review = researchFieldReview(contactScopedLead);
  return renderResearchReviewSummary(review, contactScopedLead) + renderResearchReviewGroups(review, contactScopedLead);
}
function campaignResearchProgress(campaign, leads, run = null) {
  const scoped = (leads || []).filter((lead) => leadKampagnen(lead).includes(String(campaign || '').trim()));
  const counts = { new: 0, queued: 0, running: 0, completed: 0, failed: 0, validated: 0 };
  for (const lead of scoped) {
    const status = effektiverRechercheStatus(lead);
    if (lead.validation_status === 'validated') counts.validated += 1;
    else if (status === 'failed') counts.failed += 1;
    else if (status === 'queued') counts.queued += 1;
    else if (status === 'running') counts.running += 1;
    else if (['completed', 'needs_review'].includes(lead.research_status)) counts.completed += 1;
    else counts.new += 1;
  }
  const total = scoped.length;
  const actionable = campaignResearchQueue(scoped).length;
  const processed = counts.completed + counts.failed + counts.validated;
  // Der dauerhafte Parent-Task ist die Autoritaet fuer einen laufenden Start.
  // Lead-Projektionen duerfen nach einem WebRTC-Abbruch noch auf "new" stehen;
  // trotzdem muss die UI den bereits angenommenen Task anzeigen und einen
  // doppelten Kampagnenstart verhindern.
  const active = ['queued', 'running'].includes(run?.status);
  const running = active || counts.running > 0;
  const queued = run?.status === 'queued' || counts.queued > 0;
  const trackedLead = scoped.find((lead) => (
    ['queued', 'running'].includes(lead.research_status)
    && String(lead.task_id || '').trim()
    && String(lead.command_id || '').trim()
  ));
  const campaignTrackingLead = scoped.find((lead) => (
    String(lead?.payload?.campaign_task_id || '').trim()
    && String(lead?.payload?.campaign_command_id || '').trim()
  ));
  let status = 'new';
  if (run?.status === 'running' || counts.running > 0) status = 'running';
  else if (queued) status = 'queued';
  else if (total > 0 && counts.validated === total) status = 'validated';
  else if (run?.status === 'failed' || (processed === total && counts.failed > 0)) status = 'failed';
  else if (total > 0 && processed === total) status = 'completed';
  return {
    status,
    active,
    queued,
    running,
    total,
    actionable,
    processed,
    percent: total > 0 ? Math.round((processed / total) * 100) : 0,
    counts,
    currentLeadId: run?.currentLeadId || '',
    currentLeadName: run?.currentLeadName || '',
    trackingTaskId: active ? String(run?.taskId || campaignTrackingLead?.payload?.campaign_task_id || trackedLead?.task_id || '').trim() : '',
    trackingCommandId: active ? String(run?.commandId || campaignTrackingLead?.payload?.campaign_command_id || trackedLead?.command_id || '').trim() : '',
    error: run?.error || '',
  };
}

async function openCtoxTask(taskId, commandId) {
  const normalizedTaskId = String(taskId || '').trim();
  const normalizedCommandId = String(commandId || '').trim();
  if (!normalizedTaskId || !normalizedCommandId) {
    showBusinessAlert('Für diese Automatisierung wurde kein verfolgbarer CTOX Task bestätigt.');
    return;
  }
  const focus = {
    taskId: normalizedTaskId,
    commandId: normalizedCommandId,
    taskStatus: 'queued',
    sourceModule: 'outbound-lead-generation',
    openDrawer: true,
  };
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
  } catch {}
  const params = new URLSearchParams({
    task_id: normalizedTaskId,
    command_id: normalizedCommandId,
    task_status: 'queued',
    source: 'outbound-lead-generation',
    drawer: '1',
  });
  await state.ctx?.businessChat?.open?.({
    title: 'Kampagnenrecherche',
    task_id: normalizedTaskId,
    command_id: normalizedCommandId,
    focus: {
      task_id: normalizedTaskId,
      command_id: normalizedCommandId,
    },
    source_module: 'outbound-lead-generation',
    reuseActive: false,
  });
  location.hash = `#ctox?${params.toString()}`;
  await state.ctx?.openApp?.('ctox');
}
function campaignResearchStatusLabel(status) {
  const labels = {
    new: tr('statusNew', 'Neu'),
    queued: tr('statusQueued', 'Wartet'),
    running: tr('statusRunning', 'Laufend'),
    completed: tr('statusCompleted', 'Abgeschlossen'),
    failed: tr('statusFailed', 'Unvollständig'),
    validated: tr('statusValidated', 'Validiert'),
  };
  return labels[status] || labels.new;
}
function domainFromUrl(value) { try { return new URL(value).hostname.replace(/^www\./, ''); } catch { return ''; } }
function fingerprint(value) { let hash = 2166136261; for (const char of String(value).toLowerCase()) { hash ^= char.charCodeAt(0); hash = Math.imul(hash, 16777619); } return (hash >>> 0).toString(36); }
function sanitizeCommandResult(result) { return { status: result?.status || '', command_id: result?.command_id || '', task_id: result?.task_id || '', error: result?.error || '', secret_value_in_payload: false }; }
function escapeHtml(value) { return String(value ?? '').replace(/[&<>'"]/g, (char) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' })[char]); }
function tr(key, fallback) { return String(state.messages?.[key] || fallback || key); }

function startResize(event) {
  const handle = event.target.closest('[data-resizer]');
  if (!handle) return;
  event.preventDefault();
  const layout = state.ctx.host.querySelector('.outbound-lead-generation-layout');
  const side = handle.dataset.resizer;
  const startX = event.clientX;
  const styles = getComputedStyle(layout);
  const start = Number.parseFloat(styles.getPropertyValue(side === 'left' ? '--leadgen-left' : '--leadgen-right')) || (side === 'left' ? 300 : 360);
  const move = (moveEvent) => {
    const delta = moveEvent.clientX - startX;
    const value = side === 'left' ? start + delta : start - delta;
    layout.style.setProperty(side === 'left' ? '--leadgen-left' : '--leadgen-right', `${Math.max(220, Math.min(520, value))}px`);
  };
  const stop = () => { globalThis.removeEventListener('pointermove', move); globalThis.removeEventListener('pointerup', stop); };
  globalThis.addEventListener('pointermove', move);
  globalThis.addEventListener('pointerup', stop, { once: true });
}

function icon(name) {
  const paths = {
    plus: '<path d="M12 5v14M5 12h14"/>',
    close: '<path d="m6 6 12 12M18 6 6 18"/>',
    settings: '<path d="M12 3a2 2 0 0 1 2 2v1.2a7 7 0 0 1 1.7 1l1-.6a2 2 0 0 1 2.7.7l.6 1a2 2 0 0 1-.7 2.7l-1 .6a7 7 0 0 1 0 2l1 .6a2 2 0 0 1 .7 2.7l-.6 1a2 2 0 0 1-2.7.7l-1-.6a7 7 0 0 1-1.7 1V19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-1.2a7 7 0 0 1-1.7-1l-1 .6a2 2 0 0 1-2.7-.7l-.6-1a2 2 0 0 1 .7-2.7l1-.6a7 7 0 0 1 0-2l-1-.6a2 2 0 0 1-.7-2.7l.6-1a2 2 0 0 1 2.7-.7l1 .6a7 7 0 0 1 1.7-1V5a2 2 0 0 1 2-2h1Z"/><circle cx="11.5" cy="12" r="2.5"/>',
    code: '<path d="m8 9-3 3 3 3M16 9l3 3-3 3M14 5l-4 14"/>',
    check: '<path d="m5 12 4 4L19 6"/>',
    wrench: '<path d="M14.7 6.3a4 4 0 0 0-5 5L3 18l3 3 6.7-6.7a4 4 0 0 0 5-5l-2.4 2.4-3-3 2.4-2.4Z"/>',
    test: '<path d="M9 3h6M10 3v5l-5 9a2 2 0 0 0 2 3h10a2 2 0 0 0 2-3l-5-9V3M8 14h8"/>',
    login: '<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4M10 17l5-5-5-5M15 12H3"/>',
    key: '<circle cx="7.5" cy="15.5" r="3.5"/><path d="m10 13 8-8M16 7l2 2M13 10l2 2"/>',
    import: '<path d="M12 3v12m0 0 4-4m-4 4-4-4M5 19h14"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>',
    spinner: '<path d="M20 12a8 8 0 1 1-2.3-5.7"/>',
    send: '<path d="m22 2-7 20-4-9-9-4Z"/><path d="M22 2 11 13"/>',
    external: '<path d="M14 3h7v7M10 14 21 3"/><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5"/>',
    book: '<path d="M4 5a3 3 0 0 1 3-3h5v18H7a3 3 0 0 0-3 2V5Z"/><path d="M20 5a3 3 0 0 0-3-3h-5v18h5a3 3 0 0 1 3 2V5Z"/>',
    edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z"/>',
    trash: '<path d="M4 7h16M9 7V4h6v3M7 7l1 14h8l1-14M10 11v6M14 11v6"/>',
    shards: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
    table: '<path d="M4 5h16v14H4zM4 10h16M4 15h16M10 5v14"/>',
    download: '<path d="M12 4v11M7 10l5 5 5-5M5 19h14"/>',
  };
  return `<svg class="leadgen-icon" viewBox="0 0 24 24" aria-hidden="true">${paths[name] || paths.check}</svg>`;
}

export const __leadgenOutboundTestHooks = {
  listLeads,
  ensureFullLeads,
  loadSelectedLeadDetails,
  prepareFullLeadAction,
  renderOptionalFieldSettings,
  saveOptionalFields,
  handleClick,
  requiredResearchFieldCount,
  exportResearchXlsx,
  analyzeImportPayload,
  importPreview,
  importPayload,
  derivedMaintenanceValues,
  operatorAttestedField,
  emailVerdictIsDeliverable,
  adapterCommandOperation,
  // Exported so the flow can be EXECUTED in tests, not only grepped:
  // every source-level assertion stayed green while renderDetail threw
  // and no recipient selection could persist.
  sellifyHandoffPrecondition,
  sendLeadToSellify,
  setContactRecipientSelection,
  buildCampaignRecipientList,
  classifySellifyPerson,
  deriveLeadRecipientEligibility,
  sellifyFreitexte,
  freitextSignatur,
  contactTabLead,
  researchAnsweredNotFound,
  setEmptyFieldRelease,
  patchLead,
  freitextPruefungPrompt,
  freitextPakete,
  freitextLeadDaten,
  freitextBestaetigungAusstehend,
  vermerkPruefungErledigt,
  reload,
  renderSyncLine,
  bindCollections,
  recoverCommandChannel,
  scheduleCollectionReload,
  testState: () => state,
  planeLeerNachladen,
  recipientEligibilitySignature,
  recipientSignatureHash,
  verarbeiteFreitextAntworten,
  repairUntrackedResearchStatuses,
  sendeFreitextPruefung,
  leseFreitextAntwort,
  baueFreitextUrteil,
  loadSellifyRecipientContext,
  normalizeProtectionText,
  repairLeadRecipientSelections,
  refreshLeadRecipientEligibility,
  renderCampaignRecipientExclusions,
  adapterCommandRecordPatch,
  adapterReady,
  campaignResearchProgress,
  campaignResearchCommandForCampaign,
  campaignRunFromCommand,
  campaignTaskStatus,
  campaignResearchPrompt,
  campaignResearchQueue,
  campaignQueuedLeadPatch,
  staleCampaignParentPlaceholder,
  terminalCampaignQueuedLeadPatch,
  enabledSourcePolicy,
  enabledPrivateResearchSources,
  evidenceSourceKey,
  fieldEvidenceSources,
  independentEvidenceCount,
  sourceEvidenceGroups,
  independentFieldEvidenceCount,
  leadReadyForValidation,
  researchSubmissionPending,
  researchLabel,
  updateDigestSinceMs,
  renderResearchReview,
  researchFieldReview,
  uebernehmeCrmKontaktdaten,
  firmenSchluessel,
  firmenNamensvarianten,
  refreshAllRecipientEligibility,
  findSellifyCompanyDuplicate,
  researchFieldValue,
  validationBlockers,
  RESEARCH_FIELD_GROUPS,
  RESEARCH_FIELDS,
  GOVERNANCE_FIELDS,
  NON_EVIDENCE_REVIEW_FIELDS,
  numericBusinessMetric,
  leadResearchMode,
  normalizedResearchCountry,
  openCampaignResearchChat,
  requireTrackedSubmission,
  repairUntrackedResearchStatuses,
  researchPolicyInstructions,
  researchPolicyRecord,
  normalizeResearchFieldKeys,
  activeResearchFields,
  adapterConfigurationDigest,
  adapterReconciliationSource,
  sourceCredentialSecretName,
  researchOutcomeWriteback,
  researchCommandLeadPatch,
  researchCommandForLead,
  newerResearchCommandCanRecoverLead,
  researchCommandObservationKey,
  reconcileResearchCommands,
  ausfuehrungsphasePatch,
  effektiverRechercheStatus,
  payloadWithoutResearchResults,
  researchResetLeadPatch,
  sellifyVorwissenAlsText,
  sellifyCrmBeleg,
  sellifySourceStatus,
  listedSources,
  isInternalResearchSource,
  SELLIFY_SOURCE_ID,
  sellifyVorwissenAusFirma,
  sellifySnapshotAusVorwissen,
  sellifyValuesAgree,
  sellifyLabel,
  SELLIFY_SOURCE_LABEL,
  normalizedResearchCommandStatus,
  uniqueCommands,
  resetLeadForImport,
  rxdbIdSlug,
  sellifyCompanyValues,
  sellifyPersonValues,
  sellifyWritebackContract,
  sourceNeedsBrowserAuthorization,
  sourceStatus,
  SOURCE_DEFS,
  seedSources,
  repairAdapterActivationDrift,
  // Exposed so a test can actually RENDER and inspect the markup. A source-only
  // check cannot catch a render that produces structurally empty output — a
  // truncated template still parses and still passes every unit test, and the
  // app ships blank. See tests/render-smoke.
  __render: {
    setState: (next) => Object.assign(state, next),
    getState: () => state,
    render,
    renderCampaigns,
    renderCenter,
    renderDetail,
  },
};
