// Wesen-Wissen je Crew-Mitglied als Continuity (anchors = gesichertes Wissen,
// narrative = Erfahrung). Treue Portierung der LCM-Continuity-Operationen, die
// crew_commands.rs:136-193 aufruft (Stand 4794af03d; Kern-Helfer aus dem Klon
// ctox-sync-v4/clones/app-laufzeit, src/core/context/lcm/). Die Dokumente sind
// ctox-App-Collections: ctox_crew_continuity (Kopf je Mitglied und Art) und
// ctox_crew_continuity_commits (unveränderliche Commit-Kette).
// Reine Funktionen bis auf applyContinuityOp (schreibt über den übergebenen Store).
import { fail, invalid } from './errors.js';
import { proseLine } from './crew.js';

export const CONTINUITY = 'ctox_crew_continuity';
export const CONTINUITY_COMMITS = 'ctox_crew_continuity_commits';
export const CONTINUITY_KINDS = ['anchors', 'narrative'];

// ref: src/core/context/lcm/runtime_support.rs:1090-1102 (continuity_template)
export const CONTINUITY_TEMPLATES = {
  narrative:
    '# CONTINUITY NARRATIVE\n\n## Situation\nsummary:\nstate:\n\n## Entries\nentry_id:\nevent_type:\nsummary:\nconsequence:\nsource_class:\nsource_ref:\nobserved_at:\n',
  anchors:
    '# CONTINUITY ANCHORS\n\n## Entries\nanchor_id:\nanchor_type:\nstatement:\nsource_class:\nsource_ref:\nobserved_at:\nconfidence:\nsupersedes:\nexpires_at:\n',
};

// ref: src/core/crew/memory.rs:20-24 (member_conversation_id) – Rust hasht den
// Thread-Schlüssel `crew:<member_id>` zu einer i64-Konversation; die App
// verwendet den Schlüssel selbst.
export function memberConversationKey(memberId) {
  return `crew:${memberId}`;
}

// ref: src/core/context/lcm/runtime_support.rs:1104-1106 (continuity_document_id)
export function continuityDocumentId(memberId, kind) {
  return `contdoc_${memberConversationKey(memberId)}_${kind}`;
}

// ref: src/core/context/lcm/mod.rs:5245-5247 (continuity_base_commit_id)
export function continuityBaseCommitId(memberId, kind) {
  return `contbase_${memberConversationKey(memberId)}_${kind}`;
}

// ref: src/core/context/lcm/mod.rs:5779-5781 (collapse_whitespace)
const collapseWhitespace = proseLine;

// Rust `str::lines()`: trennt an \n, entfernt ein folgendes \r, kein leeres Endstück.
function rustLines(text) {
  const lines = String(text).split('\n').map((l) => (l.endsWith('\r') ? l.slice(0, -1) : l));
  if (lines.length && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

// Rust `trim_start_matches("- ")`: entfernt das Präfix beliebig oft.
function trimStartMatches(text, prefix) {
  let out = text;
  while (out.startsWith(prefix)) out = out.slice(prefix.length);
  return out;
}

// Rust `str::len()` = UTF-8-Bytes (ohne TextEncoder; Handler laufen im nackten vm).
export function utf8Length(text) {
  let n = 0;
  for (const ch of String(text)) {
    const cp = ch.codePointAt(0);
    n += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
  }
  return n;
}

// ref: src/core/context/lcm/mod.rs:5323-5344 (infer_continuity_section, Arme Anchors/Narrative)
function inferSection(kind, diffLine) {
  const normalized = collapseWhitespace(diffLine).toLowerCase();
  if (kind === 'anchors') return '## Entries';
  if (normalized.startsWith('summary:') || normalized.startsWith('state:')) return '## Situation';
  return '## Entries';
}

// ref: src/core/context/lcm/mod.rs:5289-5321 (normalize_continuity_diff)
export function normalizeContinuityDiff(kind, diffText) {
  const normalized = [];
  let currentSection = null;
  let isExplicitSection = false;
  for (const rawLine of rustLines(diffText)) {
    const line = rawLine.trimEnd();
    const syntaxLine = line.trimStart();
    if (syntaxLine.trim() === '') continue;
    if (syntaxLine.startsWith('## ')) {
      currentSection = syntaxLine.trim();
      isExplicitSection = true;
      normalized.push(syntaxLine.trim());
      continue;
    }
    if (!isExplicitSection && (syntaxLine.startsWith('+') || syntaxLine.startsWith('-'))) {
      const section = inferSection(kind, syntaxLine.slice(1).trim());
      if (section && currentSection !== section) {
        currentSection = section;
        normalized.push(section);
      }
    }
    normalized.push(syntaxLine);
  }
  return normalized.join('\n');
}

// ref: src/core/context/lcm/mod.rs:5431-5455 (parse_continuity_sections)
function parseSections(base) {
  const sections = new Map();
  let current = null;
  for (const rawLine of rustLines(base)) {
    const line = rawLine.trimEnd();
    if (line.startsWith('## ')) {
      current = line;
      if (!sections.has(line)) sections.set(line, []);
      continue;
    }
    if (line.startsWith('#') || line.trim() === '') continue;
    if (current !== null) sections.get(current).push(collapseWhitespace(trimStartMatches(line, '- ').trim()));
  }
  return sections;
}

// ref: src/core/context/lcm/mod.rs:5457-5479 (render_continuity_sections)
function renderSections(template, sections) {
  const out = [];
  for (const rawLine of rustLines(template)) {
    const line = rawLine.trimEnd();
    if (line.startsWith('## ')) {
      out.push(line);
      for (const item of sections.get(line) || []) out.push(`- ${item}`);
      out.push('');
    } else if (line.startsWith('# ')) {
      out.push(line);
      out.push('');
    }
  }
  return `${out.join('\n').trimEnd()}\n`;
}

// ref: src/core/context/lcm/mod.rs:5381-5429 (apply_continuity_diff)
export function applyContinuityDiff(kind, base, diffText) {
  const sections = parseSections(base);
  let current = null;
  for (const rawLine of rustLines(diffText)) {
    const syntaxLine = rawLine.trimEnd().trimStart();
    if (syntaxLine.trim() === '') continue;
    if (syntaxLine.startsWith('## ')) {
      const section = syntaxLine.trim();
      if (!sections.has(section)) throw invalid(`unknown continuity section in diff: ${section}`);
      current = section;
      continue;
    }
    if (current === null && (syntaxLine.startsWith('+') || syntaxLine.startsWith('-'))) {
      current = inferSection(kind, syntaxLine.slice(1).trim());
    }
    if (current === null) throw invalid('continuity diff requires a section header before +/- lines');
    const entry = sections.get(current);
    if (!entry) throw invalid('diff section missing in document');
    if (syntaxLine.startsWith('+')) {
      const value = collapseWhitespace(syntaxLine.slice(1));
      if (value !== '' && !entry.includes(value)) entry.push(value);
    } else if (syntaxLine.startsWith('-')) {
      const value = collapseWhitespace(syntaxLine.slice(1));
      sections.set(current, entry.filter((existing) => existing !== value));
    } else {
      throw invalid(`unsupported continuity diff line: ${syntaxLine}`);
    }
  }
  return renderSections(base, sections);
}

// ref: src/core/context/lcm/mod.rs:5249-5267 (continuity_commit_id) – sha256 über
// Konversation, Art, Diff, Text, Zeit; erste 8 Bytes hex. Ohne WebCrypto
// (nackter vm-Kontext) deterministischer FNV-Rückfall, gleiche Form.
export async function continuityCommitId(memberId, kind, diffText, rendered, createdAt) {
  const input = `${memberConversationKey(memberId)}${kind}${diffText}${rendered}${createdAt}`;
  const subtle = globalThis.crypto && globalThis.crypto.subtle;
  if (subtle && typeof subtle.digest === 'function' && typeof TextEncoder === 'function') {
    const digest = new Uint8Array(await subtle.digest('SHA-256', new TextEncoder().encode(input)));
    return `contc_${[...digest.slice(0, 8)].map((b) => b.toString(16).padStart(2, '0')).join('')}`;
  }
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  for (const ch of input) {
    const cp = ch.codePointAt(0);
    h1 = Math.imul(h1 ^ cp, 0x01000193) >>> 0;
    h2 = Math.imul(h2 ^ cp, 0x85ebca6b) >>> 0;
  }
  return `contc_${h1.toString(16).padStart(8, '0')}${h2.toString(16).padStart(8, '0')}`;
}

// ref: src/core/context/lcm/runtime_support.rs:849-874 (ensure_continuity_document_with)
// Legt fehlende Dokumente mit Vorlage und Basis-Commit an. Rückgabe: Kopf.
export async function ensureContinuityDocument(store, memberId, kind, nowMs) {
  const id = continuityDocumentId(memberId, kind);
  const existing = await store.get(CONTINUITY, id);
  if (existing) return existing;
  const createdAt = String(nowMs); // lcm/mod.rs:6015 iso_now() = Millisekunden als Text
  const baseCommitId = continuityBaseCommitId(memberId, kind);
  const template = CONTINUITY_TEMPLATES[kind];
  await store.put(CONTINUITY_COMMITS, {
    id: baseCommitId,
    document_id: id,
    member_id: memberId,
    kind,
    parent_commit_id: null,
    diff_text: '',
    rendered_text: template,
    created_at: createdAt,
    created_at_ms: nowMs,
    updated_at_ms: nowMs,
  });
  const doc = {
    id,
    member_id: memberId,
    kind,
    head_commit_id: baseCommitId,
    content: template,
    created_at: createdAt,
    updated_at: createdAt,
    updated_at_ms: nowMs,
  };
  await store.put(CONTINUITY, doc);
  return doc;
}

// ref: src/core/context/lcm/mod.rs:2611-2628 (continuity_init_documents) – für
// Crew nur anchors + narrative (die Focus-/Mission-State-Spur nutzt Crew nicht).
export async function initContinuityDocuments(store, memberId, nowMs) {
  const out = {};
  for (const kind of CONTINUITY_KINDS) out[kind] = await ensureContinuityDocument(store, memberId, kind, nowMs);
  return out;
}

// Eine Operation (diff | replace | full) auf das Dokument anwenden, Commit
// anhängen, Kopf weiterschalten. ref: src/core/context/lcm/mod.rs:2674-2759
// (continuity_apply_diff), :2761-2821 (continuity_full_replace_document),
// :2823-2900 (continuity_string_replace_document).
export async function applyContinuityOp(store, memberId, kind, request, nowMs) {
  const document = await ensureContinuityDocument(store, memberId, kind, nowMs);
  const before = document.content;
  let diffText;
  let rendered;
  if (request.mode === 'diff') {
    diffText = normalizeContinuityDiff(kind, request.diff);
    rendered = applyContinuityDiff(kind, before, diffText);
  } else if (request.mode === 'full') {
    rendered = request.body.trim();
    if (rendered === '') throw invalid('continuity_full_replace_document: empty content');
    diffText = `<tool:full_replace len=${utf8Length(rendered)}>`;
  } else if (request.mode === 'replace') {
    const { find, replace } = request;
    if (find === '') throw invalid('continuity_string_replace_document: find is empty');
    const matches = before.split(find).length - 1;
    if (matches === 0) {
      throw fail('conflict', `continuity_string_replace_document: find string not present in ${kind} document`);
    }
    if (matches > 1) {
      throw fail(
        'conflict',
        `continuity_string_replace_document: find string matches ${matches} times in ${kind} document; refusing ambiguous replace`,
      );
    }
    rendered = before.replace(find, () => replace);
    if (rendered === before) throw fail('conflict', 'continuity_string_replace_document: replace produced no change');
    diffText = `<tool:string_replace find_len=${utf8Length(find)} replace_len=${utf8Length(replace)}>`;
  } else {
    throw invalid('mode must be diff, replace or full');
  }
  const createdAt = String(nowMs);
  const commitId = await continuityCommitId(memberId, kind, diffText, rendered, createdAt);
  await store.put(CONTINUITY_COMMITS, {
    id: commitId,
    document_id: document.id,
    member_id: memberId,
    kind,
    parent_commit_id: document.head_commit_id,
    diff_text: diffText,
    rendered_text: rendered,
    created_at: createdAt,
    created_at_ms: nowMs,
    updated_at_ms: nowMs,
  });
  const next = { ...document, head_commit_id: commitId, content: rendered, updated_at: createdAt, updated_at_ms: nowMs };
  delete next._rev;
  await store.put(CONTINUITY, next);
  return next;
}

// ref: src/core/crew/memory.rs:129-178 (entry_lines)
function entryLines(content, startKey, tagKey, primary, secondary) {
  const entries = [];
  let current = null;
  for (const raw of rustLines(content)) {
    const trimmed = trimStartMatches(raw.trim(), '- ').trim();
    if (trimmed === '' || trimmed.startsWith('#')) continue;
    const colon = trimmed.indexOf(':');
    if (colon < 0) continue;
    const key = trimmed.slice(0, colon).trim().toLowerCase();
    const value = proseLine(trimmed.slice(colon + 1));
    if (key === startKey) {
      if (current) entries.push(current);
      current = new Map();
    }
    if (current && value !== '' && !current.has(key)) current.set(key, value);
  }
  if (current) entries.push(current);
  return entries
    .filter((entry) => entry.has(primary))
    .map((entry) => {
      let line = entry.has(tagKey) ? `[${entry.get(tagKey)}] ${entry.get(primary)}` : entry.get(primary);
      if (secondary && entry.has(secondary)) line += ` — ${entry.get(secondary)}`;
      return line;
    });
}

// ref: src/core/crew/memory.rs:180-194 (anchor_lines, narrative_lines)
export const anchorLines = (anchors) => entryLines(anchors, 'anchor_id', 'anchor_type', 'statement', null);
export const narrativeLines = (narrative) => entryLines(narrative, 'entry_id', 'event_type', 'summary', 'consequence');

// Sicht `memory` am Mitglied, wie sie harness_cockpit_projections.rs:1588-1608
// (project_crew) aus der Continuity baute; die UI liest sie dort.
export function memoryView(anchorsDoc, narrativeDoc) {
  const anchors = anchorsDoc ? anchorsDoc.content : '';
  const narrative = narrativeDoc ? narrativeDoc.content : '';
  const a = anchorsDoc ? anchorsDoc.updated_at : '';
  const n = narrativeDoc ? narrativeDoc.updated_at : '';
  return {
    anchors,
    narrative,
    anchor_count: anchorLines(anchors).length,
    experience_count: narrativeLines(narrative).length,
    updated_at: a > n ? a : n,
  };
}
