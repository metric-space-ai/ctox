// Port of the Outbound "Update-Verteiler" report (deterministic Fachlogik).
//
// ref: outbound_update_digest.rs (origin/main 4794af03d)
//      :80 parse_config, :119 parse_hhmm, :126 is_plausible_email, :139 parse_recipients
//      :188-509 text/millis/lead helpers, short_reason, source_problem, registry runs,
//               source_target_key, registry_problem, local_label, plural
//      :569 collect_digest, :807 digest_subject, :853 render_text, :1065 render_html
//      :1290 build_report, :1400 mark_as_correction
// Native pieces stay capabilities (see outbound.update_digest.send_now.js).
//
// RUST-EIGENHEIT / ABWEICHUNG markers as in the other ports.
import { has, isObject } from './olg.js';

export const DIGEST_CONFIG_ID = 'outbound_update_digest_v1';
export const DIGEST_STATUS_ID = 'outbound_update_digest_status_v1';
export const POLICY_COLLECTION = 'outbound_lead_generation_research_policies';
export const LEAD_COLLECTION = 'outbound_lead_generation_leads';
export const ADAPTER_COLLECTION = 'outbound_lead_generation_adapters';
export const SOURCE_COLLECTION = 'outbound_lead_generation_sources';
export const STATE_FILE = 'outbound-update-digest-state.json';
export const FIRST_REPORT_WINDOW_MS = 24 * 60 * 60 * 1000;
const STUCK_RESEARCH_MS = 2 * 60 * 60 * 1000;
const LIST_LIMIT = 15;
const SOURCE_TEST_FRESH_MS = 3 * 24 * 60 * 60 * 1000;
const REVIEW_LIST_LIMIT = 8;
const CAMPAIGN_LIST_LIMIT = 14;
const DEFAULT_TZ = 'Europe/Berlin';

const get = (value, key) => (has(value, key) ? value[key] : undefined);
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());
const asciiUpper = (s) => String(s).replace(/[a-z]/g, (c) => c.toUpperCase());

/** Rust `String::cmp` = UTF-8 byte order = code point order. */
export function cmpStr(a, b) {
  const x = Array.from(a);
  const y = Array.from(b);
  const n = Math.min(x.length, y.length);
  for (let i = 0; i < n; i += 1) {
    const d = x[i].codePointAt(0) - y[i].codePointAt(0);
    if (d) return d < 0 ? -1 : 1;
  }
  return x.length - y.length < 0 ? -1 : x.length > y.length ? 1 : 0;
}

/** Rust `str::trim_matches(c)` for one char. */
function trimChar(s, ch) {
  let start = 0;
  let end = s.length;
  while (start < end && s[start] === ch) start += 1;
  while (end > start && s[end - 1] === ch) end -= 1;
  return s.slice(start, end);
}

// ---------------------------------------------------------------- config

/** ref: :119 — `u32::parse` accepts a leading '+'. */
export function parseHhmm(value) {
  const raw = String(value).trim();
  const at = raw.indexOf(':');
  if (at < 0) return null;
  const h = raw.slice(0, at).trim();
  const m = raw.slice(at + 1).trim();
  if (!/^\+?\d+$/.test(h) || !/^\+?\d+$/.test(m)) return null;
  const hours = Number(h);
  const minutes = Number(m);
  return hours < 24 && minutes < 60 ? hours * 60 + minutes : null;
}

/** ref: :126 */
export function isPlausibleEmail(value) {
  const v = String(value).trim();
  const at = v.indexOf('@');
  if (at < 0) return false;
  const local = v.slice(0, at);
  const domain = v.slice(at + 1);
  return local !== '' && domain.includes('.') && !domain.startsWith('.') && !domain.endsWith('.')
    && !/\s/u.test(v) && !domain.includes('@');
}

/** ref: :139 */
export function parseRecipients(value) {
  let raw = [];
  if (Array.isArray(value)) raw = value.filter((v) => typeof v === 'string');
  else if (typeof value === 'string') raw = value.split(/[,;\n]/);
  const out = [];
  for (const address of raw) {
    const a = asciiLower(address.trim());
    if (isPlausibleEmail(a) && !out.includes(a)) out.push(a);
  }
  return out;
}

/** chrono-tz `parse::<Tz>()`; ABWEICHUNG: Intl accepts IANA names case-insensitively. */
export function resolveTimezone(name) {
  if (typeof name !== 'string' || name.trim() === '') return DEFAULT_TZ;
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: name.trim() });
    return name.trim();
  } catch {
    return DEFAULT_TZ;
  }
}

/** ref: :80 parse_config → config | null */
export function parseConfig(doc) {
  const config = has(doc, 'update_digest') ? doc.update_digest : doc;
  const days = get(config, 'weekdays');
  const weekdays = Array.isArray(days)
    ? [...new Set(days.filter((d) => Number.isInteger(d) && d >= 1 && d <= 7))].sort((a, b) => a - b)
    : [];
  const minuteOfDay = parseHhmm(typeof get(config, 'time') === 'string' ? config.time : '');
  if (minuteOfDay == null) return null;
  return {
    enabled: typeof get(config, 'enabled') === 'boolean' ? config.enabled : false,
    recipients: parseRecipients(get(config, 'recipients')),
    weekdays,
    minute_of_day: minuteOfDay,
    timezone: resolveTimezone(get(config, 'timezone')),
    sender_email: asciiLower((typeof get(config, 'sender_email') === 'string' ? config.sender_email : '').trim()),
  };
}

/** ref: :57 DigestState with serde(default); any wrong-typed field → whole default. */
export function normalizeState(raw) {
  const state = { last_sent_slot: '', last_sent_at_ms: 0, attempt_slot: '', attempts: 0, last_attempt_at_ms: 0, last_error: '' };
  if (!isObject(raw)) return state;
  const types = { last_sent_slot: 's', last_sent_at_ms: 'i', attempt_slot: 's', attempts: 'u', last_attempt_at_ms: 'i', last_error: 's' };
  for (const [key, type] of Object.entries(types)) {
    if (!has(raw, key)) continue;
    const v = raw[key];
    const ok = type === 's' ? typeof v === 'string' : Number.isInteger(v) && (type !== 'u' || v >= 0);
    if (!ok) return { last_sent_slot: '', last_sent_at_ms: 0, attempt_slot: '', attempts: 0, last_attempt_at_ms: 0, last_error: '' };
    state[key] = v;
  }
  return state;
}

// ---------------------------------------------------------------- lead helpers

export function text(value, key) {
  const v = get(value, key);
  return typeof v === 'string' ? v.trim() : '';
}

export function millis(value, key) {
  const v = get(value, key);
  if (typeof v !== 'number' || !Number.isFinite(v)) return 0;
  return Math.trunc(v);
}

function leadName(lead) {
  const name = text(lead, 'name');
  if (name) return name;
  const firm = text(get(lead, 'data'), 'firma_name');
  if (firm) return firm;
  return text(lead, 'id');
}

function isTestLead(lead) {
  const marker = (v) => asciiUpper(v).startsWith('UITEST-');
  return marker(text(lead, 'name')) || marker(text(lead, 'campaign'));
}

function researchFinishedMs(lead) {
  const finished = millis(get(lead, 'payload'), 'research_finished_at_ms');
  return finished > 0 ? finished : millis(lead, 'research_updated_at_ms');
}

function fieldCounts(lead) {
  const fields = get(lead, 'field_status');
  if (!isObject(fields)) return [0, 0, 0];
  const values = Object.values(fields);
  const withStatus = (wanted) => values.filter((v) => get(v, 'status') === wanted).length;
  return [withStatus('verified'), withStatus('action_required'), values.length];
}

function contactCount(lead) {
  const contacts = get(lead, 'contacts');
  return Array.isArray(contacts) ? contacts.length : 0;
}

/**
 * ref: mission/channels/outbound_review.rs:659 ensure_founder_outbound_body_text_clean → Fehlertext (wörtlich) oder null.
 * Header-Zeilen in den ersten fünf nicht-leeren Zeilen, danach interne Begriffe.
 */
export function founderBodyTextGate(body) {
  const lowered = asciiLower(body);
  const firstLines = String(body).split('\n').map((l) => l.replace(/\r$/, '').trim()).filter((l) => l !== '').slice(0, 5);
  const prefixes = ['an:', 'to:', 'cc:', 'bcc:', 'betreff:', 'subject:'];
  const headerHits = firstLines.filter((line) => prefixes.some((p) => asciiLower(line).startsWith(p)));
  if (headerHits.length > 0) {
    return `founder/owner outbound email failed communication review because addressing or subject headers were placed in the message body: ${headerHits.join(', ')}`;
  }
  const markers = ['/home/', 'queue:', 'runtime/ctox.sqlite3', 'strategic direction setup', 'review rework', 'review-rework',
    'self-work', 'thread_key', 'message_key', 'conversation_id', 'lease_owner', 'route_status', 'routing-state',
    'review-approval', 'review approval', 'send-proof', 'send proof', 'outbound-message-row', 'outbound message row',
    'review/send proof', 'inbound `email:', 'steht jetzt auf `handled`', 'status `handled`', 'sqlite', 'host-pfad',
    'host-pfade', 'vps-pfad', 'api.qrserver.com', 'qrserver.com', 'public server', 'public link', 'oeffentlicher server',
    'oeffentlicher link', 'offentlicher server', 'offentlicher link'];
  const hits = markers.filter((m) => lowered.includes(m));
  return hits.length > 0 ? `founder/owner outbound email failed communication review due to internal-language leakage: ${hits.join(', ')}` : null;
}

/** ref: wie founderBodyTextGate → true when clean. */
export function founderBodyTextClean(body) {
  return founderBodyTextGate(body) == null;
}

/** ref: :254 short_reason */
export function shortReason(raw) {
  const first = String(raw).split(/[\n|]/)[0].trim().replace(/\.+$/, '');
  if (first === '') return null;
  const chars = Array.from(first);
  let short = chars.slice(0, 110).join('');
  if (chars.length > 110) short += '…';
  if (!founderBodyTextClean(short) || short.includes('`') || short.includes('::')) {
    return 'technischer Fehler, Details in der App';
  }
  return short;
}

function isReconciliationNoise(error) {
  const lowered = asciiLower(error);
  return ['reconcil', 'adapter-abgleich', 'command writeback failed', 'unsupported status', 'queue lease'].some((m) => lowered.includes(m));
}

/** ref: :290 */
export function sourceProblem(source, adapter) {
  const adapterStatus = adapter ? text(adapter, 'status') : '';
  const status = asciiLower(adapterStatus !== '' ? adapterStatus : text(source, 'adapter_status'));
  const scrape = asciiLower(adapter ? text(adapter, 'scrape_status') : '');
  const adapterAuth = adapter ? text(adapter, 'auth_status') : '';
  const auth = asciiLower(adapterAuth !== '' ? adapterAuth : text(source, 'auth_status'));
  const error = adapter ? text(adapter, 'last_error') : '';
  if (isReconciliationNoise(error)) return null;
  if (auth === 'credential_missing') return 'Zugangsdaten fehlen';
  if (scrape === 'script_required') return 'noch kein Abruf-Skript hinterlegt';
  if (status.includes('blocked') || scrape === 'blocked') return 'Zugriff blockiert';
  if (status.includes('portal_drift')) return 'Seitenaufbau geändert, Abruf muss angepasst werden';
  if (status.includes('temporary_unreachable')) return 'bei der letzten Prüfung nicht erreichbar';
  if (status.includes('fields_missing')) return 'liefert nicht alle erwarteten Felder';
  if (status.includes('failed') || scrape.includes('failed') || scrape.includes('error')) return 'letzte Prüfung fehlgeschlagen';
  return null;
}

/** ref: :348 — ABWEICHUNG: RFC 3339 checked by regex, parsed by Date.parse. */
export function isoMs(value) {
  const v = String(value).trim();
  if (!/^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/.test(v)) return 0;
  const ms = Date.parse(v.replace(' ', 'T'));
  return Number.isFinite(ms) ? ms : 0;
}

function runSucceeded(status) {
  return status === 'succeeded' || status === 'completed_empty' || status === 'partial_output';
}

/**
 * ref: :362 load_registry_runs — the two SQL queries run over the OLG run collection
 * (scrape-registry.js registryRuns → {last:[{target_key,status,started_at,result_json}],
 * ok:[{target_key,started_at}]}); folding into RegistryRun is ported here.
 */
export function foldRegistryRuns(rows) {
  const out = new Map();
  for (const row of Array.isArray(get(rows, 'last')) ? rows.last : []) {
    if (typeof get(row, 'target_key') !== 'string' || typeof get(row, 'status') !== 'string') continue;
    let result = null;
    try {
      result = JSON.parse(typeof row.result_json === 'string' ? row.result_json : '');
    } catch {
      result = null;
    }
    const detail = [get(result, 'detail'), get(result, 'failure_mode')]
      .filter((v) => typeof v === 'string').map((v) => v.trim()).filter((v) => v !== '').join(' · ');
    // BTreeMap::insert: a later row for the same key replaces the earlier one.
    out.set(row.target_key, {
      status: row.status,
      at_ms: isoMs(typeof row.started_at === 'string' ? row.started_at : ''),
      detail: Array.from(detail).slice(0, 300).join(''),
      last_ok_ms: 0,
    });
  }
  for (const row of Array.isArray(get(rows, 'ok')) ? rows.ok : []) {
    const run = typeof get(row, 'target_key') === 'string' ? out.get(row.target_key) : undefined;
    if (run) run.last_ok_ms = isoMs(typeof row.started_at === 'string' ? row.started_at : '');
  }
  return out;
}

/** ref: :432 */
export function sourceTargetKey(source) {
  const declared = text(source, 'target_key');
  if (declared) return declared;
  let key = '';
  for (const ch of text(source, 'id')) {
    if (/^[A-Za-z0-9]$/.test(ch)) key += ch.toLowerCase();
    else if (!key.endsWith('-')) key += '-';
  }
  return trimChar(key, '-');
}

const MISSING_CREDENTIALS = 'Keine Zugangsdaten hinterlegt – in den Recherche-Einstellungen unter Quellen & Zugänge eintragen';

/** ref: :464 */
export function registryProblem(run) {
  const t = asciiLower(`${run.status} ${run.detail}`);
  if (t.includes('customer is not active') || t.includes('account is not active') || t.includes('provider_account_inactive')) {
    return 'Konto beim Anbieter inaktiv';
  }
  if ((t.includes('missing') || t.includes('not configured') || t.includes('fehlt'))
    && (t.includes('credential') || t.includes('token') || t.includes('api key') || t.includes('login'))) {
    return MISSING_CREDENTIALS;
  }
  if (t.includes('authorization') || t.includes('auth_required') || t.includes('credential')) return 'Zugang wurde abgelehnt';
  if (t.includes('portal_drift')) return 'Seitenaufbau geändert, Abruf muss angepasst werden';
  if (t.includes('blocked')) return 'Zugriff blockiert';
  if (t.includes('temporary_unreachable') || t.includes('unreachable')) return 'beim letzten Abruf nicht erreichbar';
  return 'letzter Abruf fehlgeschlagen';
}

function sourceWideFailure(run) {
  const p = registryProblem(run);
  return p === 'Konto beim Anbieter inaktiv' || p === MISSING_CREDENTIALS || p === 'Zugang wurde abgelehnt';
}

const DAYS = ['Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa', 'So'];
const WEEKDAY_INDEX = { Mon: 0, Tue: 1, Wed: 2, Thu: 3, Fri: 4, Sat: 5, Sun: 6 };

/** Local calendar fields of an instant in an IANA zone. */
export function localParts(tz, ms) {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: tz, hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', weekday: 'short',
  }).formatToParts(new Date(ms));
  const pick = (type) => parts.find((p) => p.type === type)?.value ?? '';
  return {
    year: pick('year'), month: pick('month'), day: pick('day'),
    hour: pick('hour') === '24' ? '00' : pick('hour'), minute: pick('minute'),
    weekday: WEEKDAY_INDEX[pick('weekday')] ?? 0,
  };
}

/** ref: :496 */
export function localLabel(tz, ms, withTime) {
  const p = localParts(tz, ms);
  return withTime ? `${DAYS[p.weekday]} ${p.day}.${p.month}. ${p.hour}:${p.minute}` : `${p.day}.${p.month}.`;
}

function plural(count, one, many) {
  return `${count} ${count === 1 ? one : many}`;
}

// ---------------------------------------------------------------- collect

/** ref: :569 collect_digest */
export function collectDigest(leadsIn, sources, adapters, registry, sinceMs, nowMs, tz) {
  const inWindow = (ms) => ms > sinceMs && ms <= nowMs;
  const leads = leadsIn.filter((lead) => !isTestLead(lead));
  const p = localParts(tz, nowMs);
  const data = {
    campaigns: [],
    period: `${localLabel(tz, sinceMs, true)} bis ${localLabel(tz, nowMs, true)}`,
    date_label: `${DAYS[p.weekday]} ${p.day}.${p.month}.${p.year}`,
    researched: [], verified: 0, contacts: 0, sellify_done: [], attention: [], stale_sources: [], review: [],
    review_leads: 0, review_fields: 0, total: leads.length, total_completed: 0, total_new: 0, running: 0, total_sellify: 0,
  };
  const failed = [];
  const stuck = [];
  const sellifyFailed = [];
  const campaigns = new Map();
  for (const lead of leads) {
    const status = text(lead, 'research_status');
    let names = [text(lead, 'campaign')];
    const more = get(get(lead, 'payload'), 'weitere_kampagnen');
    if (Array.isArray(more)) names.push(...more.filter((n) => typeof n === 'string').map((n) => n.trim()));
    names = [...new Set(names.filter((n) => n !== ''))].sort(cmpStr);
    for (const name of names) {
      if (!campaigns.has(name)) campaigns.set(name, { name, leads: 0, researched: 0, completed: 0, review: 0, running: 0, open: 0 });
      const row = campaigns.get(name);
      row.leads += 1;
      if (status === 'completed') { row.researched += 1; row.completed += 1; }
      else if (status === 'needs_review' || status === 'partially_completed') { row.researched += 1; row.review += 1; }
      else if (status === 'running' || status === 'queued' || status === 'requested') row.running += 1;
      else row.open += 1;
    }
    const [verified, open, fields] = fieldCounts(lead);
    if (status === 'completed') data.total_completed += 1;
    else if (status === 'needs_review' || status === 'partially_completed') {
      data.review_leads += 1;
      data.review_fields += open;
      if (open > 0) data.review.push([leadName(lead), open]);
    } else if (status === '' || status === 'new') data.total_new += 1;
    else if (status === 'running' || status === 'queued' || status === 'requested') {
      data.running += 1;
      const updated = millis(lead, 'research_updated_at_ms');
      if (updated > 0 && nowMs - updated > STUCK_RESEARCH_MS) {
        stuck.push({ kind: 'Recherche hängt', subject: leadName(lead), detail: `kein Fortschritt seit ${localLabel(tz, updated, true)}` });
      }
    }
    if (['completed', 'needs_review', 'partially_completed'].includes(status) && inWindow(researchFinishedMs(lead))) {
      const contacts = contactCount(lead);
      data.verified += verified;
      data.contacts += contacts;
      data.researched.push({ name: leadName(lead), verified, fields, contacts, open });
    }
    if (status === 'failed' && inWindow(millis(lead, 'research_updated_at_ms'))) {
      const err = get(lead, 'research_error');
      failed.push({
        kind: 'Recherche fehlgeschlagen',
        subject: leadName(lead),
        detail: (typeof err === 'string' ? shortReason(err) : null) ?? 'Grund siehe App',
      });
    }
    const sellify = text(lead, 'sellify_status');
    if (sellify === 'completed') {
      data.total_sellify += 1;
      if (inWindow(millis(get(lead, 'payload'), 'sellify_finished_at_ms'))) data.sellify_done.push(leadName(lead));
    } else if (sellify === 'failed') {
      sellifyFailed.push({ kind: 'Sellify-Übergabe fehlgeschlagen', subject: leadName(lead), detail: 'erneut übergeben oder in der App prüfen' });
    }
  }
  const sourceRows = [];
  for (const source of sources) {
    if (get(source, 'enabled') === false) continue;
    const sourceId = text(source, 'id');
    if (sourceId === '' || sourceId === 'sellify') continue;
    const label = text(source, 'label') || sourceId;
    const run = registry.get(sourceTargetKey(source));
    if (run && run.at_ms > 0) {
      if (runSucceeded(run.status)) continue;
      const recentlyOk = run.last_ok_ms > 0 && nowMs - run.last_ok_ms <= SOURCE_TEST_FRESH_MS;
      if (recentlyOk && !sourceWideFailure(run)) continue;
      if (nowMs - run.at_ms > SOURCE_TEST_FRESH_MS) { data.stale_sources.push(label); continue; }
      const lastOk = run.last_ok_ms > 0 ? `, zuletzt erfolgreich ${localLabel(tz, run.last_ok_ms, true)}` : ', noch kein erfolgreicher Abruf';
      sourceRows.push({ kind: 'Quelle gestört', subject: label, detail: `${registryProblem(run)} (letzter Abruf ${localLabel(tz, run.at_ms, true)}${lastOk})` });
      continue;
    }
    // max_by_key returns the LAST maximum (RUST-EIGENHEIT for ties).
    let adapter = null;
    for (const candidate of adapters) {
      if (text(candidate, 'source_id') !== sourceId) continue;
      if (adapter === null || millis(candidate, 'updated_at_ms') >= millis(adapter, 'updated_at_ms')) adapter = candidate;
    }
    const problem = sourceProblem(source, adapter);
    if (problem == null) continue;
    const checked = adapter ? millis(adapter, 'updated_at_ms') : 0;
    if (checked <= 0 || nowMs - checked > SOURCE_TEST_FRESH_MS) { data.stale_sources.push(label); continue; }
    sourceRows.push({ kind: 'Quelle gestört', subject: label, detail: `${problem} (App-Test ${localLabel(tz, checked, false)})` });
  }
  sourceRows.sort((a, b) => cmpStr(a.subject, b.subject));
  data.stale_sources.sort(cmpStr);
  data.attention.push(...failed, ...stuck, ...sellifyFailed, ...sourceRows);
  const campaignRows = [...campaigns.values()].sort((a, b) => cmpStr(a.name, b.name));
  campaignRows.sort((a, b) => {
    const sa = a.name.startsWith('Sellify:') ? 0 : 1;
    const sb = b.name.startsWith('Sellify:') ? 0 : 1;
    return sa - sb || cmpStr(a.name, b.name);
  });
  data.campaigns = campaignRows;
  data.researched.sort((a, b) => b.verified - a.verified || cmpStr(a.name, b.name));
  data.review.sort((a, b) => b[1] - a[1] || cmpStr(a[0], b[0]));
  return data;
}

// ---------------------------------------------------------------- render

export function digestSubject(data) {
  let subject = `Outbound-Update ${data.date_label}: ${plural(data.researched.length, 'Recherche', 'Recherchen')} abgeschlossen, ${data.sellify_done.length} an Sellify`;
  if (data.attention.length) subject += `, ${plural(data.attention.length, 'Blocker', 'Blocker')}`;
  return subject;
}

const FOOTER_TEXT = 'Automatisch erstellt aus der Outbound-App. Empfänger und Zeitplan ändern: Outbound-App › Recherche-Einstellungen › Update-Verteiler.';
const GENERAL = 'Quelle gestört';
const generalBlockers = (d) => d.attention.filter((r) => r.kind === GENERAL);
const leadBlockers = (d) => d.attention.filter((r) => r.kind !== GENERAL);
function staleSourcesText(d) {
  return d.stale_sources.length
    ? `${plural(d.stale_sources.length, 'Quelle', 'Quellen')} seit über drei Tagen nicht neu geprüft (letzter Stand fehlerhaft): ${d.stale_sources.join(', ')}. Neu prüfen unter Recherche-Einstellungen › Quellen & Zugänge.`
    : null;
}

export function renderText(d) {
  const out = [
    `OUTBOUND-UPDATE · ${d.date_label}`, `Zeitraum: ${d.period}`, '', 'AUF EINEN BLICK',
    `Recherchen abgeschlossen: ${d.researched.length}`, `Felder belegt: ${d.verified}`,
    `Ansprechpartner gefunden: ${d.contacts}`, `An Sellify übergeben: ${d.sellify_done.length}`, `Blocker: ${d.attention.length}`, '',
  ];
  const general = generalBlockers(d);
  const stale = staleSourcesText(d);
  if (general.length || stale != null) {
    out.push('ALLGEMEINE BLOCKER (betreffen alle Recherchen)');
    for (const r of general) out.push(`- ${r.kind}: ${r.subject} – ${r.detail}`);
    if (stale != null) out.push(`Hinweis: ${stale}`);
    out.push('');
  }
  if (d.campaigns.length) {
    out.push('KAMPAGNEN (aktueller Stand)');
    for (const r of d.campaigns.slice(0, CAMPAIGN_LIST_LIMIT)) {
      out.push(`- ${r.name}: ${r.leads} Leads, ${r.researched} recherchiert (${r.completed} freigegeben, ${r.review} warten auf Prüfung), ${r.running} in Arbeit, ${r.open} nicht begonnen`);
    }
    if (d.campaigns.length > CAMPAIGN_LIST_LIMIT) out.push(`- … und ${d.campaigns.length - CAMPAIGN_LIST_LIMIT} weitere`);
    out.push('');
  }
  out.push('ABGESCHLOSSENE RECHERCHEN (im Zeitraum)');
  if (!d.researched.length) out.push('Keine Recherche in diesem Zeitraum abgeschlossen.');
  for (const r of d.researched.slice(0, LIST_LIMIT)) {
    out.push(`- ${r.name}: ${r.verified} von ${r.fields} Feldern belegt, ${plural(r.contacts, 'Ansprechpartner', 'Ansprechpartner')}${r.open > 0 ? `, ${plural(r.open, 'Feld', 'Felder')} zu prüfen` : ''}`);
  }
  if (d.researched.length > LIST_LIMIT) out.push(`- … und ${d.researched.length - LIST_LIMIT} weitere`);
  if (d.sellify_done.length) {
    out.push('', 'AN SELLIFY ÜBERGEBEN');
    for (const n of d.sellify_done.slice(0, LIST_LIMIT)) out.push(`- ${n}`);
  }
  out.push('');
  const leads = leadBlockers(d);
  out.push('BLOCKER BEI EINZELNEN LEADS');
  if (!leads.length) out.push('Keine.');
  for (const r of leads.slice(0, LIST_LIMIT)) out.push(`- ${r.kind}: ${r.subject} – ${r.detail}`);
  if (leads.length > LIST_LIMIT) out.push(`- … und ${leads.length - LIST_LIMIT} weitere`);
  if (d.review_leads > 0) {
    out.push('', `RECHERCHIERT, WARTET AUF PRÜFUNG: ${plural(d.review_leads, 'Lead', 'Leads')} warten auf Prüfung und Freigabe, ${plural(d.review_fields, 'Feld', 'Felder')} offen`);
    for (const [name, open] of d.review.slice(0, REVIEW_LIST_LIMIT)) out.push(`- ${name}: ${plural(open, 'Feld', 'Felder')}`);
    if (d.review.length > REVIEW_LIST_LIMIT) out.push(`- … und ${d.review.length - REVIEW_LIST_LIMIT} weitere`);
  }
  out.push('', 'GESAMTSTAND');
  out.push(`${plural(d.total, 'Lead', 'Leads')} gesamt: ${d.total_completed + d.review_leads} recherchiert (${d.total_completed} freigegeben, ${d.review_leads} warten auf Prüfung), ${d.running} in Arbeit, ${d.total_new} nicht begonnen, ${d.total_sellify} an Sellify übergeben.`);
  out.push('', FOOTER_TEXT);
  return out.join('\n');
}

export function esc(value) {
  return String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

const INK = '#1f2328';
const MUTED = '#656d76';
const LINE = '#d8dee4';
const PANEL = '#f6f8fa';
const ACCENT = '#c2410c';
const DANGER = '#b42318';
const OK = '#1a7f37';
const FONT = "-apple-system,'Segoe UI',Helvetica,Arial,sans-serif";

function htmlSectionTitle(title, note) {
  return `<tr><td style="padding:22px 24px 8px;"><div style="font:600 13px/1.3 ${FONT};color:${INK};text-transform:uppercase;letter-spacing:.06em;">${esc(title)}</div>${note === '' ? '' : `<div style="font:13px/1.4 ${FONT};color:${MUTED};padding-top:2px;">${esc(note)}</div>`}</td></tr>`;
}

function htmlTable(headers, rows) {
  const head = headers.map(([label, align]) => `<th align="${align}" style="font:600 12px/1.3 ${FONT};color:${MUTED};padding:8px 10px;border-bottom:1px solid ${LINE};text-align:${align};">${esc(label)}</th>`).join('');
  // zip: cells beyond the header count are dropped.
  const body = rows.map((cells) => `<tr>${cells.slice(0, headers.length).map((cell, i) => {
    const align = headers[i][1];
    return `<td align="${align}" style="font:14px/1.4 ${FONT};color:${INK};padding:9px 10px;border-bottom:1px solid ${LINE};text-align:${align};vertical-align:top;">${cell}</td>`;
  }).join('')}</tr>`).join('');
  return `<tr><td style="padding:0 14px;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="border-collapse:collapse;"><tr>${head}</tr>${body}</table></td></tr>`;
}

function htmlParagraph(t, color) {
  return `<tr><td style="padding:2px 24px 4px;font:14px/1.5 ${FONT};color:${color};">${esc(t)}</td></tr>`;
}

function moreRow(total, shown, columns) {
  if (!(total > shown)) return [];
  return [[`<span style="color:${MUTED};">… und ${total - shown} weitere in der App</span>`, ...Array(columns - 1).fill('')]];
}

function blockerCells(row) {
  return [
    `<strong>${esc(row.subject)}</strong><div style="font:12px/1.3 ${FONT};color:${DANGER};padding-top:2px;">${esc(row.kind)}</div>`,
    esc(row.detail),
  ];
}

export function renderHtml(d) {
  const kpi = (value, label, color) => `<td width="25%" align="center" style="padding:14px 6px;border-right:1px solid ${LINE};"><div style="font:700 26px/1.1 ${FONT};color:${color};">${value}</div><div style="font:12px/1.3 ${FONT};color:${MUTED};padding-top:4px;">${esc(label)}</div></td>`;
  const blockerColor = d.attention.length ? DANGER : OK;
  const rows = [
    `<tr><td style="padding:22px 24px 16px;border-bottom:3px solid ${ACCENT};"><div style="font:600 12px/1.3 ${FONT};color:${ACCENT};text-transform:uppercase;letter-spacing:.08em;">Outbound-Update</div><div style="font:700 22px/1.25 ${FONT};color:${INK};padding-top:4px;">${esc(d.date_label)}</div><div style="font:13px/1.4 ${FONT};color:${MUTED};padding-top:4px;">Zeitraum ${esc(d.period)}</div></td></tr>`,
    `<tr><td style="padding:16px 14px 0;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="border-collapse:collapse;background:${PANEL};border:1px solid ${LINE};"><tr>${kpi(d.researched.length, 'Recherchen abgeschlossen', INK)}${kpi(d.verified, 'Felder belegt', INK)}${kpi(d.sellify_done.length, 'an Sellify übergeben', INK)}${kpi(d.attention.length, 'Blocker', blockerColor).split(`border-right:1px solid ${LINE};`).join('')}</tr></table></td></tr>`,
  ];
  const general = generalBlockers(d);
  const stale = staleSourcesText(d);
  if (general.length || stale != null) {
    rows.push(htmlSectionTitle('Allgemeine Blocker', 'betreffen alle Recherchen – Quellen & Zugänge'));
    if (general.length) rows.push(htmlTable([['Betrifft', 'left'], ['Was ist los', 'left']], general.map(blockerCells)));
    if (stale != null) rows.push(htmlParagraph(stale, MUTED));
  }
  if (d.campaigns.length) {
    rows.push(htmlSectionTitle('Kampagnen', 'aktueller Stand aller Kampagnen'));
    const tr = d.campaigns.slice(0, CAMPAIGN_LIST_LIMIT).map((r) => [
      `<strong>${esc(r.name)}</strong>`, String(r.leads), `<span style="color:${OK};">${r.researched}</span>`, String(r.completed),
      String(r.review), String(r.running), r.open > 0 ? `<span style="color:${DANGER};">${r.open}</span>` : '0',
    ]);
    tr.push(...moreRow(d.campaigns.length, CAMPAIGN_LIST_LIMIT, 7));
    rows.push(htmlTable([['Kampagne', 'left'], ['Leads', 'right'], ['recherchiert', 'right'], ['freigegeben', 'right'],
      ['wartet auf Prüfung', 'right'], ['in Arbeit', 'right'], ['nicht begonnen', 'right']], tr));
  }
  rows.push(htmlSectionTitle('Abgeschlossene Recherchen', d.researched.length ? 'im Berichtszeitraum' : ''));
  if (!d.researched.length) rows.push(htmlParagraph('Keine Recherche in diesem Zeitraum abgeschlossen.', MUTED));
  else {
    const tr = d.researched.slice(0, LIST_LIMIT).map((r) => [
      `<strong>${esc(r.name)}</strong>`, `${r.verified} / ${r.fields}`, String(r.contacts),
      r.open > 0 ? `<span style="color:${DANGER};">${r.open}</span>` : '–',
    ]);
    tr.push(...moreRow(d.researched.length, LIST_LIMIT, 4));
    rows.push(htmlTable([['Firma', 'left'], ['Felder belegt', 'right'], ['Ansprechpartner', 'right'], ['zu prüfen', 'right']], tr));
  }
  if (d.sellify_done.length) {
    rows.push(htmlSectionTitle('An Sellify übergeben', ''));
    rows.push(htmlTable([['Firma', 'left']], d.sellify_done.slice(0, LIST_LIMIT).map((n) => [esc(n)])));
  }
  const leads = leadBlockers(d);
  rows.push(htmlSectionTitle('Blocker bei einzelnen Leads', leads.length ? 'braucht eine Entscheidung oder einen Handgriff' : ''));
  if (!leads.length) rows.push(htmlParagraph('Keine Blocker.', OK));
  else {
    const tr = leads.slice(0, LIST_LIMIT).map(blockerCells);
    tr.push(...moreRow(leads.length, LIST_LIMIT, 2));
    rows.push(htmlTable([['Betrifft', 'left'], ['Was ist los', 'left']], tr));
  }
  if (d.review_leads > 0) {
    rows.push(htmlSectionTitle('Recherchiert, wartet auf Prüfung',
      `${plural(d.review_leads, 'Lead', 'Leads')} warten auf Prüfung und Freigabe, ${plural(d.review_fields, 'Feld', 'Felder')} offen`));
    if (d.review.length) {
      const tr = d.review.slice(0, REVIEW_LIST_LIMIT).map(([name, open]) => [esc(name), String(open)]);
      tr.push(...moreRow(d.review.length, REVIEW_LIST_LIMIT, 2));
      rows.push(htmlTable([['Firma', 'left'], ['offene Felder', 'right']], tr));
    }
  }
  rows.push(htmlSectionTitle('Gesamtstand', ''));
  rows.push(htmlTable([['Leads', 'right'], ['recherchiert', 'right'], ['davon freigegeben', 'right'], ['wartet auf Prüfung', 'right'],
    ['in Arbeit', 'right'], ['nicht begonnen', 'right'], ['an Sellify', 'right']], [[
    `<strong>${d.total}</strong>`, `<strong>${d.total_completed + d.review_leads}</strong>`, String(d.total_completed),
    String(d.review_leads), String(d.running), String(d.total_new), String(d.total_sellify),
  ]]));
  rows.push(`<tr><td style="padding:24px 24px 22px;font:12px/1.5 ${FONT};color:${MUTED};">${esc(FOOTER_TEXT)}</td></tr>`);
  return `<!DOCTYPE html><html lang="de"><head><meta charset="utf-8"><meta name="color-scheme" content="light"><meta name="supported-color-schemes" content="light"><title>Outbound-Update</title></head><body style="margin:0;padding:0;background:#eef1f4;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#eef1f4;"><tr><td align="center" style="padding:20px 10px;"><table role="presentation" width="640" cellpadding="0" cellspacing="0" style="width:100%;max-width:640px;background:#ffffff;border:1px solid ${LINE};border-collapse:collapse;">${rows.join('')}</table></td></tr></table></body></html>`;
}

/** ref: :1290 build_report */
export function buildReport(leads, sources, adapters, registry, sinceMs, nowMs, tz) {
  const data = collectDigest(leads, sources, adapters, registry, sinceMs, nowMs, tz);
  return {
    subject: digestSubject(data),
    body: renderText(data),
    html: renderHtml(data),
    stats: {
      since_ms: sinceMs,
      until_ms: nowMs,
      researched: data.researched.length,
      verified_fields: data.verified,
      contacts: data.contacts,
      sellify_handovers: data.sellify_done.length,
      blockers: data.attention.length,
      stale_sources: data.stale_sources.length,
      needs_review: data.review_leads,
      leads_total: data.total,
    },
  };
}

/** ref: :1400 mark_as_correction (mutates report). */
export function markAsCorrection(report, replacedMs, tz) {
  const note = `Korrigierte Fassung: ersetzt das Outbound-Update vom ${localLabel(tz, replacedMs, true)}, das fehlerhafte Angaben enthielt.`;
  report.subject = `Korrigierte Fassung · ${report.subject}`;
  report.body = `${note}\n\n${report.body}`;
  const banner = `<tr><td style="padding:12px 20px;background:#fff4e5;border-bottom:1px solid #f0c78a;font:600 14px/1.4 Arial,sans-serif;color:#7a4a00;">${esc(note)}</td></tr>`;
  const start = report.html.indexOf('max-width:640px;');
  const end = start >= 0 ? report.html.indexOf('>', start) : -1;
  if (end >= 0) report.html = report.html.slice(0, end + 1) + banner + report.html.slice(end + 1);
  else report.html = `<p>${esc(note)}</p>${report.html}`;
}
