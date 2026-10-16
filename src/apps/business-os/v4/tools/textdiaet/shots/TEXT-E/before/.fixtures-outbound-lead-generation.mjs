// Shell v4 Geometrie-Labor – Fixture-Erzeuger.
// Erzeugt realistische Datensätze je Collection aus collections.schema.json (Pflichtfelder plus übliche Felder),
// mit lesbaren Namen in Titelfeldern statt UUIDs. Deterministisch (gleiche Eingabe → gleiche Daten).
//
//   import { buildFixtures } from './fixtures.mjs';
//   const { schemas, seed } = buildFixtures('notes');   // seed: { collection: rows[] }
//
//   node fixtures.mjs <appId>   → JSON auf stdout (zum Ansehen)
//
// Quellen: modules/<id>/collections.schema.json (eigene Collections) und – für foreign_collections aus
// v4/manifests/<id>.manifest.json – das Schema der besitzenden App (erste App, die die Collection definiert).
// Werte für status/kind/state/type/… werden, wo möglich, aus den String-Vergleichen im App-Code abgeleitet, damit
// Filter der App die Zeilen nicht verwerfen.
import { readFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = "/Users/michaelwelsch/Documents/ctox-shell-v4/src/apps/business-os/v4/tools/geometry";
export const BOS = join(HERE, '..', '..', '..');
const MODULES = "/Users/michaelwelsch/Documents/ctox-shell-v4/src/apps/business-os/customer-modules";
const MANIFESTS = join(BOS, 'v4', 'manifests');

// System-Collections und reine Binär-/Protokollablagen bekommen keine Zeilen.
const SKIP = new Set(['business_commands', 'business_apps', 'business_app_versions', 'shell_preferences', 'files']);
const SKIP_RE = /(_chunks|_blob_chunks|_input_events|_frames|_event_routing_state|_reporting_events)$/;
const ROWS_DEFAULT = 6;
const ROWS_FOREIGN = 4;

// Gemeinsamer deutscher Datensatz (Strang SEED): dieselben 12 Firmen, Ansprechpartner, Beträge in allen Apps.
// Dynamisch über HERE geladen, damit Kopien dieser Datei (rundgang2 customerConfig) dieselben Daten finden.
const DATA = await import(pathToFileURL(join(HERE, 'fixtures-data.mjs')).href);
const { COMPANIES, STAFF, OWN_DOMAIN, MAIL_SUBJECTS, MAIL_BODIES, TITLES, SENTENCES, TAGS, FILES, NET_EUR, VAT, UNIT_EUR, DEALS, personEmail, contactForCompany, companyOf } = DATA;
const ROWS_COMPANY = COMPANIES.length; // Firmen-/Kontakt-Collections: alle 12 Firmen
const GENERIC_STATUS = ['open', 'active', 'in_progress', 'done', 'draft', 'new'];
const BASE_MS = Date.UTC(2026, 9, 14, 9, 30); // 14.10.2026 09:30 UTC

const sch = (id) => { const c = join(MODULES, id, 'collections.schema.json'); return existsSync(c) ? c : join("/Users/michaelwelsch/Documents/ctox-shell-v4/src/apps/business-os/modules", id, 'collections.schema.json'); };

function readJson(file) {
  try { return JSON.parse(readFileSync(file, 'utf8')); } catch { return null; }
}

let ownerIndex = null;
// Collection → {app, schema} (erste definierende App, alphabetisch)
export function schemaIndex() {
  if (ownerIndex) return ownerIndex;
  ownerIndex = new Map();
  for (const id of readdirSync(MODULES).sort()) {
    if (!statSync(join(MODULES, id)).isDirectory()) continue;
    const d = readJson(sch(id));
    for (const [c, s] of Object.entries(d?.collections || {})) if (!ownerIndex.has(c)) ownerIndex.set(c, { app: id, schema: s });
  }
  return ownerIndex;
}

// Werte, die der App-Code mit Feldern vergleicht: status === 'open', kind: 'invoice', case 'draft'
function codeLiterals(appId) {
  const out = new Map();
  const dir = join(MODULES, appId);
  const files = [];
  const walk = (d, depth = 0) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      const st = statSync(p);
      if (st.isDirectory()) { if (depth < 2 && !/^(tests?|handlers|locales|assets|node_modules)$/.test(n)) walk(p, depth + 1); }
      else if (/\.m?js$/.test(n) && !/test|smoke|e2e/.test(n)) files.push(p);
    }
  };
  if (existsSync(dir)) walk(dir);
  const re = /\b([a-z_]{3,32})\s*(?:===|!==|==|!=)\s*['"]([a-z][a-z0-9_-]{1,30})['"]|\b([a-z_]{3,32})\s*:\s*['"]([a-z][a-z0-9_-]{1,30})['"]/g;
  for (const f of files) {
    const src = readFileSync(f, 'utf8');
    for (const m of src.matchAll(re)) {
      const field = m[1] || m[3];
      const val = m[2] || m[4];
      if (!/(status|state|kind|type|stage|phase|channel|direction|priority|severity|level|mode|role|category|source)$/.test(field)) continue;
      if (!out.has(field)) out.set(field, new Set());
      if (out.get(field).size < 8) out.get(field).add(val);
    }
  }
  // Gültiger Grundzustand (Strang VIS4): Fehlerwerte (failed, error, blocked …) ans Ende, damit die erste, meist
  // ausgewählte Zeile einen gesunden Zustand zeigt. Fehlerfälle prüft das Labor eigens (zone-cases.mjs, Zone erwartet).
  const rank = (v) => (FAILURE_RE.test(v) ? 1 : 0);
  return new Map([...out].map(([k, v]) => [k, [...v].sort((a, b) => rank(a) - rank(b))]));
}
const FAILURE_RE = /^(fail(ed|ure)?|errors?|errored|blocked|rejected|cancel(l)?ed|aborted|disconnected|offline|expired|stopped|closed|infected|invalid|missing|timeout|timed_out|denied|crashed|dead|stale|broken)$/;

// Zeilen-IDs: lesbarer Einzahl-Stamm der Collection ohne App-Präfix, nie abgeschnitten (communication_accounts →
// account-001, journal_entries → journal-entry-001, outbound_companies → company-001).
const singular = (w) => w.replace(/ies$/, 'y').replace(/(ss|us)$/, '$1').replace(/([^s])s$/, '$1');
const slug = (c) => singular(c.replace(/^(accounting|customer|outbound|support|calendar|planning|ctox_ticket|ctox|appsec|iot|news|research|knowledge|document|spreadsheet|business_module|business|matching|browser|desktop|workjet|user_thread|user|communication)_/, '')).replace(/_/g, '-') || 'row';
export const rowId = (collection, i) => `${slug(collection)}-${String(i + 1).padStart(3, '0')}`;

function typeOf(ps) {
  const t = Array.isArray(ps?.type) ? ps.type.find((x) => x !== 'null') : ps?.type;
  return t || 'string';
}

// Firmen-Collections (Kundenkonten, Outbound-Firmen …) – nicht Kontenrahmen oder Postfächer.
const COMPANY_COLL_RE = /(^|_)(accounts|companies|customers|organizations|organisations|parties|vendors|suppliers|clients)$/;
const isCompanyCollection = (c) => COMPANY_COLL_RE.test(c) && !/^(accounting|communication|ctox|business|workjet|calendar)_/.test(c);
const CONTACT_COLL_RE = /(^|_)(contacts|people|persons)$/;
const COMPANY_REF_RE = /(^|_)(party|customer|client|company|organization|organisation|account|debtor|creditor|counterparty|vendor|supplier)$/;

// Fremdschlüssel: foo_id → Collection, deren Name auf foo / foos endet.
// Ohne Treffer werden führende Qualifizierer abgelegt (current_version_id → version → document_versions), damit
// Verweise wie „aktuelle Version“ auf eine vorhandene Zeile zeigen statt ins Leere (Strang VIS4, documents).
// Mehrere Kandidaten: gleiche App-Präfix wie die verweisende Collection zuerst (accounting_journal_entry_lines.account_id
// → accounting_accounts, nicht customer_accounts), dann die kürzeste.
// Parteien ohne eigene Collection (party_id, debtor_id …) zeigen auf die Firmen-Collection der App (Strang SEED).
function refCollection(field, collections, from = '') {
  let base = field.replace(/_ids?$/, '');
  const prefix = from.split('_')[0];
  const rank = (c) => (prefix && c.startsWith(`${prefix}_`) ? 0 : 1);
  for (;;) {
    const b = base;
    const cands = [...collections].filter((c) => c === b || c === `${b}s` || c === `${b.replace(/y$/, 'ies')}` || c.endsWith(`_${b}`) || c.endsWith(`_${b}s`) || c.endsWith(`_${b.replace(/y$/, 'ies')}`));
    if (cands.length) return cands.sort((x, y) => rank(x) - rank(y) || x.length - y.length)[0];
    const k = base.indexOf('_');
    if (k < 0) break;
    base = base.slice(k + 1);
  }
  if (COMPANY_REF_RE.test(field.replace(/_ids?$/, ''))) {
    const comp = [...collections].filter(isCompanyCollection).sort((x, y) => rank(x) - rank(y) || x.length - y.length)[0];
    if (comp) return comp;
  }
  return null;
}

const QTY = [1, 8, 40, 2, 120, 1];
const euro = (n) => Math.round(n * 100) / 100;
// Geldfelder: Betrag je Zeile aus dem gemeinsamen Netto-Betrag; *_cents ganzzahlig in Cent.
function moneyFor(f, ctx, i) {
  // Positionen (…_lines): Netto = Stückpreis × Menge, damit die Zeile in sich aufgeht.
  const net = /_lines?$/.test(ctx.collection) && ctx.props?.unit_price_cents ? euro(UNIT_EUR[i % UNIT_EUR.length] * QTY[i % QTY.length]) : ctx.ent.net;
  const gross = euro(net * (1 + VAT));
  const open = (i % 3 === 0) ? 0 : gross; // jede dritte Rechnung bezahlt
  let eur;
  if (/(annual_recurring|(^|_)arr(_|$)|yearly_revenue|annual_revenue)/.test(f)) eur = ctx.ent.company.arr;
  else if (/(monthly_recurring|(^|_)mrr(_|$))/.test(f)) eur = euro(ctx.ent.company.arr / 12);
  else if (/revenue/.test(f)) eur = ctx.ent.company.arr;
  else if (/(tax|vat)/.test(f)) eur = euro(net * VAT);
  else if (/(skonto|discount)/.test(f)) eur = euro(gross * 0.02);
  else if (/fee/.test(f)) eur = 5;
  else if (/interest/.test(f)) eur = euro(gross * 0.0075);
  else if (/(unit_price|price|hourly|rate_cents)/.test(f)) eur = UNIT_EUR[i % UNIT_EUR.length];
  else if (/(open|outstanding|due|balance)/.test(f)) eur = open;
  else if (/paid/.test(f)) eur = euro(gross - open);
  else if (/(net|subtotal)/.test(f)) eur = net;
  else if (/budget/.test(f)) eur = 25000 + (i % 6) * 7500;
  else if (/(gross|total|amount|sum|value|cost|allocated)/.test(f)) eur = gross;
  else eur = net;
  return /_cents$/.test(f) ? Math.round(eur * 100) : eur;
}

function valueFor(field, ps, i, ctx) {
  const t = typeOf(ps);
  const f = field.toLowerCase();
  const { company, contact } = ctx.ent;
  const staff = STAFF[(i + ctx.salt) % STAFF.length];
  const person = ctx.isContact || !/(user|member|employee|crew|agent|staff)/.test(ctx.collection) ? contact : { first: staff[0], last: staff[1], role: staff[2] };
  const pick = (arr, off = 0) => arr[(i + ctx.salt + off) % arr.length];
  const isMsg = /(message|thread|mail|conversation|inbox)/.test(ctx.collection);
  const ms = BASE_MS - (i * 26 + ctx.salt * 3) * 3600_000;
  if (Array.isArray(ps?.enum) && ps.enum.length) return ps.enum[i % ps.enum.length];
  if (f === 'id') return ctx.id;
  if (/^(is_|has_)?(deleted|trashed|archived|hidden|locked|disabled|muted|blocked|spam)$/.test(f.replace(/^is_|^has_/, '')) || /^(is_deleted|is_trashed|is_archived|archived|deleted|is_hidden|is_locked)$/.test(f)) return t === 'boolean' ? false : (t === 'number' ? 0 : '');
  if (/(deleted_at|archived_at|trashed_at)/.test(f)) return t === 'number' ? 0 : '';
  if (/_ids$/.test(f) && t === 'array') { const rc = refCollection(f, ctx.collections, ctx.collection); return rc ? [rowId(rc, i % ctx.rowsOf(rc))] : []; }
  if (/_id$/.test(f) && t === 'string') {
    const rc = refCollection(f, ctx.collections, ctx.collection);
    if (rc) return rowId(rc, (ctx.refIndex?.[f] ?? i) % ctx.rowsOf(rc));
    // Datei-Verweise ohne Zeile wären hängende Referenzen (files bekommt keine Fixture-Zeilen): leer = gültig.
    if (/(^|_)(blob|file|attachment|asset)_id$/.test(f)) return '';
    if (/user|owner|author|actor|assignee/.test(f)) return 'owner';
    return `${f.replace(/_id$/, '').replace(/_/g, '-')}-${String((i % 3) + 1).padStart(3, '0')}`;
  }
  if (t === 'boolean') return /^(is_)?(active|enabled|visible|published)$/.test(f) ? true : (/favorite|pinned|starred|read|unread|flagged/.test(f) ? i % 3 === 0 : i % 2 === 0);
  if (t === 'number' || t === 'integer') {
    if (/(_ms|_at|time|timestamp|date)$/.test(f) || /^(ts|at)$/.test(f)) return /due|until|end|expires|start/.test(f) ? ms + 14 * 86400_000 : ms;
    if (/(employee|headcount|staff)_?(count|size|number)?$/.test(f) || /^employees$/.test(f)) return company.employees;
    if (/(tax|vat)_rate$/.test(f)) return VAT; // Anteil [0, 1] wie in invoices validiert
    if (/(skonto|discount)_percent$/.test(f)) return [2, 0, 3][i % 3];
    if (/_days$/.test(f)) return [14, 30, 10, 60][i % 4];
    if (/^(quantity|qty)$/.test(f)) return QTY[i % QTY.length];
    if (/^(debit|credit)(_cents)?$/.test(f)) return moneyFor(f.startsWith('debit') ? 'net' : 'net', ctx, i) * (/_cents$/.test(f) ? 100 : 1) * ((i % 2 === 0) === f.startsWith('debit') ? 1 : 0);
    if (/(_cents$|amount|total|price|net|gross|sum|value|cost|budget|revenue|balance|tax|vat|paid|open|fee|interest|skonto|allocated)/.test(f) && !/(count|percent|rate$|ratio)/.test(f)) return moneyFor(f, ctx, i);
    if (/(percent|ratio|rate|score|progress|confidence|probability)/.test(f)) return [0.82, 0.45, 0.67, 0.91, 0.3, 0.58][i % 6] * (/percent|progress/.test(f) ? 100 : 1);
    if (/(idx|index|order|position|sort|rank|seq|version|revision|page)/.test(f)) return i + 1;
    if (/(count|qty|quantity|number|size|bytes|length|duration|minutes|hours|days)/.test(f)) return [3, 12, 1, 7, 24, 5][i % 6];
    if (/(lat)$/.test(f)) return 48.37 + i * 0.01;
    if (/(lng|lon)$/.test(f)) return 10.89 + i * 0.01;
    return [1, 2, 3, 5, 8, 13][i % 6];
  }
  if (t === 'array') {
    const it = typeOf(ps?.items || {});
    if (/tag|label|keyword|categor/.test(f) && it === 'string') return [pick(TAGS), pick(TAGS, 2)];
    if (/participant/.test(f) && it === 'string') return [personEmail(person, company.domain), personEmail({ first: staff[0], last: staff[1] }, OWN_DOMAIN)];
    if (/(recipient|to|cc)/.test(f) && it === 'string') return [personEmail({ first: staff[0], last: staff[1] }, OWN_DOMAIN)];
    return [];
  }
  if (t === 'object') return /(address|anschrift)/.test(f) ? { street: company.street, zip: company.zip, city: company.city, country: 'DE' } : {};
  // string
  const lit = ctx.literals.get(field) || ctx.literals.get(f.replace(/^.*_/, ''));
  if (lit?.length && /(status|state|kind|type|stage|phase|channel|direction|priority|severity|level|mode|role|category|source)$/.test(f)) return lit[i % lit.length];
  if (/participant.*_json$/.test(f)) return JSON.stringify([personEmail(person, company.domain), personEmail({ first: staff[0], last: staff[1] }, OWN_DOMAIN)]);
  if (/(recipient|to|cc|bcc).*_json$/.test(f)) return JSON.stringify([personEmail({ first: staff[0], last: staff[1] }, OWN_DOMAIN)]);
  if (/_json$/.test(f)) return '{}';
  if (/(_at|_date|date|time|timestamp)$/.test(f) || f === 'at') { const d = new Date(/due|until|end|expires/.test(f) ? ms + 14 * 86400_000 : ms); return /date$/.test(f) && !/time/.test(f) ? d.toISOString().slice(0, 10) : d.toISOString(); }
  // Absender/Empfänger: externer Ansprechpartner schreibt an eigene Mitarbeitende.
  if (/(recipient|^to$|_to$)/.test(f) && /(email|mail|address|^to$|_to$)/.test(f)) return personEmail({ first: staff[0], last: staff[1] }, OWN_DOMAIN);
  if (/(email|mail$|e_mail)/.test(f) || (/(sender|from)/.test(f) && /address/.test(f))) return personEmail(person, ctx.isStaff ? OWN_DOMAIN : company.domain);
  if (/(sender|from)_?(display|name)?$/.test(f) || /(sender|from)_(display|name)/.test(f)) return `${person.first} ${person.last}`;
  if (/phone|mobile|tel|fax/.test(f)) return `+49 ${[821, 40, 221, 7461, 911, 341, 421, 711, 511, 841, 231, 89][ctx.ent.c % 12]} ${4500 + i * 37}-${10 + i}`;
  if (/^(first_?name|given_name|vorname)$/.test(f)) return person.first;
  if (/^(last_?name|family_name|surname|nachname)$/.test(f)) return person.last;
  if (/(job_?title|position|job_role|^jobtitle$|function_title)/.test(f)) return person.role;
  if (/(industry|branche|sector)/.test(f)) return company.industry;
  if (/(company|organization|organisation|firm|account_name|customer_name|vendor|supplier|employer|client|party_name|debtor)/.test(f) && !/_(id|key)$/.test(f)) return company.name;
  if (/(display_name|full_name|contact_name|person|assignee_name|owner_name|author_name|user_name|^name$)/.test(f)) {
    if (ctx.isCompany) return company.name;
    if (/(opportunit|deal|pipeline|projekt|project)/.test(ctx.collection)) return `${company.short} – ${DEALS[(i + ctx.salt) % DEALS.length]}`;
    if (/contact|employee|user|member|person|crew|agent|people|candidate|applicant/.test(ctx.collection)) return `${person.first} ${person.last}`;
    return pick(TITLES);
  }
  if (isMsg && /(subject|^title$)/.test(f)) return MAIL_SUBJECTS[(i + ctx.salt) % MAIL_SUBJECTS.length];
  if (/(title|subject|headline|label|caption|heading|^topic$)/.test(f)) return pick(TITLES);
  if (/(filename|file_name|^path$|file_path)/.test(f)) return pick(FILES);
  if (/(mime|content_type)/.test(f)) return ['application/pdf', 'text/markdown', 'image/png'][i % 3];
  if (/linkedin/.test(f)) return `https://www.linkedin.com/company/${company.domain.replace(/\.de$/, '')}`;
  if (/(^|_)x_url$|twitter/.test(f)) return '';
  if (/(meeting|video|call)_?url/.test(f)) return `https://meet.${OWN_DOMAIN}/${company.domain.replace(/\.de$/, '')}`;
  if (/(file|storage|download|pdf|document)_?url/.test(f)) return '';
  if (/((^|_)url|website|homepage|href)$/.test(f)) return `https://www.${company.domain}`;
  if (/(domain|host)$/.test(f)) return company.domain;
  if (/(city|town|ort)$/.test(f)) return company.city;
  if (/^(address|email_address|account_address)$/.test(f) && /communication|mail|inbox|sender/.test(ctx.collection)) return personEmail({ first: staff[0], last: staff[1] }, OWN_DOMAIN);
  if (/(street|strasse)/.test(f) || /^address(_line\d?)?$/.test(f)) return company.street;
  if (/^unit$/.test(f)) return ['Std', 'Stk', 'h', 'Pauschal'][i % 4];
  if (/(zip|postal|plz)/.test(f)) return company.zip;
  if (/country/.test(f)) return 'DE';
  if (/currency/.test(f)) return 'EUR';
  if (/(locale|lang|language)/.test(f)) return 'de-DE';
  if (/(color|colour)/.test(f)) return ['teal', 'amber', 'violet', 'slate', 'rose', 'green'][i % 6];
  if (/(icon|emoji|avatar|image)/.test(f)) return '';
  if (/(tags|labels|keywords)$/.test(f)) return `${pick(TAGS)}, ${pick(TAGS, 3)}`;
  if (/(status|state|stage|phase)$/.test(f)) return GENERIC_STATUS[i % GENERIC_STATUS.length];
  if (/(priority|severity)$/.test(f)) return ['high', 'medium', 'low'][i % 3];
  if (/(kind|type|category|channel|direction|mode|role|source|provider|module)$/.test(f)) return ['standard', 'email', 'internal'][i % 3];
  if (/(hash|sha|checksum|etag|token|secret|password|passcode)/.test(f)) return '';
  // Fehlerfelder leer: gültiger Grundzustand ohne Laufzeitfehler (Fehlerfälle: zone-cases.mjs).
  if (/(^|_)(error|errors|error_message|error_text|error_code|failure_reason|failure_message)$/.test(f)) return '';
  if (/(rrule|recurrence|exdate|rdate|cron)/.test(f)) return ''; // Regelausdrücke: leer = keine Wiederholung
  if (/(folder|notebook|group|team|department)$/.test(f)) return ['Vertrieb', 'Einkauf', 'Projekte', 'Intern'][i % 4];
  if (/^search_text$/.test(f)) return ctx.isCompany ? `${company.name} ${company.domain} ${company.city}` : (ctx.isContact ? `${person.first} ${person.last} ${company.name}` : `${company.name} · ${MAIL_SUBJECTS[(i + ctx.salt) % MAIL_SUBJECTS.length]}`);
  if (/(invoice_no|invoice_number|document_number|^number$)/.test(f) && /invoice|credit|receipt|dunning/.test(ctx.collection)) return `RE-2026-${String(1040 + i).padStart(4, '0')}`;
  if (/(number|_no$|reference|ref$|code$|key$|invoice_no|sku)/.test(f)) return `${new Date(ms).getUTCFullYear()}-${String(1040 + i).padStart(5, '0')}`;
  if (isMsg && /(body|text|content|preview|snippet|excerpt)/.test(f) && !/html/.test(f)) {
    const b = MAIL_BODIES[(i + ctx.salt) % MAIL_BODIES.length].replace(/Viele Grüße|Beste Grüße|Mit freundlichen Grüßen|Gruß$/, (g) => `${g}\n${person.first} ${person.last}`);
    return /(preview|snippet|excerpt)/.test(f) ? b.split('\n').filter(Boolean)[1] : b;
  }
  if (/(content|body|text|description|summary|note|notes|comment|message|details|markdown|html|excerpt|snippet|answer|question|instruction|prompt|reason)/.test(f)) {
    const s = pick(SENTENCES);
    return /html|content/.test(f) && /note|document|knowledge|page/.test(ctx.collection) ? `<h1>${pick(TITLES)}</h1><p>${s}</p>` : s;
  }
  if (/(user|owner|author|actor|assignee|created_by|updated_by)/.test(f)) return 'owner';
  if (/(folder|notebook|group|team|department|project)/.test(f)) return ['Vertrieb', 'Einkauf', 'Projekte', 'Intern'][i % 4];
  if (/(version|rev)$/.test(f)) return String(i + 1);
  return pick(TITLES).split(' ')[0];
}

// Zeilen-Entität: zu welcher Firma (und welchem Ansprechpartner) gehört Zeile i? Firmen-Collections: Zeile i = Firma i.
// Sonst folgt die Zeile ihrem ersten Firmenverweis (account_id, company_id, party_id …), damit Name, Domain und Mail
// der Zeile zur verwiesenen Firmenzeile passen. Kontakte: je Firma zwei Personen (Zeile i → zweite Person ab i ≥ n).
function entityFor(collection, props, i, ctx) {
  const isCompany = isCompanyCollection(collection);
  const isContact = CONTACT_COLL_RE.test(collection);
  let c = i;
  if (!isCompany) {
    for (const field of Object.keys(props)) {
      if (!/_id$/.test(field)) continue;
      const rc = refCollection(field, ctx.collections, collection);
      if (rc && isCompanyCollection(rc)) { c = i % ctx.rowsOf(rc); break; }
    }
  }
  const contact = contactForCompany(c, Math.floor(i / Math.max(1, ctx.rowsOf(collection))) + (isContact ? 0 : i));
  return { c, company: companyOf(c), contact, net: NET_EUR[(i + ctx.salt) % NET_EUR.length], isCompany, isContact };
}

function rowsFor(collection, schema, n, ctx) {
  const props = schema?.properties || {};
  const required = new Set(schema?.required || []);
  const rows = [];
  for (let i = 0; i < n; i++) {
    const id = rowId(collection, i);
    const row = {};
    const ent = entityFor(collection, props, i, ctx);
    const c = { ...ctx, id, collection, ent, props, isCompany: ent.isCompany, isContact: ent.isContact, isStaff: /(user|member|employee|crew|staff)/.test(collection) };
    for (const [field, ps] of Object.entries(props)) {
      // Pflichtfelder immer; optionale, wenn sie lesbar/üblich sind (keine Blob-/Hash-/Diagnosefelder).
      if (!required.has(field) && /(blob|chunk|hash|diagnostic|state_dir|raw|embedding|vector|secret|token)/i.test(field)) continue;
      const v = valueFor(field, ps, i, c);
      if (v === undefined) continue;
      if (typeOf(ps) === 'string' && ps.maxLength && typeof v === 'string' && v.length > ps.maxLength) row[field] = v.slice(0, ps.maxLength);
      else row[field] = v;
    }
    row.id = id;
    rows.push(row);
  }
  return rows;
}

// Gültiger Grundzustand je App, wo der Kern-Stub eine Fähigkeit des echten Kerns nicht hat (Strang VIS4). Jeder
// Eintrag nennt den Grund; der dabei ausgesparte Fehlerweg ist als Zonen-Fall in zone-cases.mjs abgedeckt.
export const BASE_PATCHES = {
  // DOCX braucht die native Office-Vorbereitung des Kerns (Rust prepare → DOCY), die der Stub nicht hat; ohne sie
  // meldet documents korrekt „DOCX editor konnte nicht geladen werden“ in der Zone (Zonen-Fall docx-ohne-kern).
  // Der Grundzustand misst das Layout daher mit Markdown-Dokumenten (im Browser editierbar, gleiche Spalten).
  documents: (seed) => ({
    ...seed,
    documents: (seed.documents || []).map((r) => ({ ...r, document_type: 'markdown_document', mime_type: 'text/markdown', filename: String(r.filename || 'Dokument').replace(/\.[a-z]+$/i, '') + '.md' })),
  }),
};

export function buildFixtures(appId) {
  const own = readJson(sch(appId))?.collections || {};
  const manifest = readJson(join(MANIFESTS, `${appId}.manifest.json`));
  const foreign = Object.keys(manifest?.foreign_collections || {});
  const index = schemaIndex();
  const schemas = {};
  const counts = {};
  for (const [c, s] of Object.entries(own)) { schemas[c] = s; counts[c] = isCompanyCollection(c) || CONTACT_COLL_RE.test(c) ? ROWS_COMPANY : ROWS_DEFAULT; }
  for (const c of foreign) if (!schemas[c] && index.has(c)) { schemas[c] = index.get(c).schema; counts[c] = ROWS_FOREIGN; }
  const collections = new Set(Object.keys(schemas).filter((c) => !SKIP.has(c) && !SKIP_RE.test(c)));
  const literals = codeLiterals(appId);
  const seed = {};
  let salt = 0;
  for (const c of collections) {
    salt = (salt + 3) % 8;
    seed[c] = rowsFor(c, schemas[c], counts[c], { collections, literals, salt, rowsOf: (rc) => counts[rc] || ROWS_DEFAULT });
  }
  const patched = BASE_PATCHES[appId] ? BASE_PATCHES[appId](seed) : seed;
  return { schemas, seed: patched, skipped: Object.keys(schemas).filter((c) => !collections.has(c)), foreign };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const id = process.argv[2];
  if (!id) { console.error('Aufruf: node fixtures.mjs <appId>'); process.exit(64); }
  const r = buildFixtures(id);
  console.log(JSON.stringify({ skipped: r.skipped, seed: r.seed }, null, 1));
}
