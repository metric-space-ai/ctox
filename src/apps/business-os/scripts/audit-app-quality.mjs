#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '../../..');
const contract = JSON.parse(readFileSync(join(appRoot, 'qa/app-quality-contracts.json'), 'utf8'));
const strict = process.argv.includes('--strict');
const output = process.env.BUSINESS_OS_APP_QUALITY_AUDIT
  || join(repoRoot, 'output/playwright/business-os-app-quality-audit.json');
const actionPatterns = {
  filter: /filter|status|kategorie|category|scope|segment/i,
  sort: /sort|sortier|reihenfolge|order/i,
  import: /import|upload|datei|file/i,
  edit: /edit|bearbeit|ändern|change|update/i,
  install: /install/i,
  release: /release|freigabe|publish|veröffentlich/i,
  select: /select|auswähl|detail/i,
  run: /run|start|ausführ|process|verarbeit|automation|capture|erfass|present|vorstell|signature\.request|publish_clarification/i,
  approve: /approve|approval|freigab|genehmig|entscheid|signature\.sign|unterzeichn/i,
  open: /open|öffnen/i,
  create: /create|erstell|anleg|neu/i,
  move: /move|verschieb|drag/i,
  resume: /resume|fortsetz|wiederauf|folgeauftrag|follow-up|continuation|session\.prompt|retry|erneut/i,
  delegate: /delegat|übergab|handoff|weiterleit/i,
  reply: /reply|antwort/i,
  select_archetype: /archetyp|archetype/i,
  review: /review|prüf|dedup|duplicate|duplik|consent/i,
  save: /save|speicher/i,
  search: /search|such/i,
  preview: /preview|vorschau/i,
  export: /export|auskunft/i,
  erase: /erase|lösch/i,
  check: /check|prüf/i,
  schedule: /schedule|plan|termin/i,
  complete: /complete|abschließ|erledig/i,
  configure: /config|konfig|source-detail|source drawer|widget editor|widget-code|trigger-logik/i,
  handoff: /handoff|übergab|weiterleit|publish_clarification|send reviewed|geprüft senden/i,
  rollback: /rollback|zurückroll|zurücksetz/i,
  launch: /launch|start|öffnen/i,
  resize: /resize|resizer|data-resizer|größe|breite/i,
  restore: /restore|wiederherstell/i,
  cancel: /cancel|abbrech|stopp/i,
};

const results = contract.apps.map((app) => auditApp(app));
const report = {
  schema: 'ctox.business_os.app_quality_static_audit.v1',
  generated_at: new Date().toISOString(),
  scope: 'Implementation signals only; browser and business-story evidence remains authoritative.',
  ok: results.every((result) => result.gaps.length === 0),
  apps: results,
  totals: {
    apps: results.length,
    without_locale_signal: results.filter((item) => !item.signals.locale).map((item) => item.id),
    without_container_signal: results.filter((item) => !item.signals.containerResponsive).map((item) => item.id),
    without_test_signal: results.filter((item) => !item.signals.tests).map((item) => item.id),
    without_context_signal: results.filter((item) => !item.signals.contextTargets).map((item) => item.id),
    without_command_signal: results.filter((item) => !item.signals.commandBus).map((item) => item.id),
    total_gaps: results.reduce((sum, item) => sum + item.gaps.length, 0),
  },
};

mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
console.log(`Business OS app quality static audit: ${results.length} apps, ${report.totals.total_gaps} unproven signal(s)`);
console.log(`Locale gaps: ${report.totals.without_locale_signal.join(', ') || 'none'}`);
console.log(`Container gaps: ${report.totals.without_container_signal.join(', ') || 'none'}`);
console.log(`Test gaps: ${report.totals.without_test_signal.join(', ') || 'none'}`);
console.log(`Report: ${output}`);
if (strict && !report.ok) process.exit(1);

function auditApp(app) {
  const moduleDir = join(appRoot, 'modules', app.id);
  const files = walk(moduleDir);
  const platformFiles = app.id === 'desktop'
    ? [join(appRoot, 'shared/window-manager.js')]
    : [];
  const sourceFiles = [...files, ...platformFiles]
    .filter((path) => ['.html', '.js', '.mjs', '.css', '.md'].includes(extname(path)));
  const source = sourceFiles.map((path) => readFileSync(path, 'utf8')).join('\n');
  const css = files.filter((path) => path.endsWith('.css')).map((path) => readFileSync(path, 'utf8')).join('\n');
  const testFiles = files.filter((path) => /(?:^|\/)(?:test\.mjs|test\.js|[^/]+\.test\.mjs)$/.test(path));
  const actions = Object.fromEntries(app.required_actions.map((action) => [
    action,
    Boolean(actionPatterns[action]?.test(source)),
  ]));
  const commandRequired = app.required_actions.some((action) => [
    'run', 'approve', 'delegate', 'handoff', 'release', 'install', 'rollback', 'reply', 'erase', 'export', 'check',
  ].includes(action));
  const signals = {
    locale: app.id === 'desktop' || /ctx\.locale|loadModuleMessages|document\.documentElement\.lang|ctox-business-os-language/.test(source),
    themeTokens: app.id === 'desktop' || /var\(--(?:bg|surface|surface-2|text|muted|line|accent)/.test(css),
    containerResponsive: app.id === 'desktop'
      || /@container\s+business-app-window/.test(css)
      || !/@media\s*\([^)]*(?:width|height)/.test(css),
    noViewportResponsive: app.id === 'desktop' || !/@media\s*\([^)]*(?:width|height)/.test(css),
    tests: app.id === 'desktop' || testFiles.length > 0,
    contextTargets: app.id === 'desktop'
      || /contextActions|data-context-(?:record|collection|field|surface)|data-record-(?:id|type)|data-label|dataset\.context(?:Record|Collection|Field|Surface)/.test(source),
    commandBus: app.id === 'desktop'
      || !commandRequired
      || /commandBus\??\.dispatch|commandBus\.dispatch|contextActions\??\.dispatch|contextActions\.dispatch/.test(source)
      || (/commandBus/.test(source) && /\bbus\??\.dispatch|\bbus\.dispatch/.test(source)),
    directData: app.id === 'desktop' || /(?:ctx|state\.ctx)\.db|db\??\.collection|\.collection\??\.|subscribeCollections\s*\(/.test(source),
    actions,
  };
  const gaps = [];
  for (const [name, value] of Object.entries(signals)) {
    if (name === 'actions') continue;
    if (!value) gaps.push(name);
  }
  for (const [action, present] of Object.entries(actions)) {
    if (!present) gaps.push(`action:${action}`);
  }
  return {
    id: app.id,
    archetype: app.archetype,
    variant: app.variant,
    business_story: app.business_story,
    test_files: testFiles.map((path) => path.slice(moduleDir.length + 1)),
    signals,
    gaps,
  };
}

function walk(root, out = []) {
  if (!existsSync(root)) return out;
  for (const name of readdirSync(root)) {
    const path = join(root, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}
