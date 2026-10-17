// Gemeinsamer Testaufbau für research gegen den eingefrorenen Kern-Stub (createStubKernel).
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/4b0d0ac), read-only unter /__stub/.
// Playwright: PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

export const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
export const BOS = normalize(join(dirname(fileURLToPath(import.meta.url)), '../../..'));
const require = createRequire(import.meta.url);
export const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

export const DOMAIN = 'research/drone-bearings';
export const TASK = {
  id: 'research_drone_bearings',
  title: 'Drohnen-Lagerlasten',
  prompt: 'Lagerlasten für Drohnenantriebe recherchieren',
  criteria: '',
  status: 'ready',
  knowledge_domain: DOMAIN,
  source_catalog_key: 'source_catalog',
  payload: { user_created: true },
  created_at_ms: 1,
  updated_at_ms: 2,
};
export const TABLES = [
  {
    id: `table:${DOMAIN}:source_catalog`,
    domain: DOMAIN,
    table_key: 'source_catalog',
    title: 'Verified Source Registry',
    description: 'Research source catalog',
    row_count: 2,
    rows: [
      { source_id: 'src-a', title: 'Wälzlager unter Propellerlast', source_url: 'https://example.test/a', source_type: 'paper', publisher: 'Example Press' },
      { source_id: 'src-b', title: 'Messreihe Motorlager 2025', source_url: 'https://example.test/b', source_type: 'dataset', publisher: 'Lab B' },
    ],
    updated_at_ms: 1,
  },
  {
    id: `table:${DOMAIN}:source_candidates`,
    domain: DOMAIN,
    table_key: 'source_candidates',
    title: 'Discovery Candidates',
    row_count: 1,
    rows: [{ source_id: 'cand-1', title: 'Kandidat Lagerreibung', source_url: 'https://example.test/c' }],
    updated_at_ms: 1,
  },
];

// shell:true lädt die Basis-Stile, die in v4 die Shell um das App-Fenster legt (nur für Screenshots).
export function pageHtml(width = 1200, height = 720, { shell = false } = {}) {
  const shellCss = shell
    ? ['/ui-contract/v1/workjet-ui-contract.css', '/app.css', '/shared/base.css', '/themes/ctox-desktop-shell.css']
      .map((href) => `<link rel="stylesheet" href="${href}">`).join('\n')
      // Fenster-Container wie das Shell-Fenster (.shell-window-content): die App-CSS fragt @container business-app-window;
      // Icon-Zone wie im v4-Rahmen (v4/frame/frame.css: --shell-v2-icon-size 28px).
      + '<style>#win{container-type:inline-size;container-name:business-app-window;--shell-v2-icon-size:28px;--shell-v2-header-row-size:37px}</style>'
    : '';
  return `<!doctype html><html><head><meta charset="utf-8">
${shellCss}
<link rel="stylesheet" href="/modules/research/index.css">
<style>html,body{margin:0;background:#f4f5f7;font-family:system-ui,sans-serif}</style>
</head><body><div id="win" style="width:${width}px;height:${height}px;position:relative;overflow:hidden;background:#fff"></div></body></html>`;
}

export async function startServer(html) {
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(html); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (native Import-Map greift nicht, OFFEN-F).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return { server, base: `http://127.0.0.1:${server.address().port}/` };
}

// Im Browser: Kernel mit Plattform-Apps, Aufgabe und Knowledge-Tabellen anlegen.
export async function seedKernel(page, { task = TASK, tables = TABLES } = {}) {
  // E48 (Stub 8265bd7): Fremdlesen (business_commands, ctox_queue_tasks, knowledge_*) braucht die veröffentlichte
  // Deklaration aus dem echten Manifest (v4/manifests/research.manifest.json) und aktive Eigentümer.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('research'));
  await page.evaluate(async ({ task, tables }) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/research/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'ctox', title: 'CTOX' }, { id: 'knowledge', title: 'Knowledge' }, { id: 'research', title: 'Web Research' }]);
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('research', '1.0.0', { title: 'Web Research', manifestExtra: window.__routes });
    if (task) await kernel.sim.push('research_tasks', [task]);
    if (tables?.length) await kernel.sim.push('knowledge_tables', tables);
    await kernel.sim.settle();
    const api = kernel.appApi('research', '1.0.0');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp('research').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, { task, tables });
}
