#!/usr/bin/env node
// Strang SEED (Shell v4) – Demo-Daten für die Testinstanz welsch, wiederholbar und idempotent.
//
//   node v4/tools/seed/seed-welsch.mjs                      → nur zählen (lesend), JSON auf stdout
//   node v4/tools/seed/seed-welsch.mjs --apply              → zählen, fehlendes Material einspielen, nachzählen
//   Optionen: --url http://127.0.0.1:18765  --apps customers,invoices  --threshold 8  --out <bericht.json>
//
// Weg (wie seed-app-demo.mjs vom 07.09.): Playwright öffnet die laufende Shell über den Loopback-Tunnel (Rolle admin,
// keine Anmeldung), öffnet je App das Modul (registriert Schemas, startet Replikation) und schreibt über die
// Datenbank-Handles der Shell (window.CTOX_BUSINESS_OS_APP.db.collection(name)). Das repliziert per RxDB/WebRTC in den
// nativen Store; kein HTTP-Datenweg.
//
// Sicherheitsregeln (hart im Code, nicht abschaltbar):
//  - Nur Primärschlüssel mit Präfix „demo-“ (sonst wird die Zeile verworfen) → später per Präfix löschbar.
//  - Nur EINFÜGEN: vorhandene Primärschlüssel werden übersprungen (findByIds), nie überschrieben oder gelöscht.
//  - Eingespielt wird nur, wenn die Hauptcollection der App unter --threshold Datensätze hat.
//  - DENY: Befehls-/Queue-Collections und alles mit Außenwirkung (Versand, Konten, Freigaben, Regeln, Agentenläufe).
//  - Keine Mail-/Kanal-Daten (Threads, Nachrichten, Konten); Outbound-Kampagnen nur als Entwurf/pausiert; Outbound-Nachrichten,
//    -Engagements, -Freigaben, Sequenzen gar nicht; Outbound-Firmen ohne wartende Recherche (queued/pending → researched).
// Datenquelle: modules/<id>/tests/seed-demo.json (Schema ctox.business_os.seed_demo.v1, „$ref:coll/id“ → id).
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join, dirname, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '..', '..', '..'));
const SEED_ROOTS = [join(BOS, 'modules'), join(homedir(), 'Documents/ctox/src/apps/business-os/modules')];
const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : d; };
const URL_BASE = opt('--url', 'http://127.0.0.1:18765');
const APPLY = args.includes('--apply');
const THRESHOLD = Number(opt('--threshold', '8'));
const ONLY = String(opt('--apps', '')).split(',').filter(Boolean);
const OUT = opt('--out', null);

// Hauptcollection je App (zählt für die Schwelle). Apps ohne Eintrag: erste eigene Collection des Moduls.
export const MAIN = {
  buchhaltung: 'accounting_journal_entries', calendar: 'calendar_events', consent: 'business_consents',
  conversations: 'communication_threads', customers: 'customer_accounts', esign: 'signature_requests',
  intake: 'applications', interviews: 'interview_meetings', invoices: 'accounting_invoices', iot: 'iot_assets',
  knowledge: 'knowledge_items', kundenpipeline: 'kundenpipeline_vorgaenge', mail: 'communication_threads',
  notes: 'notes', outbound: 'outbound_companies', placements: 'placements', shiftflow: 'planning_shifts',
  submissions: 'submissions', support: 'support_conversations', matching: 'matching_requirements',
  nachweise: 'business_credentials',
};

// Collections, die nie beschrieben werden (Befehle, Queue, Konten, Versand, Freigaben, Regeln, Agentenläufe).
export const DENY = new Set([
  'business_commands', 'ctox_queue_tasks', 'communication_accounts', 'outbound_engagements', 'outbound_approvals',
  'outbound_sequences', 'outbound_sender_assignments', 'outbound_meeting_requests', 'outbound_research_runs',
  'outbound_research_adapters', 'outbound_account_limits', 'outbound_suppression_entries', 'outbound_sources',
  'support_automation_rules', 'support_assignment_policies', 'support_agent_requests', 'support_agent_suggestions',
  'support_macros', 'support_inboxes', 'calendar_sources', 'calendar_booking_pages', 'calendar_booking_holds',
  'calendar_bookings', 'iot_rulesets', 'iot_agents', 'iot_agent_status', 'accounting_dunning_runs',
  'accounting_dunning_letters', 'accounting_recurring_invoices', 'accounting_invoice_approvals', 'desktop_files',
  'outbound_messages', 'outbound_skillbooks', 'ctox_ticket_cases', 'iot_alarms', 'support_reporting_events',
  // Kanal-Projektionen: nativ führend (channels::pull_communication_*); eingehende Mails ohne Routing-Zustand gelten
  // dem nativen Router als „pending“ (mission/channels/mod.rs) und würden Agentenarbeit/Antworten auslösen.
  'communication_threads', 'communication_messages',
  // Dokumente sind blob-gestützt (Dateiinhalt im nativen Store); Demo-Zeilen ohne Blob wären kaputte Dokumente.
  'documents', 'document_versions',
]);

// Zeilenfilter je Collection: Zustände, die nichts versenden.
export const ROW_FILTER = {};
// Erzwungene Werte je Collection (Versandsperre/kein Auto-Lauf zusätzlich zur DENY-Liste).
export const ROW_FORCE = {
  outbound_campaigns: (r) => ({ ...r, status: r.status === 'draft' ? 'draft' : 'paused' }),
  outbound_companies: (r) => (/^(queued|pending|running)$/.test(String(r.research_status)) ? { ...r, research_status: 'researched' } : r),
};

function loadSeed(id) {
  for (const root of SEED_ROOTS) {
    const p = join(root, id, 'tests', 'seed-demo.json');
    if (existsSync(p)) return JSON.parse(readFileSync(p, 'utf8'));
  }
  return null;
}

const deref = (v) => {
  if (typeof v === 'string' && v.startsWith('$ref:')) return v.slice(5).split('/')[1];
  if (Array.isArray(v)) return v.map(deref);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, deref(x)]));
  return v;
};

export function planFor(id) {
  const seed = loadSeed(id);
  if (!seed) return null;
  const collections = {};
  const dropped = {};
  for (const [name, docs] of Object.entries(seed.collections || {})) {
    if (DENY.has(name)) { dropped[name] = `deny (${docs.length})`; continue; }
    const filter = ROW_FILTER[name] || (() => true);
    const force = ROW_FORCE[name] || ((r) => r);
    const rows = docs.filter(filter).map((d) => force(deref({ ...d, updated_at_ms: d.updated_at_ms === 'now' || d.updated_at_ms == null ? Date.now() : d.updated_at_ms })));
    if (rows.length < docs.length) dropped[name] = `gefiltert ${docs.length - rows.length}/${docs.length}`;
    if (rows.length) collections[name] = rows;
  }
  return { collections, dropped };
}

// Läuft in der Seite. Zählt (lesend) und fügt – nur bei apply – fehlende demo-Zeilen ein.
const PAGE_FN = async ({ id, names, main, plan, apply, threshold }) => {
  const a = window.CTOX_BUSINESS_OS_APP;
  const out = { id, counts: {}, before: null, inserted: {}, skippedExisting: {}, errors: {} };
  const count = async (name) => {
    const c = a.db.collection?.(name);
    if (!c) return null;
    try { return await c.count().exec(); } catch (e) { return `err: ${String(e?.message || e).slice(0, 120)}`; }
  };
  for (const n of names) out.counts[n] = await count(n);
  out.before = out.counts[main] ?? null;
  if (!apply || !plan) return out;
  if (typeof out.before !== 'number' || out.before >= threshold) { out.note = `kein Einspielen (Haupt=${out.before}, Schwelle ${threshold})`; return out; }
  for (const [name, rows] of Object.entries(plan)) {
    const c = a.db.collection?.(name);
    if (!c) { out.errors[name] = 'Collection nicht registriert'; continue; }
    const pk = c.schema?.primaryPath || 'id';
    const valid = rows.filter((r) => String(r[pk] ?? '').startsWith('demo-'));
    if (valid.length < rows.length) out.errors[name] = `${rows.length - valid.length} Zeilen ohne demo-Präfix verworfen`;
    // Vorhandene Schlüssel prüfen (find kappt bei 200 → in 100er-Blöcken mit $in).
    const have = new Set();
    const ids = valid.map((r) => r[pk]);
    for (let k = 0; k < ids.length; k += 100) {
      const docs = await c.find({ selector: { [pk]: { $in: ids.slice(k, k + 100) } } }).exec();
      for (const d of docs) have.add(d.primary ?? (d.toJSON ? d.toJSON()[pk] : d[pk]));
    }
    const fresh = valid.filter((r) => !have.has(r[pk]));
    out.skippedExisting[name] = valid.length - fresh.length;
    if (!fresh.length) { out.inserted[name] = 0; continue; }
    try {
      if (typeof c.bulkInsert === 'function') {
        const res = await c.bulkInsert(fresh);
        out.inserted[name] = Array.isArray(res?.success) ? res.success.length : fresh.length;
        if (res?.error?.length) out.errors[name] = `${res.error.length} Fehler: ${String(res.error[0]?.message || JSON.stringify(res.error[0])).slice(0, 240)}`;
      } else {
        let n = 0; const errs = [];
        for (const doc of fresh) { try { await c.insert(doc); n += 1; } catch (e) { errs.push(String(e?.message || e)); } }
        out.inserted[name] = n;
        if (errs.length) out.errors[name] = `${errs.length} Fehler: ${errs[0].slice(0, 240)}`;
      }
    } catch (e) { out.errors[name] = String(e?.message || e).slice(0, 240); }
  }
  for (const n of names) out.counts[n] = await count(n);
  return out;
};

async function main() {
  const require = createRequire(join(homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
  const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
  const EXE = join(homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
  const browser = await chromium.launch({ executablePath: existsSync(EXE) ? EXE : undefined });
  const report = { url: URL_BASE, at: new Date().toISOString(), apply: APPLY, threshold: THRESHOLD, apps: [] };
  try {
    const page = await (await browser.newContext({ viewport: { width: 1280, height: 900 } })).newPage();
    await page.goto(new URL('/business-os/#desktop', URL_BASE).toString(), { waitUntil: 'domcontentloaded', timeout: 60000 });
    await page.waitForFunction(() => { const a = window.CTOX_BUSINESS_OS_APP; return Boolean(a?.db) && a.dataPlaneReadyStatus === 'ready'; }, null, { timeout: 120000 });
    // Modulkatalog kommt nach dem Datenebenen-Start: warten, bis er da ist (bzw. alle gewünschten Apps enthält).
    await page.waitForFunction((only) => { const m = window.CTOX_BUSINESS_OS_APP.modules || []; return only.length ? only.every((id) => m.some((x) => x.id === id)) : m.length >= 20; }, ONLY, { timeout: 90000 }).catch(() => console.error('Modulkatalog unvollständig'));
    const mods = await page.evaluate(() => window.CTOX_BUSINESS_OS_APP.modules.map((m) => ({ id: m.id, collections: m.collections || [], source: m.source || null })));
    for (const m of mods) {
      if (ONLY.length && !ONLY.includes(m.id)) continue;
      const names = m.collections.filter((c) => !['business_commands'].includes(c));
      if (!names.length) continue;
      const mainName = MAIN[m.id] || names.find((c) => c.startsWith(`${m.id.replace(/-/g, '_')}`)) || names[0];
      const plan = APPLY ? planFor(m.id) : null;
      // Modul öffnen: registriert Schemas und Replikation (Fremd-Collections inklusive).
      try { await page.evaluate((id) => window.CTOX_BUSINESS_OS_APP.openModule(id), m.id); } catch (e) { /* App darf scheitern, Zählen geht trotzdem */ }
      await page.waitForTimeout(12000);
      // Zweite Zählung 20 s später (Replikation kann nachlaufen): Einspielen nur, wenn beide unter der Schwelle liegen.
      const first = await page.evaluate(async (n) => { const c = window.CTOX_BUSINESS_OS_APP.db.collection?.(n); return c ? c.count().exec() : null; }, mainName);
      await page.waitForTimeout(20000);
      const r = await page.evaluate(PAGE_FN, { id: m.id, names, main: mainName, plan: plan?.collections || null, apply: APPLY, threshold: THRESHOLD });
      r.firstCount = first;
      r.main = mainName; r.source = m.source; r.hasSeed = Boolean(loadSeed(m.id)); if (plan) r.dropped = plan.dropped;
      if (APPLY && Object.values(r.inserted).some((n) => n > 0)) {
        // Replikation Zeit geben (6 s + 60 ms je Zeile), bevor der Tab weiterzieht oder schließt.
        const total = Object.values(r.inserted).reduce((x, y) => x + y, 0);
        await page.waitForTimeout(6000 + total * 60);
        r.after = await page.evaluate(async (n) => window.CTOX_BUSINESS_OS_APP.db.collection(n).count().exec(), mainName);
      }
      report.apps.push(r);
      console.error(`${m.id}: ${mainName}=${r.before}${r.after != null ? ` → ${r.after}` : ''} ${r.note || ''} ${Object.keys(r.errors).length ? JSON.stringify(r.errors) : ''}`);
    }
    if (APPLY) await page.waitForTimeout(8000);
  } finally {
    await browser.close();
  }
  const json = JSON.stringify(report, null, 1);
  if (OUT) writeFileSync(OUT, json);
  console.log(json);
}

if (process.argv[1] && normalize(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => { console.error(e?.stack || e); process.exit(1); });
}
