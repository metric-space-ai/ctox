#!/usr/bin/env node
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { existsSync, readdirSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { extname, resolve } from 'node:path';

const businessOsRoot = resolve(import.meta.dirname, '../../..');
const require = createRequire(new URL('../../../package.json', import.meta.url));
const { chromium } = require('playwright');
const serveOnly = process.argv.includes('--serve');
const outputArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'));
const outputDir = resolve(outputArgument || 'runtime/qa/research-semantic-graph');

function chromiumExecutable() {
  const cache = resolve('runtime/browser/interactive-reference/ms-playwright');
  if (!existsSync(cache)) return undefined;
  for (const entry of readdirSync(cache).filter((name) => name.startsWith('chromium-')).sort().reverse()) {
    const candidate = resolve(cache, entry, 'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
    if (existsSync(candidate)) return candidate;
  }
  return undefined;
}

const html = `<!doctype html>
<html lang="de" data-theme="dark">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Research Graph QA</title>
    <link rel="stylesheet" href="/app.css" />
    <link rel="stylesheet" href="/shared/base.css" />
    <style>
      html, body { width: 100%; height: 100%; margin: 0; overflow: hidden; background: var(--bg); color: var(--text); }
      #host { width: 100vw; height: 100vh; container: business-app-window / inline-size; }
      #host > .ctox-workspace { width: 100%; height: 100%; }
      svg { width: 16px; height: 16px; }
    </style>
  </head>
  <body>
    <div id="host"></div>
    <script type="module">
      import { mount } from '/modules/research/index.js';

      const clusters = [
        ['Data Visualization', 'semantic graph', 'network analysis', 'knowledge mapping', 'visual analytics', 'interactive canvas', 'graph rendering', 'spatial layout', 'node centrality', 'cluster exploration', 'visual hierarchy', 'information design'],
        ['Insight Generation', 'pattern discovery', 'research synthesis', 'decision intelligence', 'hypothesis testing', 'gap analysis', 'strategic signals', 'market evidence', 'competitive insight', 'trend detection', 'sensemaking', 'recommendation'],
        ['Agent Orchestration', 'autonomous agents', 'workflow automation', 'tool execution', 'multi agent system', 'human approval', 'durable tasks', 'agent memory', 'context routing', 'execution harness', 'task delegation', 'process mining'],
        ['Enterprise Readiness', 'governance', 'audit evidence', 'data privacy', 'role permissions', 'compliance', 'reliability', 'observability', 'deployment', 'security controls', 'tenant isolation', 'production operations'],
        ['Research Quality', 'source provenance', 'evidence strength', 'scholarly sources', 'fact extraction', 'confidence score', 'citation quality', 'systematic research', 'source validation', 'contradiction check', 'methodology', 'reproducibility'],
        ['Product Strategy', 'buyer clarity', 'market category', 'value proposition', 'pricing model', 'customer proof', 'enterprise adoption', 'integration depth', 'product differentiation', 'go to market', 'use cases', 'outcome metrics'],
        ['Knowledge Systems', 'knowledge base', 'document creation', 'retrieval', 'semantic search', 'living dashboard', 'research library', 'knowledge graph', 'information ecology', 'source catalog', 'evidence matrix', 'continuous update'],
      ];
      const palette = ['#58a9d8', '#79b85a', '#f0a13d', '#df554d', '#a985d8', '#79c9c3', '#d7c34d'];
      const graphNodes = clusters.flatMap((terms, cluster) => terms.map((label, index) => ({
        node_id: 'node:' + cluster + ':' + index,
        label,
        kind: index === 0 ? 'topic' : 'concept',
        cluster_id: 'cluster:' + cluster,
        occurrences: Math.max(2, 22 - index * 1.4 + cluster),
        betweenness_centrality: index === 0 ? 0.94 - cluster * 0.045 : Math.max(0.04, 0.52 - index * 0.035 + cluster * 0.01),
        source_ids_json: JSON.stringify(['source_' + (cluster * 3 + index) % 25, 'source_' + (cluster * 5 + index + 3) % 25]),
        provenance_json: JSON.stringify({ table: 'source_catalog', method: 'cooccurrence' }),
      })));
      const graphEdges = [];
      for (let cluster = 0; cluster < clusters.length; cluster += 1) {
        for (let index = 1; index < clusters[cluster].length; index += 1) {
          graphEdges.push({ edge_id: 'hub:' + cluster + ':' + index, source_id: 'node:' + cluster + ':0', target_id: 'node:' + cluster + ':' + index, weight: 13 - index * 0.5, source_ids_json: JSON.stringify(['source_' + (cluster * 4 + index) % 25]) });
          graphEdges.push({ edge_id: 'mesh:' + cluster + ':' + index, source_id: 'node:' + cluster + ':' + index, target_id: 'node:' + cluster + ':' + ((index % 11) + 1), weight: 5 + index % 4, source_ids_json: JSON.stringify(['source_' + (cluster * 7 + index) % 25]) });
          if (index % 2 === 0) graphEdges.push({ edge_id: 'cross:' + cluster + ':' + index, source_id: 'node:' + cluster + ':' + index, target_id: 'node:' + ((cluster + index) % clusters.length) + ':' + ((index * 3) % 11 + 1), weight: 3 + index % 5, source_ids_json: JSON.stringify(['source_' + (cluster * 9 + index) % 25]) });
        }
      }
      const sourceRows = Array.from({ length: 28 }, (_, index) => ({
        source_id: 'source_' + index,
        title: ['Research benchmark', 'Product analysis', 'Enterprise field study', 'Technical architecture', 'Market evidence', 'Scholarly review', 'Customer proof'][index % 7] + ' ' + String(index + 1).padStart(2, '0'),
        source_type: index % 3 === 0 ? 'scholarly' : 'web',
        source_url: 'https://example.com/research/' + index,
        summary: clusters[index % clusters.length].join(' ') + ' evidence governance research insight',
        contribution_note: 'Verified evidence for ' + clusters[index % clusters.length][0] + ' and ' + clusters[(index + 2) % clusters.length][1],
        evidence_relevance: 92 - index,
        confidence: 0.93 - index * 0.008,
        verification_status: index === 25 ? 'verified' : index === 26 ? 'verified' : index === 27 ? 'rejected' : 'verified',
        http_status: index === 25 ? 404 : 200,
        snapshot_hash: 'sha256:research-' + index,
        evidence_eligible: index < 25,
        source_tier: 'primary',
        metadata_only: index === 26,
        relevance_status: index === 27 ? 'fachfremd' : 'relevant',
      }));
      const evidenceRows = sourceRows.flatMap((source, index) => [0, 1].map((fact) => ({
        evidence_id: 'evidence_' + index + '_' + fact,
        source_id: source.source_id,
        criterion_id: clusters[index % clusters.length][fact + 1],
        fact_label: clusters[index % clusters.length][fact + 2],
        fact_value: 82 - index + fact,
        quote: 'Verified research finding about ' + clusters[index % clusters.length][fact + 1],
        confidence: 0.91,
      })));
      const table = (key, rows) => ({ id: 'table_' + key, domain: 'competitive_ai_research', table_key: key, title: key, description: 'Research evidence table', rows, row_count: rows.length });
      const store = {
        research_tasks: [{
          id: 'research_semantic_graph',
          title: 'Autonomous AI Employee Platforms',
          prompt: 'Compare the emerging category using product depth, enterprise readiness, research quality and customer evidence.',
          criteria: 'Traceable evidence, robust agent execution, governance, integration and measurable outcomes.',
          status: 'ready',
          knowledge_domain: 'competitive_ai_research',
          source_catalog_key: 'source_catalog',
          curated_table_key: 'evaluation_matrix',
          measurements_table_key: 'evidence_points',
          payload: { graph_contract: { nodes_table_key: 'semantic_graph_nodes', edges_table_key: 'semantic_graph_edges' } },
          created_at_ms: Date.now() - 10000,
          updated_at_ms: Date.now(),
        }],
        research_runs: [], research_notes: [], business_commands: [], ctox_queue_tasks: [], documents: [], document_versions: [], document_blob_chunks: [],
        knowledge_tables: [table('source_catalog', sourceRows), table('evidence_points', evidenceRows), table('evaluation_matrix', sourceRows.slice(0, 14)), table('semantic_graph_nodes', graphNodes), table('semantic_graph_edges', graphEdges)],
      };
      const subscriptions = new Map();
      function collection(name) {
        const notify = () => subscriptions.get(name)?.forEach((listener) => listener({ name }));
        return {
          find: () => ({ exec: async () => (store[name] || []).map((value) => ({ toJSON: () => structuredClone(value) })) }),
          findOne: (id) => ({ exec: async () => {
            const value = (store[name] || []).find((item) => item.id === id);
            return value ? { toJSON: () => structuredClone(value), atomicPatch: async (patch) => Object.assign(value, patch) } : null;
          } }),
          upsert: async (value) => { const index = (store[name] ||= []).findIndex((item) => item.id === value.id); if (index >= 0) store[name][index] = structuredClone(value); else store[name].push(structuredClone(value)); notify(); },
          $: { subscribe: (listener) => { if (!subscriptions.has(name)) subscriptions.set(name, new Set()); subscriptions.get(name).add(listener); return { unsubscribe: () => subscriptions.get(name)?.delete(listener) }; } },
        };
      }
      window.__researchQa = { commands: [], graphNodes, graphEdges };
      const host = document.getElementById('host');
      const cleanup = await mount({
        host,
        locale: 'de',
        module: { id: 'research' },
        db: { collection },
        sync: { startCollection: async () => true, leaseCollection: async () => ({ release: async () => true }) },
        permissions: { canReadCollection: () => true, canWriteCollection: () => true },
        commandBus: { dispatch: async (command) => { window.__researchQa.commands.push(structuredClone(command)); return { status: 'accepted', task_status: 'queued', task_id: 'qa_task_' + window.__researchQa.commands.length }; } },
        contextActions: { dispatch: async () => ({ status: 'accepted' }) },
        getActionIcon: (name) => '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="5" fill="none" stroke="currentColor"/><path d="M5 8h6M8 5v6" stroke="currentColor"/></svg>',
        storageScope: { set: () => {} },
        canModifyModule: () => true,
        session: { user: { role: 'admin' } },
        closeDrawers: () => {},
        openRightDrawer: () => {},
      });
      window.__researchQa.cleanup = cleanup;
      window.__researchQa.ready = true;
    </script>
  </body>
</html>`;

await import('node:fs/promises').then(({ mkdir }) => mkdir(outputDir, { recursive: true }));
const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url || '/', 'http://127.0.0.1');
    if (url.pathname === '/favicon.ico') {
      response.writeHead(204, { 'cache-control': 'no-store' });
      response.end();
      return;
    }
    if (url.pathname === '/' || url.pathname === '/qa') {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
      response.end(html);
      return;
    }
    const path = resolve(businessOsRoot, '.' + url.pathname);
    if (!path.startsWith(businessOsRoot + '/')) throw new Error('invalid path');
    const content = await readFile(path);
    const mime = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' }[extname(path)] || 'application/octet-stream';
    response.writeHead(200, { 'content-type': mime, 'cache-control': 'no-store' });
    response.end(content);
  } catch (error) {
    response.writeHead(404, { 'content-type': 'text/plain' });
    response.end(String(error.message || error));
  }
});
await new Promise((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
const port = server.address().port;
if (serveOnly) {
  process.stdout.write(`Research Graph preview: http://127.0.0.1:${port}/qa\n`);
  await new Promise(() => {});
}
const browser = await chromium.launch({ executablePath: chromiumExecutable(), headless: true, args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-webgl'] });
const failures = [];
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  page.setDefaultTimeout(60000);
  page.on('console', (message) => { if (message.type() === 'error') failures.push('console: ' + message.text()); });
  page.on('pageerror', (error) => failures.push('pageerror: ' + error.message));
  page.on('requestfailed', (request) => failures.push('requestfailed: ' + request.url() + ' ' + (request.failure()?.errorText || '')));
  await page.goto(`http://127.0.0.1:${port}/qa`, { waitUntil: 'networkidle' });
  try {
    await page.waitForSelector('[data-research-graph-host]', { state: 'attached', timeout: 180000 });
  } catch (error) {
    const body = await page.locator('body').innerText().catch(() => '<body unavailable>');
    const mountState = await page.evaluate(() => ({
      qaKeys: Object.keys(window.__researchQa || {}),
      ready: window.__researchQa?.ready,
      graphHosts: document.querySelectorAll('[data-research-graph-host]').length,
      canvases: document.querySelectorAll('[data-research-graph-host] canvas').length,
    })).catch(() => ({}));
    throw new Error(`research app failed to mount: ${error.message}; state=${JSON.stringify(mountState)}; browser failures=${JSON.stringify(failures)}; body=${body.slice(0, 2000)}`);
  }
  await page.waitForSelector('[data-research-graph-host] canvas', { state: 'visible', timeout: 180000 });
  await page.waitForTimeout(3200);
  assert.equal(await page.locator('[data-evidence-status="http_error"]').count(), 1);
  assert.equal(await page.locator('[data-evidence-status="metadata_only"]').count(), 1);
  assert.equal(await page.locator('[data-evidence-status="rejected"]').count(), 1);
  assert.equal(await page.locator('.research-ranking-list .research-rank-row').count(), 25);
  assert.equal(await page.locator('.research-graph-dimension').textContent(), '3D');
  assert.ok(await page.locator('.research-graph-topics li').count() >= 5);
  assert.ok(await page.locator('[data-research-graph-host] canvas').boundingBox());
  await page.screenshot({ path: resolve(outputDir, 'desktop-3d.png'), fullPage: true, timeout: 90000 });

  await page.locator('[data-action="graph-dimension"]').click();
  await page.waitForTimeout(1000);
  assert.equal(await page.locator('.research-graph-dimension').textContent(), '2D');
  await page.locator('[data-action="graph-search"]').fill('governance');
  await page.locator('[data-action="graph-panel"][data-graph-panel="analytics"]').click();
  await page.waitForSelector('.research-graph-metrics');
  await page.screenshot({ path: resolve(outputDir, 'desktop-2d-analytics.png'), fullPage: true, timeout: 90000 });

  await page.locator('[data-action="graph-layer"][data-graph-layer="evidence"]').click();
  await page.waitForSelector('[data-research-graph-host] canvas', { state: 'visible', timeout: 60000 });
  await page.waitForTimeout(1400);
  await page.locator('[data-action="graph-ai"][data-graph-ai="document"]').click();
  await page.waitForTimeout(1500);
  assert.equal(
    await page.evaluate(() => window.__researchQa.commands.length),
    1,
    `document action did not dispatch: ${JSON.stringify(failures)}`,
  );
  assert.equal(await page.evaluate(() => window.__researchQa.commands[0].command_type), 'research.systematic.report.create');
  await page.locator('[data-action="graph-ai"][data-graph-ai="research"]').click();
  await page.waitForTimeout(1500);
  assert.equal(
    await page.evaluate(() => window.__researchQa.commands.length),
    2,
    `research action did not dispatch: ${JSON.stringify(failures)}`,
  );
  assert.equal(await page.evaluate(() => window.__researchQa.commands[1].command_type), 'research.systematic.run');

  const collapse = page.locator('[data-action="toggle-diagram"]');
  assert.equal(await collapse.count(), 1);
  await collapse.click();
  assert.equal(await page.locator('.research-graph-shell').evaluate((node) => getComputedStyle(node).display), 'none');
  await collapse.click();
  await page.waitForSelector('[data-research-graph-host] canvas', { state: 'visible', timeout: 60000 });

  await page.setViewportSize({ width: 720, height: 900 });
  await page.waitForTimeout(900);
  assert.ok(await page.locator('.research-graph-stage').isVisible());
  assert.equal(await page.locator('[data-graph-command="zoom-in"]').evaluate((node) => getComputedStyle(node).display), 'none');
  await page.screenshot({ path: resolve(outputDir, 'compact.png'), fullPage: true, timeout: 90000 });

  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForSelector('[data-research-graph-host] canvas', { state: 'visible', timeout: 60000 });
  await page.waitForTimeout(1200);
  assert.equal(await page.locator('.research-graph-dimension').textContent(), '3D');
  assert.deepEqual(failures, []);
  process.stdout.write(JSON.stringify({ ok: true, url: `http://127.0.0.1:${port}/qa`, screenshots: ['desktop-3d.png', 'desktop-2d-analytics.png', 'compact.png'], failures }, null, 2) + '\n');
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}
