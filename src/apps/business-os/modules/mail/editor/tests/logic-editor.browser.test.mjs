#!/usr/bin/env node
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const repoRoot = findRepoRoot(dirname(fileURLToPath(import.meta.url)));
const businessOsRoot = join(repoRoot, 'src/apps/business-os');

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url || '/', 'http://127.0.0.1');
    if (url.pathname === '/') {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end(testPage());
      return;
    }
    const relative = normalize(decodeURIComponent(url.pathname)).replace(/^[/\\]+/, '');
    const filePath = resolve(businessOsRoot, relative);
    const fileStats = await stat(filePath).catch(() => null);
    if (!filePath.startsWith(`${businessOsRoot}/`) || !fileStats?.isFile()) {
      response.writeHead(404).end('not found');
      return;
    }
    response.writeHead(200, { 'content-type': mimeType(filePath) });
    response.end(await readFile(filePath));
  } catch (error) {
    response.writeHead(500).end(String(error?.message || error));
  }
});

await new Promise((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
const address = server.address();
let browser;
try {
  browser = await chromium.launch({
    executablePath: findChromiumExecutable(),
    headless: true,
    args: ['--no-sandbox', '--disable-dev-shm-usage'],
    timeout: 30_000,
  });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${address.port}/`, {
    waitUntil: 'domcontentloaded',
    timeout: 30_000,
  });
  await page.waitForFunction(
    () => document.body.dataset.testStatus !== 'running',
    undefined,
    { timeout: 6000 },
  );
  const stdout = await page.content();
  assert.match(stdout, /data-test-status="passed"/);
  assert.match(stdout, /Mail logic editor browser DOM test passed/);
  console.log('Mail logic editor browser DOM test passed');
} finally {
  try {
    await browser?.close();
  } finally {
    server.closeAllConnections();
    await new Promise((resolveClose) => server.close(resolveClose));
  }
}

function testPage() {
  return `<!doctype html>
<html lang="de" data-theme="light">
<head>
  <meta charset="utf-8">
  <link rel="stylesheet" href="/shared/base.css">
  <link rel="stylesheet" href="/modules/mail/editor/editor.css">
  <style>
    :root{--bg:#f1f3f4;--surface:#f8f9fa;--surface-2:#e7eaec;--line:#c6ccd0;--text:#20262b;--text-strong:#11171c;--muted:#65717b;--accent:#237c74;--accent-soft:#d8ebe8;--danger:#b64a43;--warning:#9b6b12;--success:#237a4b;--focus-ring:#237c74}
    html,body{width:100%;min-height:100%;margin:0}body{display:grid;grid-template-columns:380px minmax(0,800px);gap:16px;align-items:start;padding:12px;background:var(--bg)}#logic{height:760px;overflow:auto;padding:8px;background:var(--surface)}#adapter{position:relative;width:800px;height:700px;min-width:0}
  </style>
</head>
<body data-test-status="running">
  <div id="logic"></div><div id="adapter"></div><pre id="result">RUNNING</pre>
  <script type="module">
    const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
    const assert = (condition, message) => { if (!condition) throw new Error(message); };
    const one = (selector, root = document) => {
      const element = root.querySelector(selector);
      if (!element) throw new Error('Missing element: ' + selector);
      return element;
    };
    const click = async (selector, root = document) => {
      one(selector, root).click();
      await settle();
    };
    const change = async (selector, value, root = document) => {
      const control = one(selector, root);
      control.value = value;
      control.dispatchEvent(new Event('change', { bubbles: true }));
      await settle();
    };
    const input = async (selector, value, root = document) => {
      const control = one(selector, root);
      control.value = value;
      control.dispatchEvent(new Event('input', { bubbles: true }));
      await settle();
    };

    try {
      const { mountMailLogicEditor } = await import('/modules/mail/editor/logic-editor-v1.mjs');
      let sourceDocument = {
        version: 1,
        content: {
          type: 'page', data: { value: {} }, children: [{
            type: 'section', data: { value: {} }, children: [{
              type: 'column', data: { value: {} }, children: [{
                type: 'text', data: { value: { content: '<p>Premium-Angebot</p>' } }, children: [],
              }],
            }],
          }],
        },
      };
      const selectedPath = 'content.children.0.children.0.children.0';
      const testDataEvents = [];
      const previewEvents = [];
      let selectionListener = null;
      mountMailLogicEditor({
        host: one('#logic'),
        locale: 'de',
        mergeTags: { contact: { segment: '', score: 0 } },
        getSelectedBlockId: () => '',
        onSelectionChange: (listener) => {
          selectionListener = listener;
          return () => { selectionListener = null; };
        },
        getDocument: () => structuredClone(sourceDocument),
        setDocument: (next) => { sourceDocument = structuredClone(next); },
        onTestDataChange: (data) => testDataEvents.push(structuredClone(data)),
        onPreviewChange: (preview) => previewEvents.push(structuredClone(preview)),
      });
      await settle();
      assert(one('[data-mail-logic-editor]'), 'editor mounted');
      assert(one('.ctox-empty').textContent.includes('Wähle'), 'empty state shown before frame selection');
      selectionListener({ idx: selectedPath });
      await settle();

      await click('[data-logic-action="add-rule"]');
      let firstRule = one('.mail-logic-rule');
      await change('[data-logic-field="field"]', 'contact.segment', firstRule);
      firstRule = one('.mail-logic-rule');
      await change('[data-logic-field="operator"]', 'equals', firstRule);
      firstRule = one('.mail-logic-rule');
      await change('[data-logic-field="valueType"]', 'string', firstRule);
      firstRule = one('.mail-logic-rule');
      await change('[data-logic-field="value"]', 'kunde', firstRule);

      await input('[data-logic-test-data]', '{"contact":{"segment":"kunde","score":90}}');
      assert(one('[data-logic-preview-result]').textContent.includes('Block wird angezeigt'), 'matching preview visible');
      const textBlock = () => sourceDocument.content.children[0].children[0].children[0];
      assert(textBlock().data.value.logic.root.children[0].field === 'contact.segment', 'rule persisted at idx path');
      assert(textBlock().data.value.logic.testData.contact.segment === 'kunde', 'test data persisted');

      await input('[data-logic-test-data]', '{"contact":{"segment":"lead","score":90}}');
      assert(one('[data-logic-preview-result]').textContent.includes('ausgeblendet'), 'non-matching preview visible');
      assert(previewEvents.at(-1).matched === false, 'canvas bridge received hidden preview state');

      await click('.mail-logic-group [data-logic-action="add-group"]');
      let groups = document.querySelectorAll('.mail-logic-group');
      assert(groups.length === 2, 'nested group created');
      await change('[data-logic-field="combinator"]', 'or', groups[1]);
      groups = document.querySelectorAll('.mail-logic-group');
      await click('[data-logic-action="add-rule"]', groups[1]);
      assert(document.querySelectorAll('.mail-logic-rule').length === 2, 'nested rule created');

      await click('.mail-logic-group [data-logic-action="add-rule"]');
      assert(document.querySelectorAll('.mail-logic-rule').length === 3, 'third rule created');
      const rows = document.querySelectorAll('.mail-logic-rule');
      const lastRuleId = rows[rows.length - 1].dataset.logicNodeId;
      await click('.mail-logic-rule[data-logic-node-id="' + lastRuleId + '"] [data-logic-action="up"]');
      assert(textBlock().data.value.logic.root.children[1].id === lastRuleId, 'reorder persisted');
      await click('.mail-logic-rule[data-logic-node-id="' + lastRuleId + '"] [data-logic-action="delete"]');
      assert(!textBlock().data.value.logic.root.children.some((node) => node.id === lastRuleId), 'delete persisted');

      await input('[data-logic-test-data]', '{bad json');
      assert(one('[data-logic-test-data]').getAttribute('aria-invalid') === 'true', 'invalid JSON exposed');
      assert(one('[data-logic-json-error]').textContent.includes('gültiges JSON'), 'invalid JSON explained');
      assert(testDataEvents.length > 1, 'preview received live test data');
      assert(previewEvents.some((event) => event.matched === true), 'canvas bridge received matching preview state');

      const { createMailContentEditor } = await import('/modules/mail/editor/mail-content-editor.mjs');
      let adapterDocument = structuredClone(sourceDocument);
      const adapterPreviews = [];
      let adapterSelectionListener = null;
      const adapter = await createMailContentEditor({
        host: one('#adapter'),
        ctx: {},
        mode: 'html',
        htmlDocument: adapterDocument,
        mergeTags: { contact: { segment: '' } },
        easyEmailRuntime: {
          async createEasyEmailEditor(runtimeOptions) {
            assert(runtimeOptions.logicBridge.managedExternally === true, 'runtime receives external logic contract');
            assert(Object.keys(runtimeOptions.panelHosts).join(',') === 'logic', 'only logic panel is adapter-owned');
            return {
              ownsPanels: true,
              getDocument: async () => structuredClone(adapterDocument),
              getHtml: async () => '<html></html>',
              setDocument: async (next) => { adapterDocument = structuredClone(next); },
              getSelectedBlockId: () => selectedPath,
              onSelectionChange(listener) {
                adapterSelectionListener = listener;
                listener({ blockId: selectedPath });
                return () => { adapterSelectionListener = null; };
              },
              setMergeTags: async () => {},
              setLogicPreview: async (preview) => adapterPreviews.push(structuredClone(preview)),
              setActivePanel: async () => {},
              focus() {},
              async destroy() {},
            };
          },
        },
      });
      await adapter.openHtmlPanel('logic');
      await settle();
      const drawer = one('.mail-content-editor-drawer', one('#adapter'));
      assert(drawer.hidden === false && drawer.getAttribute('aria-modal') === 'true', 'logic opens in modal right drawer');
      assert(one('[data-mail-logic-editor]', drawer), 'logic editor mounted inside drawer');
      assert(adapterPreviews.length > 0, 'adapter forwards visible preview state to runtime');
      adapter.closeHtmlPanel();
      assert(drawer.hidden === true, 'logic drawer closes explicitly');
      await adapter.destroy();
      assert(adapterSelectionListener === null, 'selection bridge unsubscribed on destroy');

      document.body.dataset.testStatus = 'passed';
      one('#result').textContent = 'Mail logic editor browser DOM test passed';
    } catch (error) {
      document.body.dataset.testStatus = 'failed';
      one('#result').textContent = 'FAIL: ' + (error?.stack || error);
    }
  </script>
</body>
</html>`;
}

function findRepoRoot(start) {
  let current = resolve(start);
  while (current !== dirname(current)) {
    if (existsSync(join(current, 'AGENTS.md'))) return current;
    current = dirname(current);
  }
  throw new Error('Repository root not found');
}

function findChromiumExecutable() {
  const candidates = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
  ].filter(Boolean);
  const executable = candidates.find((candidate) => existsSync(candidate));
  if (!executable) throw new Error('Chromium executable not found');
  return executable;
}

function mimeType(filePath) {
  const extension = extname(filePath);
  if (extension === '.js' || extension === '.mjs') return 'text/javascript; charset=utf-8';
  if (extension === '.css') return 'text/css; charset=utf-8';
  if (extension === '.html') return 'text/html; charset=utf-8';
  if (extension === '.json') return 'application/json; charset=utf-8';
  return 'application/octet-stream';
}
