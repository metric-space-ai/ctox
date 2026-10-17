#!/usr/bin/env node
// Port-Scanner App-API 4.0: listet je App jede Stelle, die nach PORTING.md umgebaut werden muss.
// Mechanische Hilfe, kein Umschreiben. Regel-IDs = Abschnitte in PORTING.md.
//
//   node scan.mjs [--modules <dir>] [--app <id>] [--md] [--json]
//   Standard: <business-os>/modules, Ausgabe Datei:Zeile [Regel] Ausschnitt → Vorschlag.
//   --md   Markdown-Tabelle je App (SCAN-BASELINE.md), --json Rohdaten.
// Gezählt: .js/.mjs/.html je App ohne Tests (tests/, test.mjs, *.test.*, *_tests.js, e2e*, *smoke*), Kommentarzeilen
// (// … und Blockkommentare) ausgenommen; vendor/ der App (Fremdcode) ausgenommen; handlers/ ausgenommen (Node-Laufzeit §16.9, eigener Vertrag ctx.cap); CSS separat: Selektoren außerhalb von [data-app="<id>"].
import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs';
import { join, relative, extname, basename, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { scopeCss } from './scope-css.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = join(HERE, '..', '..', '..');

// [id, Muster, Vorschlag] – Reihenfolge = Priorität; eine Zeile kann mehrere Regeln treffen.
export const RULES = [
  ['R1-mount', /export\s+(?:async\s+)?function\s+mount\s*\(\s*ctx\b|\bonActivate\b/, 'export async function mount(root, api, { restored }) → { unmount, snapshot }'],
  ['R2-host', /\bctx\.(?:host|left|right)\b|\bstate\.ctx\.(?:host|left|right)\b/, 'root (Inhaltsbereich); left/right gibt es nicht mehr'],
  ['R3-db', /\bctx\.db\b|\.collection\(\s*['"`\w]|\.find\(\s*[^)]*\)\s*\.\$|\.findOne\(|\.exec\(\)|\.\$\.subscribe|\b(?:insert|upsert|bulkUpsert|incrementalPatch|incrementalModify)\(/, "api.query(coll, {selector, sort, limit, skip}).subscribe(cb) / api.get / api.put|patch|remove"],
  ['R4-sync', /\b(?:startCollection|stopCollection|restartCollections?|leaseCollection|leaseModule|activeModuleSyncLease|collectionReadiness|subscribeCollectionReadiness|collectionFreshness|subscribeCollectionFreshness|suspendCollections|resumeCollections|awaitInSync|awaitInitialReplication|requireRevision|localReplicaComplete|requestNative|readNativeCollectionDocument|readCollectionNativeDocument|dataPlaneReady|waitForDataPlaneReady)\b|\bctx\.sync\b/, 'entfällt ersatzlos (Live-Abfrage ist immer lokal und vollständig)'],
  ['R5-command', /\bcommandBus\b|\.dispatch\(\s*\{|\buntil\s*:\s*['"](?:accepted|terminal|local)|\bwaitForAccepted\b|\bwaitForTerminal\b/, 'const h = api.command(type, payload); h.status.subscribe(s => …) – nie warten; failed → api.status.report'],
  ['R6-shell', /\bctx\.(?:notifications|getActionIcon|getSvgIcon|openLeftDrawer|openRightDrawer|openBottomDrawer|closeDrawers|contextMenu|windowManager|eventBus|openDesktopApp|openModuleSource|openModuleVersions|openBusinessChat|businessChat|presence|contextActions|pinToTaskbar|unpinFromTaskbar|toggleTaskbarPin|isTaskbarPinned|reportIssue|shellStyle|desktopApps|getDesktopApps|modules|getModules|documents|actions|governance)\b|\bstate\.ctx\.(?:openLeftDrawer|openRightDrawer|openBottomDrawer|closeDrawers|getActionIcon|notifications)\b/, 'api.shell.* (setTitle, registerHeader, navigate/onRoute, dialog/menu, window, openFile); Icons/Drawer app-lokal im root (app-api.d.ts OFFEN-D)'],
  ['R7-dialog', /\bshowBusiness(?:Confirm|Prompt|Alert)\b|\bwindow\.(?:confirm|alert|prompt)\(|(?<![\w.])(?:confirm|alert|prompt)\(\s*['"`]/, 'await api.shell.dialog({kind, title, text, default})'],
  ['R8-i18n', /\bloadModuleMessages\b|\bctx\.locale\b|ctox-business-os-language/, 'api.shell.locale (LiveDoc) + api.shell.t(key), eingebettete Rückfalltexte'],
  ['R9-actor', /\bctx\.(?:session|user|actor|permissions|canModifyModule|runtimeCapabilities|storageScope)\b|\bcanModifyBusinessModule\b|\bcanUseBusinessPermission\b/, 'api.actor.{id, role, permissions.get()} – nur Spiegel, Entscheidung im Core'],
  ['R10-http', /\bfetch\(\s*['"`](?:https?:|\/)|\/api\/|\?v=|\bAPP_BUILD\b|registry\.json|mobile-apps\.json|system-apps\.json|OFFLINE_FALLBACK_CATALOG/, "kein HTTP-Datenweg; fetch(new URL('./x', import.meta.url)) ohne ?v= (SW liefert versioniert)"],
  ['R11-state', /renderListOrState|ctox-syncing|Daten werden synchronisiert|wird geladen|werden geladen|wird synchronisiert|nicht verbunden|Syncing data|Loading\b|still loading|syncingData|syncUnavailable|advancedStatus|recoveryWarning|syncDiagnostics|startMaintenanceMonitor|ctoxHealthTimer/, 'leere Liste rendert nichts; Zustand nur über api.status.report({level, code, text})'],
  ['R12-timer', /\bsetInterval\(|\bpromiseWithTimeout\b|RETRY_DELAYS|retryDelays|\bwithTimeout\(/, 'Schleifen/Retry entfernen – Live-Abo statt Polling (setInterval nur für deklarierte UI-Animation)'],
  ['R13-nav', /\blocation\.(?:reload|hash|href|assign|replace)\b|\bsessionStorage\b|\blocalStorage\b|postMessage\(|addEventListener\(\s*['"]message['"]/, 'api.shell.navigate(appId, params) / onRoute(cb); Fensterzustand → snapshot()/restored'],
  ['R14-import', /from\s+['"](?:\.\.\/)+shared\/|import\(\s*['"](?:\.\.\/)+shared\//, "import … from 'ctox:shared/<datei>.js' (SW schreibt um, §16.15)"],
  ['R15-guard', /\bwireShellV2ModuleTitle\b|\bcreateDynamicAppDataGuard\b|\bhasReviewedCollectionDataGrant\b|\bwindow\.CTOX_BUSINESS_OS_APP\b/, 'entfällt (Löschliste Entwurf §11)'],
];

function isTestPath(rel) {
  return /(^|\/)tests?\//.test(rel) || /(^|\/)test\.mjs$/.test(rel) || /\.test\.|smoke|\.browser\.mjs$|[_-]tests?\.m?js$|(^|\/)e2e[^/]*$/.test(rel) || /(^|\/)(?:node_modules|dist|qa|handlers)\//.test(rel);
}

// Fremdcode der App (modules/<id>/vendor/**, REPORT.md Strang ISO) wird nicht gescannt.
export function isVendorPath(rel) {
  return /^vendor(\/|$)/.test(rel.split('\\').join('/'));
}

function walk(dir, out = [], root = dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) {
      if (!isVendorPath(relative(root, p))) walk(p, out, root);
    }
    else out.push(p);
  }
  return out;
}

function stripComments(lines) {
  let inBlock = false;
  return lines.map((line) => {
    let s = line;
    if (inBlock) {
      const end = s.indexOf('*/');
      if (end < 0) return '';
      s = s.slice(end + 2); inBlock = false;
    }
    s = s.replace(/\/\*.*?\*\//g, '');
    const start = s.indexOf('/*');
    if (start >= 0) { inBlock = true; s = s.slice(0, start); }
    if (/^\s*\/\//.test(s)) return '';
    if (/^\s*<!--.*-->\s*$/.test(s)) return '';
    return s.replace(/(^|[^:'"`\\])\/\/(?!\/).*$/, '$1');
  });
}

export function scanApp(appDir, appId = basename(appDir)) {
  const findings = [];
  let cssUnscoped = 0;
  let codeLines = 0;
  for (const file of walk(appDir)) {
    const rel = relative(appDir, file);
    if (isTestPath(rel)) continue;
    const ext = extname(file);
    if (ext === '.css') {
      const { unscoped } = scopeCss(readFileSync(file, 'utf8'), appId);
      const shellAncestors = unscoped.filter((s) => /\.shell-window|\.business-app-window|data-shell-contract/.test(s)).length;
      cssUnscoped += unscoped.length;
      if (unscoped.length) findings.push({ file: rel, line: 0, rule: 'R16-css', text: `${unscoped.length} Selektoren ungescoped (${shellAncestors} mit Shell-Vorfahr)`, suggestion: `node v4/tools/port/scope-css.mjs ${appId} ${rel} --write` });
      continue;
    }
    if (basename(file) === 'schema.js') {
      findings.push({ file: rel, line: 0, rule: 'R17-schema', text: 'schema.js vorhanden', suggestion: 'löschen; einzige Schemaquelle collections.schema.json (§16.13)' });
    }
    if (!['.js', '.mjs', '.html'].includes(ext)) continue;
    const raw = readFileSync(file, 'utf8').split('\n');
    const lines = stripComments(raw);
    codeLines += raw.length;
    lines.forEach((line, i) => {
      if (!line.trim()) return;
      for (const [id, re, suggestion] of RULES) {
        if (re.test(line)) findings.push({ file: rel, line: i + 1, rule: id, text: raw[i].trim().slice(0, 140), suggestion });
      }
    });
  }
  return { app: appId, codeLines, cssUnscoped, findings };
}

export function scanModules(modulesDir) {
  return readdirSync(modulesDir)
    .filter((name) => statSync(join(modulesDir, name)).isDirectory() && existsSync(join(modulesDir, name, 'module.json')))
    .sort()
    .map((name) => scanApp(join(modulesDir, name), name));
}

function ruleCounts(findings) {
  const counts = {};
  for (const f of findings) if (f.rule !== 'R16-css') counts[f.rule] = (counts[f.rule] || 0) + 1;
  return counts;
}

function toMarkdown(results, modulesDir) {
  const ids = [...RULES.map(([id]) => id), 'R17-schema'];
  const short = (id) => id.split('-')[0];
  const out = [];
  out.push(`| App | Zeilen (Code) | Stellen | ${ids.map(short).join(' | ')} | CSS ungescoped |`);
  out.push(`|---|---:|---:|${ids.map(() => '---:').join('|')}|---:|`);
  let total = 0, totalCss = 0;
  const totals = {};
  for (const r of results) {
    const c = ruleCounts(r.findings);
    const n = Object.values(c).reduce((a, b) => a + b, 0);
    total += n; totalCss += r.cssUnscoped;
    for (const id of ids) totals[id] = (totals[id] || 0) + (c[id] || 0);
    out.push(`| ${r.app} | ${r.codeLines} | ${n} | ${ids.map((id) => c[id] || 0).join(' | ')} | ${r.cssUnscoped} |`);
  }
  out.push(`| **Summe (${results.length} Apps)** | ${results.reduce((a, r) => a + r.codeLines, 0)} | **${total}** | ${ids.map((id) => totals[id]).join(' | ')} | ${totalCss} |`);
  return { table: out.join('\n'), total, totalCss, apps: results.length, modulesDir };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
  const modulesDir = opt('--modules') || join(BOS, 'modules');
  const app = opt('--app');
  const results = app ? [scanApp(join(modulesDir, app), app)] : scanModules(modulesDir);
  const total = results.reduce((a, r) => a + r.findings.filter((f) => f.rule !== 'R16-css').length, 0);
  if (args.includes('--json')) process.stdout.write(JSON.stringify(results, null, 2) + '\n');
  else if (args.includes('--md')) process.stdout.write(toMarkdown(results, modulesDir).table + '\n');
  else {
    for (const r of results) {
      console.log(`\n## ${r.app} – ${r.findings.filter((f) => f.rule !== 'R16-css').length} Stellen, CSS ungescoped ${r.cssUnscoped}`);
      for (const f of r.findings) console.log(`${r.app}/${f.file}:${f.line} [${f.rule}] ${f.text}\n    → ${f.suggestion}`);
    }
    console.log(`\nGesamt: ${results.length} Apps, ${total} Stellen.`);
  }
  // Exit 1, solange Stellen offen sind (als Wächter je App nutzbar: --app <id>).
  process.exitCode = total ? 1 : 0;
}
