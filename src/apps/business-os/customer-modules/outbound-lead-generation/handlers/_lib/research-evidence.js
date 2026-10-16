// Belegregeln der Personen-/Firmenrecherche — Teilschritt 1 des Ports von
// outbound.lead.research_writeback (Handler selbst bleibt bis zum Gesamtport legacy).
//
// ref: person_research_command.rs (origin/main 4794af03d)
//      :1584 research_scalar_is_populated, :1592 deduplicate_research_evidence,
//      :1640 field_accepts_single_authoritative_source, :1670 required_independent_sources,
//      :1675 independent_research_evidence_count, :1693 matching_research_evidence_count,
//      :1744 crm_aware_provider_count, :1756 COMPOUND_TLDS, :1765 research_evidence_provider,
//      :1784 research_evidence_source_key
//      person_research_gap_closure.rs :2944 QUANTITY_FIELDS, :2948 numbers_in,
//      :2981 quote_backs_value, :2991 person_name_quote_backs, :3019 email_quote_backs,
//      :3030 normalized_email_text, :3040 quantity_quote_backs, :3170 CRM_SOURCE_SCHEME
//
// OWNER-REGEL (Kundensystem THESEN): übernommen exakt wie im Rust-Stand 4794af03d —
// `required_independent_sources` liefert für JEDES Feld 1 (Owner-Regel 23.09.2026 laut
// Rust-Kommentar). Nicht erweitert, nicht ausgelegt. Siehe V4-PORT.md §15 zur Abweichung
// gegenüber der Formulierung „Ziel 2 Quellen je Feld, 1 nur im Notfall“.
import { has } from './olg.js';

export const CRM_SOURCE_SCHEME = 'sellify';
const QUANTITY_FIELDS = ['mitarbeiter', 'umsatz'];
const COMPOUND_TLDS = ['co.uk', 'org.uk', 'ac.uk', 'gov.uk', 'co.at', 'or.at', 'ac.at', 'com.au', 'net.au', 'org.au',
  'co.jp', 'com.br', 'com.tr', 'co.nz', 'com.pl'];
const SINGLE_AUTHORITATIVE = ['firma_domain', 'firma_email', 'firma_telefon', 'firma_fax', 'firma_postfach',
  'firma_besucheranschrift', 'firma_postanschrift', 'firma_homepage_fact_sheet', 'person_geschlecht', 'person_titel',
  'person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_email', 'person_email_validation',
  'person_telefon', 'person_linkedin', 'person_xing'];

const get = (value, key) => (has(value, key) ? value[key] : undefined);
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());
/** serde_json `Value::to_string()` (compact JSON). */
const jsonText = (value) => JSON.stringify(value);

/** ref: :1584 */
export function researchScalarIsPopulated(value) {
  if (typeof value === 'string') return value.trim() !== '';
  return typeof value === 'number' || typeof value === 'boolean';
}

/** ref: :1640 */
export function fieldAcceptsSingleAuthoritativeSource(fieldKey) {
  return SINGLE_AUTHORITATIVE.includes(String(fieldKey).trim());
}

/**
 * ref: :1670 — Owner rule 23.09.2026: one matching, cited source is enough for
 * every field. RUST-EIGENHEIT: the single-source classification is evaluated
 * and discarded (`let _ = …`); kept so the call graph stays identical.
 */
export function requiredIndependentSources(fieldKey) {
  void fieldAcceptsSingleAuthoritativeSource(fieldKey);
  return 1;
}

/** ref: :1765 */
export function researchEvidenceProvider(host) {
  let h = String(host).trim();
  while (h.startsWith('www.')) h = h.slice(4);
  h = asciiLower(h);
  const labels = h.split('.').filter((l) => l !== '');
  if (labels.length < 2) return labels.join('.');
  const lastTwo = labels.slice(-2).join('.');
  if (COMPOUND_TLDS.includes(lastTwo)) {
    // labels.get(len - 3) with wrapping_sub: two labels → None → last_two.
    return labels.length >= 3 ? labels[labels.length - 3] : lastTwo;
  }
  return labels[labels.length - 2];
}

function urlHost(text) {
  try {
    const parsed = new URL(text);
    return parsed.hostname === '' ? null : parsed.hostname;
  } catch {
    return undefined; // parse failure (distinct from "parsed, no host")
  }
}

/** ref: :1784 */
export function researchEvidenceSourceKey(entry) {
  const rawUrl = has(entry, 'source_url') ? entry.source_url : get(entry, 'url');
  const sourceUrl = typeof rawUrl === 'string' && rawUrl.trim() !== '' ? rawUrl.trim() : null;
  // `sellify://company/…` and `sellify://person/…` are one provider: the CRM.
  if (sourceUrl != null && asciiLower(sourceUrl).startsWith(`${CRM_SOURCE_SCHEME}://`)) return CRM_SOURCE_SCHEME;
  if (sourceUrl != null) {
    let host = urlHost(sourceUrl);
    // Url::parse(..).ok().or_else(https://…): the fallback only runs on a parse error.
    if (host === undefined) host = urlHost(`https://${sourceUrl}`) ?? null;
    if (host) {
      const provider = researchEvidenceProvider(host);
      if (provider !== '') return provider;
    }
  }
  const sourceId = get(entry, 'source_id');
  if (typeof sourceId === 'string' && sourceId.trim() !== '') {
    const value = sourceId.trim();
    const provider = researchEvidenceProvider(value);
    return provider === '' ? asciiLower(value) : provider;
  }
  return null;
}

/** ref: :1744 — Sellify counts as one source but proves nothing alone. */
export function crmAwareProviderCount(count, providers) {
  return [...providers].every((p) => p === CRM_SOURCE_SCHEME) ? 0 : count;
}

function entryFieldKey(entry) {
  const v = has(entry, 'field_key') ? entry.field_key : get(entry, 'field');
  return typeof v === 'string' ? v : null;
}

/** ref: :1675 */
export function independentResearchEvidenceCount(evidence, fieldKey) {
  const providers = new Set();
  for (const entry of evidence) {
    if (entryFieldKey(entry) !== fieldKey) continue;
    const key = researchEvidenceSourceKey(entry);
    if (key != null) providers.add(key);
  }
  return crmAwareProviderCount(providers.size, providers);
}

/** ref: :2948 numbers_in → [[number, start, end]] (Rust regex `\d` is Unicode). */
export function numbersIn(text) {
  const out = [];
  const pattern = /\p{Nd}{1,3}(?:[.,]\p{Nd}{3})+(?:[.,]\p{Nd}+)?|\p{Nd}+(?:[.,]\p{Nd}+)?/gu;
  const grouped = /^\p{Nd}{1,3}(?:([.,])\p{Nd}{3})+(?:[.,]\p{Nd}+)?$/u;
  for (const found of String(text).matchAll(pattern)) {
    const raw = found[0];
    const sep = grouped.exec(raw)?.[1] ?? null;
    // regex captures(): group 1 holds its LAST repetition (RUST-EIGENHEIT kept by JS too).
    const normalized = sep === '.' ? raw.replace(/\./g, '').replace(/,/g, '.')
      : sep === ',' ? raw.replace(/,/g, '') : raw.replace(/,/g, '.');
    // Rust f64::parse: only ASCII digits parse.
    if (!/^[0-9]+(?:\.[0-9]+)?$/.test(normalized)) continue;
    const number = Number(normalized);
    if (Number.isFinite(number)) out.push([number, found.index, found.index + raw.length]);
  }
  return out;
}

/** ref: :3040 */
function quantityQuoteBacks(field, value, quote) {
  if (!QUANTITY_FIELDS.includes(field)) return true;
  const first = numbersIn(value)[0];
  if (!first) return true;
  const wanted = first[0];
  const inQuote = numbersIn(quote);
  if (inQuote.length === 0) return false;
  const range = /\p{Nd}[\p{Nd}.,]*\s*(?:-|–|—|bis|to)\s*\p{Nd}[\p{Nd}.,]*/giu;
  const spans = [...String(quote).matchAll(range)].map((m) => [m.index, m.index + m[0].length]);
  const same = (a, b) => Math.abs(a - b) <= 0.005 * Math.max(Math.abs(a), Math.abs(b), 1e-9);
  return inQuote
    .filter(([, start, end]) => !spans.some(([os, oe]) => os <= start && end <= oe))
    .some(([n]) => same(n, wanted) || same(n, wanted * 1_000) || same(n, wanted * 1_000_000)
      || same(n * 1_000, wanted) || same(n * 1_000_000, wanted));
}

/** ref: :3030 */
export function normalizedEmailText(text) {
  const lower = String(text).toLowerCase();
  return lower
    .replace(/\s*(?:\(at\)|\[at\]|\{at\}|\(@\)|\[@\]|\sat\s)\s*/gu, '@')
    .replace(/\s*(?:\(dot\)|\[dot\]|\{dot\}|\(punkt\)|\[punkt\]|\sdot\s)\s*/gu, '.');
}

/** ref: :3019 */
export function emailQuoteBacks(field, value, quote) {
  if (field !== 'person_email') return true;
  const wanted = asciiLower(String(value).trim());
  if (!wanted.includes('@')) return true;
  return normalizedEmailText(quote).includes(wanted);
}

/** ref: :2991 */
function personNameQuoteBacks(field, value, quote) {
  if (!['person_vorname', 'person_nachname', 'person_titel'].includes(field)) return true;
  const lowered = String(value).trim().toLowerCase();
  if (field === 'person_titel' && (lowered === 'herr' || lowered === 'frau')) return true;
  const words = (text) => String(text).toLowerCase().split(/[^\p{Alphabetic}\p{N}]/u).filter((w) => w !== '');
  const wanted = words(value);
  if (wanted.length === 0) return false;
  const observed = words(quote);
  for (let i = 0; i + wanted.length <= observed.length; i += 1) {
    if (wanted.every((w, j) => observed[i + j] === w)) return true;
  }
  return false;
}

/** ref: :2981 */
export function quoteBacksValue(field, value, quote) {
  return quantityQuoteBacks(field, value, quote) && emailQuoteBacks(field, value, quote) && personNameQuoteBacks(field, value, quote);
}

/** ref: :1693 matching_research_evidence_count */
export function matchingResearchEvidenceCount(evidence, fieldKey, field, allowUnboundPerson) {
  if (!has(field, 'value')) return 0;
  const scalarText = (v) => (typeof v === 'string' ? v.trim().toLowerCase() : jsonText(v));
  const wanted = scalarText(field.value);
  const pk = get(field, 'person_key');
  const personKey = typeof pk === 'string' && pk.trim() !== '' ? pk.trim() : null;
  const isPerson = fieldKey.startsWith('person_');
  if (isPerson && personKey == null && !allowUnboundPerson) return 0;
  const matching = evidence.filter((entry) => {
    if (!has(entry, 'value') || scalarText(entry.value) !== wanted) return false;
    if (isPerson) {
      const ek = get(entry, 'person_key');
      const evidenceKey = typeof ek === 'string' && ek.trim() !== '' ? ek.trim() : null;
      if (evidenceKey !== personKey) return false;
    }
    const quote = typeof get(entry, 'quote') === 'string' ? entry.quote : '';
    return quoteBacksValue(fieldKey, wanted, quote);
  });
  return independentResearchEvidenceCount(matching, fieldKey);
}

/** ref: :1592 deduplicate_research_evidence */
export function deduplicateResearchEvidence(entries) {
  const seen = new Set();
  const str = (v) => (typeof v === 'string' ? v : '');
  return entries.filter((entry) => {
    const key = asciiLower([
      str(has(entry, 'field_key') ? entry.field_key : get(entry, 'field')),
      str(get(entry, 'source_id')),
      str(has(entry, 'source_url') ? entry.source_url : get(entry, 'url')),
      has(entry, 'value') ? jsonText(entry.value) : '',
      str(get(entry, 'person_key')),
      has(entry, 'quote') ? jsonText(entry.quote) : has(entry, 'note') ? jsonText(entry.note) : '',
    ].join('|'));
    if (key.replace(/\|/g, '') === '' || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

