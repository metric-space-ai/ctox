// CV-Textauswertung, treu portiert aus importer.rs (4794af03d):
// parse_candidate_text :2335-2398, extract_candidate_name :2588, filename_name
// :2614, clean_name :2648, looks_like_name :2658, extract_current_role :2684,
// clean_role_line :2735, joined_cv_line :2752, is_plausible_role :2771,
// extract_skills :2794, contains_skill :2837, language_values :2851.
import { B, D, byteLen, cleanText, lines, normalizeLines, regexCapture } from './text.js';

const splitWs = (s) => s.split(/\s+/u).filter(Boolean);
const allAsciiDigits = (s) => /^[0-9]+$/.test(s);

function trimMatches(value, pred) {
  const chars = [...value];
  let start = 0;
  let end = chars.length;
  while (start < end && pred(chars[start])) start++;
  while (end > start && pred(chars[end - 1])) end--;
  return chars.slice(start, end).join('');
}

export function cleanName(value) {
  const joined = splitWs(value).filter((p) => !allAsciiDigits(p)).join(' ');
  return trimMatches(joined, (ch) => !/\p{Alphabetic}/u.test(ch));
}

export function looksLikeName(value) {
  const cleaned = cleanName(value);
  const lower = cleaned.toLowerCase();
  const parts = splitWs(cleaned);
  const banned = [
    'lebenslauf', 'zur person', 'persönliche daten', 'persoenliche daten', 'berufliche tätigkeiten',
    'berufliche taetigkeiten', 'ausbildungsweg', 'geburt', 'adresse', 'telefon', 'email',
    'staatsangehörigkeit', 'staatsangehoerigkeit', 'deutsch',
  ];
  return (
    parts.length >= 2 &&
    parts.length <= 4 &&
    !cleaned.includes(':') &&
    !banned.some((b) => lower.includes(b)) &&
    parts.every((p) => /^\p{Uppercase}/u.test(p))
  );
}

export function filenameName(filename) {
  const lowerFilename = filename.toLowerCase();
  const leben = lowerFilename.startsWith('lebenslauf_')
    ? filename.replace(/^(?:Lebenslauf_)+/, '').replace(/^(?:lebenslauf_)+/, '')
    : null;
  let name = filename.replaceAll('.pdf', '').replaceAll('.PDF', '');
  for (const token of ['Lebenslauf', 'lebenslauf', 'CV', 'cv', 'JAN2026', 'KEIN Upload', '(1)']) {
    name = name.replaceAll(token, ' ');
  }
  name = name.replace(/[_-]/g, ' ');
  const parts = splitWs(name).filter((p) => !allAsciiDigits(p));
  const looksReversed = leben !== null && leben.includes('_') && !(leben.split('_')[0] ?? '').includes(' ');
  if (parts.length === 2 && looksReversed) [parts[0], parts[1]] = [parts[1], parts[0]];
  return cleanName(parts.join(' '));
}

export function extractCandidateName(filename, text) {
  const value = regexCapture(text, /Name und Vorname:\s*([^\n]+)/iu);
  if (value !== null) {
    const parts = splitWs(value);
    if (parts.length === 2) return `${parts[1]} ${parts[0]}`;
    return cleanName(value);
  }
  for (const raw of lines(text).slice(0, 8)) {
    const line = raw.trim();
    if (line === '') continue;
    if (line.startsWith('Lebenslauf')) {
      const candidate = cleanName(line.slice('Lebenslauf'.length));
      if (looksLikeName(candidate)) return candidate;
    }
    if (looksLikeName(line)) return cleanName(line);
  }
  return filenameName(filename);
}

const MONTHS = 'januar|februar|märz|maerz|april|mai|juni|juli|august|september|oktober|november|dezember';
const DATE_RANGE = new RegExp(
  [
    `${B}(?:${D}{1,2}[./])?${D}{2,4}\\s*(?:-|–|—|bis|/)\\s*(?:heute|aktuell|present|${D}{1,2}[./]?${D}{2,4})${B}`,
    `${B}(?:${MONTHS})\\s+${D}{4}\\s*(?:-|–|—|bis|/)\\s*(?:heute|aktuell|present|${D}{1,2}[./]?${D}{2,4})${B}`,
    `${B}(?:seit|ab)\\s+${D}{1,2}[./]?${D}{2,4}${B}`,
  ].join('|'),
  'giu',
);

export function cleanRoleLine(value) {
  const withoutDates = value.replace(DATE_RANGE, ' ');
  return trimMatches(cleanText(withoutDates), (ch) => ['-', '–', '—', ':', '|', '·', ','].includes(ch)).trim();
}

export function joinedCvLine(allLines, start, maxLines) {
  const out = [];
  for (const line of allLines.slice(start, start + maxLines)) {
    if (line.trim() === '') break;
    out.push(line.trim());
    const joined = out.join(' ');
    if (!joined.endsWith(' und') && !joined.endsWith(' /') && !joined.endsWith(',') && byteLen(joined) >= 32) break;
  }
  return out.join(' ');
}

export function isPlausibleRole(value) {
  const cleaned = cleanText(value);
  const lower = cleaned.toLowerCase();
  const len = byteLen(cleaned);
  return (
    len >= 8 &&
    len <= 140 &&
    !lower.includes('@') &&
    !['telefon', 'email', 'adresse', 'geburt'].some((b) => lower.includes(b)) &&
    !lower.startsWith('ich ') &&
    !lower.startsWith('ich habe') &&
    ![
      'ich habe langjährige', 'berufsausbildung', 'ausbildung:', 'lehre zum', 'führerschein',
      'fuehrerschein', 'skills', 'kenntnisse', 'lebenslauf',
    ].some((b) => lower.includes(b))
  );
}

const ROLE_PATTERNS = [
  /(?:aktuelle\s+(?:position|tätigkeit)|derzeitige\s+(?:position|tätigkeit)|current\s+role)\s*:?\s*([^\n]+)/iu,
  /(?:angestrebte\s+position|gewünschte\s+position)\s*:?\s*([^\n]+)/iu,
];

export function extractCurrentRole(text) {
  for (const pattern of ROLE_PATTERNS) {
    const value = regexCapture(text, pattern);
    if (value !== null) {
      const cleaned = cleanRoleLine(value);
      if (isPlausibleRole(cleaned)) return cleaned;
    }
  }
  const all = lines(text).map(cleanText);
  for (let idx = 0; idx < all.length; idx++) {
    const lower = all[idx].toLowerCase();
    if (!(lower.includes('heute') || lower.includes('aktuell') || lower.includes('present') || lower.includes('seit '))) continue;
    for (let offset = 0; offset <= 2; offset++) {
      const role = cleanRoleLine(joinedCvLine(all, idx + offset, 3));
      if (isPlausibleRole(role)) return role;
    }
  }
  for (const line of lines(text)) {
    const lower = line.toLowerCase();
    if (['ingenieur', 'techniker', 'entwickler', 'mechaniker', 'elektriker', 'meister', 'tester', 'manager'].some((k) => lower.includes(k))) {
      const cleaned = cleanRoleLine(line);
      if (isPlausibleRole(cleaned)) return cleaned;
    }
  }
  return null;
}

const KNOWN_SKILLS = [
  'Python', 'SQL', 'Databricks', 'Grafana', 'Tableau', 'Altium', 'C', 'C++', 'CAD', 'CATIA', 'SAP', 'MSR',
  'Gebäudeautomation', 'Automatisierung', 'HIL', 'SIL', 'ISO 26262', 'ASPICE', 'Polarion', 'dSPACE',
  'Messtechnik', 'Validierung', 'Projektleitung', 'Maschinenbau', 'Elektrotechnik', 'Sonderanlagenbau',
  'Koordination', 'Teamarbeit', 'Fahrerassistenzsysteme', 'Typisierung', 'Testmanagement',
];

export function containsSkill(text, skill) {
  if (skill === 'C') return /(^|[^A-Z0-9+#])C([^A-Z0-9+#]|$)/iu.test(text);
  if (skill === 'C++') return /(^|[^A-Z0-9+#])C\+\+([^A-Z0-9+#]|$)/iu.test(text);
  return text.toLowerCase().includes(skill.toLowerCase());
}

export function extractSkills(text) {
  return KNOWN_SKILLS.filter((s) => containsSkill(text, s));
}

export function languageValues(text) {
  const lower = text.toLowerCase();
  const out = [];
  if (lower.includes('deutsch')) out.push({ code: 'de', level: '' });
  if (lower.includes('englisch') || lower.includes('english')) out.push({ code: 'en', level: '' });
  return out;
}

const EMAIL = new RegExp(`${B}[A-Z0-9._%+-]+@[A-Z0-9.-]+\\.[A-Z]{2,}${B}`, 'iu');
const PHONE = new RegExp(`(?:\\+49|0)\\s*(?:${D}[\\s()/.-]*){6,}${D}`, 'u');
const DESIRED = /(?:Angestrebte Position|gewünschte Position)\s*:?\s*([^\n]+)/iu;
const REGION = /(?:Ort|Adresse|Wohnort)\s*:?\s*([^\n]+)/iu;
const REGION_ZIP = new RegExp(`${B}${D}{5}\\s+([A-ZÄÖÜ][^\\n,]+)`, 'u');

export function parseCandidateText(filename, text) {
  const normalized = normalizeLines(text);
  const name = extractCandidateName(filename, normalized);
  const parts = splitWs(name);
  const firstName = parts[0] ?? null;
  const popped = parts.length ? parts[parts.length - 1] : null;
  const lastName = popped !== null && (firstName !== popped || splitWs(name).length === 1) ? popped : null;
  const lower = normalized.toLowerCase();
  return {
    name,
    first_name: firstName,
    last_name: lastName,
    email: regexCapture(normalized, EMAIL),
    phone: regexCapture(normalized, PHONE),
    current_role: extractCurrentRole(normalized),
    desired_position: regexCapture(normalized, DESIRED),
    highest_degree: normalized.includes('Master') ? 'Master'
      : normalized.includes('Bachelor') ? 'Bachelor'
        : normalized.includes('Techniker') ? 'Staatlich geprüfter Techniker' : null,
    region: regexCapture(normalized, REGION) ?? regexCapture(normalized, REGION_ZIP),
    driver_license: lower.includes('führerschein') || lower.includes('fuehrerschein') ? ['B'] : [],
    languages: languageValues(normalized),
    skills: extractSkills(normalized),
    leadership: ['leitung', 'projektleitung', 'koordination', 'koordiniert', 'teamarbeit', 'fachliche führung'].some((k) => lower.includes(k))
      ? 'Hinweise auf Koordination, Teamarbeit oder fachliche Leitung im CV.'
      : null,
    confidence: text.trim() === '' ? 0.25 : 0.78,
  };
}
