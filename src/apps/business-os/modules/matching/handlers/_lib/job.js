// Stellenanzeigen-Auswertung, treu portiert aus importer.rs (4794af03d):
// parse_job_html :2164-2316, split_job_sections :2863, collect_sections :2931,
// before_any/after_any/between_any :2962-2993, bulletize :2995,
// infer_fachlevel :3008, find_jobposting :3033, first_json :3050,
// address_text :3057, infer_company_from_url :4076, origin_url :4086.
// DOM-Zugriffe (html5ever/scraper: Selektoren, Textknoten, Attribute) sind
// Fähigkeit ctx.cap.doc.html – der Handler bekommt nur Rohtexte zurück.
import { byteLen, cleanText, lines, normalizeLines, regexCapture } from './text.js';

// Rust `str::find` liefert Byte-Offsets; Slicing hier auf JS-Strings ist
// gleichwertig, weil Marker vollständige Zeichenfolgen sind.
function beforeAny(text, markers) {
  const idx = markers.map((m) => text.indexOf(m)).filter((i) => i >= 0);
  return text.slice(0, idx.length ? Math.min(...idx) : text.length);
}
function afterAny(text, markers) {
  for (const m of markers) {
    const i = text.indexOf(m);
    if (i >= 0) return text.slice(i + m.length);
  }
  return '';
}
function betweenAny(text, starts, ends) {
  for (const s of starts) {
    const i = text.indexOf(s);
    if (i >= 0) {
      const rest = text.slice(i + s.length);
      const e = ends.map((m) => rest.indexOf(m)).filter((x) => x >= 0);
      return rest.slice(0, e.length ? Math.min(...e) : rest.length);
    }
  }
  return '';
}
const eqIgnoreAsciiCase = (a, b) => a.length === b.length && a.replace(/[A-Z]/g, (c) => c.toLowerCase()) === b.replace(/[A-Z]/g, (c) => c.toLowerCase());

function collectSections(text, starts, terminal) {
  const out = [];
  let active = false;
  for (const raw of lines(text)) {
    const line = cleanText(raw);
    if (line === '') continue;
    if (terminal.some((m) => eqIgnoreAsciiCase(line, m.replace(/:+$/, ''))) || terminal.some((m) => line.startsWith(m))) {
      if (active) break;
    }
    if (starts.some((m) => line.startsWith(m))) {
      active = true;
      out.push(line);
      continue;
    }
    if (active) out.push(line);
  }
  return out.join('\n');
}

const ROLE_STARTS = ['Was dich bei uns erwartet:', 'Stellenbeschreibung:', 'Das werden Ihre WOW-Momente:'];
const REQ_STARTS = ['Ausbildung:', 'Erfahrungen & Know-How:', 'Arbeitsweise & Persönlichkeit:', 'Qualifikationen:', 'Es wäre WOW, wenn Sie das hier mitbringen:'];
const REQ_ENDS = ['Zusätzliche Informationen:', 'Unser Kärcher WOW-Paket:'];

export function splitJobSections(rawText) {
  const aboutCompany = beforeAny(rawText, ROLE_STARTS);
  const aboutRole = betweenAny(rawText, ROLE_STARTS, REQ_STARTS);
  const explicit = collectSections(rawText, REQ_STARTS, REQ_ENDS);
  const requirements = explicit.trim() === '' ? betweenAny(rawText, REQ_STARTS, REQ_ENDS) : explicit;
  const benefits = betweenAny(rawText, ['Unser Kärcher WOW-Paket:', 'Wir bieten Ihnen:'], ['Also: Wanna WOW with us?', 'Vielfalt und Inklusion']);
  const closing = afterAny(rawText, ['Zusätzliche Informationen:', 'Also: Wanna WOW with us?']);
  return [aboutCompany, aboutRole, requirements, benefits, closing].map(normalizeLines);
}

function trimStartChars(value, chars) {
  let i = 0;
  const arr = [...value];
  while (i < arr.length && chars.includes(arr[i])) i++;
  return arr.slice(i).join('');
}

export function bulletize(text) {
  return lines(text)
    .map((line) => trimStartChars(line.trim(), ['-', '•', '*']).trim())
    .filter((line) => byteLen(line) > 8)
    .slice(0, 24);
}

export function inferFachlevel(role, requirements) {
  const text = `${role}\n${requirements}`.toLowerCase();
  if (text.includes('studium') || text.includes('ingenieur') || text.includes('projekt')) return 3;
  if (text.includes('techniker') || text.includes('meister') || text.includes('mehrjährige')) return 2;
  return 1;
}

export function findJobposting(value) {
  if (Array.isArray(value)) {
    for (const item of value) {
      const found = findJobposting(item);
      if (found) return found;
    }
    return null;
  }
  if (value && typeof value === 'object') {
    if (typeof value['@type'] === 'string' && value['@type'].toLowerCase() === 'jobposting') return value;
    for (const v of Object.values(value)) {
      const found = findJobposting(v);
      if (found) return found;
    }
  }
  return null;
}

const firstJson = (value) => (Array.isArray(value) ? value[0] : value);

export function addressText(address) {
  const s = (k, d = '') => (typeof address?.[k] === 'string' ? address[k] : d);
  return [s('addressLocality'), s('addressRegion'), s('postalCode'), s('addressCountry', 'Deutschland')]
    .filter((v) => v.trim() !== '')
    .join(', ');
}

// importer.rs::origin_url. Der vm-Kontext hat keine WHATWG-`URL`; Ersatz für
// `url::Url::parse` (Schema + Host), WHATWG-URL falls vorhanden.
export function originUrl(url) {
  let parts = null;
  if (typeof globalThis.URL === 'function') {
    try {
      const u = new globalThis.URL(url);
      parts = { scheme: u.protocol.replace(/:$/, ''), host: u.hostname };
    } catch {
      parts = null;
    }
  } else {
    const m = /^([A-Za-z][A-Za-z0-9+.-]*):(?:\/\/)?(?:[^@/?#]*@)?(\[[^\]]*\]|[^:/?#\s]*)(?::\d*)?(?:[/?#]|$)/.exec(url);
    if (m && !/\s/.test(url)) parts = { scheme: m[1].toLowerCase(), host: m[2].toLowerCase() };
  }
  return parts ? `${parts.scheme}://${parts.host}` : null;
}

function inferCompanyFromUrl(url) {
  if (url.includes('bosch')) return 'Bosch';
  if (url.includes('kaercher') || url.includes('karcher')) return 'Alfred Kärcher SE & Co. KG';
  return originUrl(url) ?? 'Unbekanntes Unternehmen';
}

// Die DOM-Abfragen, die parse_job_html braucht, in einem Fähigkeitsaufruf.
export const JOB_HTML_QUERIES = [
  { key: 'ld_json', selector: 'script[type="application/ld+json"]', mode: 'all_text' },
  { key: 'og_title', selector: 'meta[property="og:title"]', mode: 'attr', attr: 'content' },
  { key: 'title', selector: 'title', mode: 'text' },
  { key: 'jobdescription', selector: '.jobdescription', mode: 'text' },
  { key: 'body', selector: 'body', mode: 'text' },
  { key: 'geo', selector: '.jobGeoLocation', mode: 'text' },
  { key: 'city', selector: '[data-careersite-propertyid="city"]', mode: 'text' },
  { key: 'org', selector: '[itemprop="hiringOrganization"]', mode: 'micro' },
  { key: 'date', selector: '[itemprop="datePosted"]', mode: 'micro' },
];

// select_text: erster Treffer, Textknoten mit "\n" verbunden → normalize_lines, leer → null.
const selectText = (raw) => {
  if (typeof raw !== 'string') return null;
  const t = normalizeLines(raw);
  return t.trim() ? t : null;
};
// meta_content: Attribut → clean_text (auch leer).
const metaContent = (raw) => (typeof raw === 'string' ? cleanText(raw) : null);
// micro_meta liefert die Fähigkeit bereits (content-Attribut, sonst clean_text der Texte).
const micro = (raw) => (typeof raw === 'string' ? raw : null);
const str = (v) => (typeof v === 'string' ? v : null);
const takeChars = (v, n) => [...v].slice(0, n).join('');

// parse_job_html. `dom` = Ergebnis von ctx.cap.doc.html für JOB_HTML_QUERIES,
// `fragmentText(html)` = ctx.cap.doc.html({html, fragment_text:true}).
export async function parseJobHtml(url, html, dom, fragmentText) {
  let jsonLd = null;
  for (const text of Array.isArray(dom.ld_json) ? dom.ld_json : []) {
    try {
      const found = findJobposting(JSON.parse(text));
      if (found) {
        jsonLd = found;
        break;
      }
    } catch {
      // serde_json::from_str schlägt fehl → nächstes Skript
    }
  }
  const title = (
    (str(jsonLd?.title) !== null ? cleanText(jsonLd.title) : null) ??
    metaContent(dom.og_title) ??
    selectText(dom.title) ??
    'Unbenannte Stelle'
  )
    .replaceAll(' - Bosch Stellenportal', '')
    .replaceAll(' Stellendetails | Alfred Kärcher SE & Co. KG', '');
  const descriptionHtml = str(jsonLd?.description) ?? '';
  let rawText = descriptionHtml.trim() !== ''
    ? normalizeLines(await fragmentText(descriptionHtml))
    : selectText(dom.jobdescription) ?? selectText(dom.body) ?? '';
  rawText = normalizeLines(rawText);

  const org = jsonLd?.hiringOrganization;
  const orgName = org && typeof org === 'object' ? (org.name !== undefined ? org.name : org['@value']) : undefined;
  const companyName = (typeof orgName === 'string' ? cleanText(orgName) : null) ?? micro(dom.org) ?? inferCompanyFromUrl(url);
  const place = jsonLd?.jobLocation !== undefined && jsonLd?.jobLocation !== null ? firstJson(jsonLd.jobLocation) : undefined;
  const address = place && typeof place === 'object' ? place.address : undefined;
  const addrText = address !== undefined ? addressText(address) : '';
  const location = addrText.trim() !== '' ? addrText : selectText(dom.geo) ?? '';
  const city = (str(address?.addressLocality) !== null ? cleanText(address.addressLocality) : null) ??
    selectText(dom.city) ?? location.split(',')[0].trim();
  const zip = str(address?.postalCode) ?? '';
  const street = str(address?.streetAddress) !== null ? cleanText(address.streetAddress) : null;
  const externalRef = str(jsonLd?.identifier?.value) ??
    regexCapture(html, /"idFS"\s*:\s*"([^"]+)"/u) ??
    regexCapture(url, /[?&]id=([^&]+)/u);
  const datePosted = str(jsonLd?.datePosted) !== null ? takeChars(jsonLd.datePosted, 10)
    : micro(dom.date) !== null ? takeChars(dom.date, 10) : null;
  const companyWebsite = str(org?.sameAs) ?? originUrl(url);
  const companyLogo = str(org?.logo);

  const [aboutCompany, aboutRole, requirements, benefits, closingNotes] = splitJobSections(rawText);
  const benefitsVec = bulletize(benefits);
  const lower = rawText.toLowerCase();
  return {
    source_name: url.includes('bosch.') ? 'Bosch Stellenportal'
      : url.includes('kaercher.') || url.includes('karcher.') ? 'Kärcher Careers' : originUrl(url) ?? 'Other',
    title,
    company_name: companyName,
    company_website: companyWebsite,
    company_logo: companyLogo,
    external_ref: externalRef,
    date_posted: datePosted,
    location: location === '' ? city : location,
    street,
    city,
    zip,
    country: 'DE',
    country_name: 'Deutschland',
    work_model: 'Vollzeit',
    contract_type: lower.includes('unbefristet') ? 'unbefristet' : null,
    remote: lower.includes('home-office') || lower.includes('mobiles arbeiten'),
    about_company: aboutCompany,
    about_role: aboutRole,
    responsibilities: bulletize(aboutRole),
    candidate_requirements: requirements,
    requirements: bulletize(requirements),
    benefits: benefitsVec,
    closing_notes: closingNotes,
    raw_text: rawText,
    fachlevel_class: inferFachlevel(aboutRole, requirements),
    incentives_value: benefitsVec.length >= 4 ? 2 : 1,
    confidence: 0.86,
  };
}

// Kernform doc.html (Strang CAPSIG, §16.32): je Abfrage ein Aufruf
// doc.html({blob_id:{ref}, selector, attribute?, limit?}) → {matches:[text|attr|null]}. Die Modi von
// JOB_HTML_QUERIES bildet der Handler ab: all_text = alle Treffer, text = erster Treffer, attr = Attribut des
// ersten Treffers, micro = content-Attribut, sonst clean_text des Texts (importer.rs micro_meta).
// Abweichung zu Rust select_text: der Kern verbindet Textknoten ohne "\n" (scraper text().join("")).
export async function queryJobDom(ctx, ref) {
  const dom = {};
  const first = (r) => (Array.isArray(r?.matches) && r.matches.length > 0 ? r.matches[0] : null);
  for (const q of JOB_HTML_QUERIES) {
    if (q.mode === 'all_text') {
      const r = await ctx.cap.doc.html({ blob_id: { ref }, selector: q.selector });
      dom[q.key] = (Array.isArray(r?.matches) ? r.matches : []).filter((v) => typeof v === 'string');
    } else if (q.mode === 'attr') {
      dom[q.key] = first(await ctx.cap.doc.html({ blob_id: { ref }, selector: q.selector, attribute: q.attr, limit: 1 }));
    } else if (q.mode === 'micro') {
      const content = first(await ctx.cap.doc.html({ blob_id: { ref }, selector: q.selector, attribute: 'content', limit: 1 }));
      if (typeof content === 'string') dom[q.key] = content;
      else {
        const text = first(await ctx.cap.doc.html({ blob_id: { ref }, selector: q.selector, limit: 1 }));
        dom[q.key] = typeof text === 'string' ? cleanText(text) : null;
      }
    } else {
      dom[q.key] = first(await ctx.cap.doc.html({ blob_id: { ref }, selector: q.selector, limit: 1 }));
    }
  }
  return dom;
}
