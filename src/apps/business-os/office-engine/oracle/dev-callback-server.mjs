import { createServer } from 'node:http';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const port = Number.parseInt(process.argv[2] || '4180', 10);
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../../../..');
const featureConfig = new Map([
  ['document.edit-save', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.edit-save'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.edit-save/ctox-canonical.docx'),
  }],
  ['document.undo-clipboard-keyboard', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.undo-clipboard-keyboard'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.undo-clipboard-keyboard/ctox-canonical.docx'),
  }],
  ['document.character-paragraph-formatting', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.character-paragraph-formatting'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.character-paragraph-formatting/ctox-canonical.docx'),
  }],
  ['document.styles-lists-numbering', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.styles-lists-numbering'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.styles-lists-numbering/ctox-canonical.docx'),
  }],
  ['document.tables', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.tables'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.tables/ctox-canonical.docx'),
  }],
  ['document.images-positioning', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.images-positioning'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.images-positioning/ctox-canonical.docx'),
  }],
  ['document.sections-headers-footers', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.sections-headers-footers'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.sections-headers-footers/ctox-canonical.docx'),
  }],
  ['document.links-bookmarks-fields', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.links-bookmarks-fields'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.links-bookmarks-fields/ctox-canonical.docx'),
  }],
  ['document.comments-track-changes', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.comments-track-changes'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.comments-track-changes/ctox-canonical.docx'),
  }],
  ['document.drawings-charts', {
    outputDirectory: resolve(root, 'runtime/office-oracle/document.drawings-charts'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/document.drawings-charts/ctox-canonical.docx'),
  }],
  ['spreadsheet.edit-save', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.edit-save'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.edit-save/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.undo-clipboard-fill', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.undo-clipboard-fill'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.undo-clipboard-fill/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.cell-format-rows-columns', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.cell-format-rows-columns'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.cell-format-rows-columns/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.formulas-references', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.formulas-references'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.formulas-references/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.multi-sheet-merge-freeze', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.multi-sheet-merge-freeze'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.multi-sheet-merge-freeze/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.sort-filter-tables', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.sort-filter-tables'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.sort-filter-tables/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.validation-conditional-formatting', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.validation-conditional-formatting'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.validation-conditional-formatting/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.comments-names-protection', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.comments-names-protection'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.comments-names-protection/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.charts', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.charts'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.charts/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
  ['spreadsheet.pivot-print-layout', {
    outputDirectory: resolve(root, 'runtime/office-oracle/spreadsheet.pivot-print-layout'),
    ctoxExportPath: resolve(root, 'output/playwright/ctox-office/ctox/spreadsheet.pivot-print-layout/ctox-canonical.xlsx'),
    extension: 'xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }],
]);
const initialState = () => ({ callbacks: [], saved: false, saved_bytes: 0, saved_sha256: '' });
const states = new Map([...featureConfig.keys()].map((feature) => [feature, initialState()]));

const json = (response, status, value) => {
  response.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'access-control-allow-origin': '*',
    'cache-control': 'no-store',
  });
  response.end(JSON.stringify(value));
};

const readBody = async (request) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return Buffer.concat(chunks);
};

const documentServerUrl = (value) => {
  const url = new URL(value);
  if (url.hostname === 'localhost' || url.hostname === '127.0.0.1') {
    url.hostname = '127.0.0.1';
    url.port = '8088';
  }
  return url;
};

const sha256 = async (bytes) => {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Buffer.from(digest).toString('hex');
};

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url || '/', `http://${request.headers.host || 'localhost'}`);
    const route = /^\/(callback|capture-ctox|reset|state|saved|saved-ctox)\/((?:document|spreadsheet)\.[a-z-]+?)(?:\.(?:docx|xlsx))?$/.exec(url.pathname);
    if (request.method === 'OPTIONS') return json(response, 200, { ok: true });
    if (!route || !featureConfig.has(route[2])) return json(response, 404, { error: 'not_found' });
    const [, operation, feature] = route;
    const config = featureConfig.get(feature);
    if (request.method === 'POST' && operation === 'callback') {
      const payload = JSON.parse((await readBody(request)).toString('utf8') || '{}');
      let state = states.get(feature);
      state.callbacks.push({ at_ms: Date.now(), payload });
      if ((payload.status === 2 || payload.status === 6) && payload.url) {
        const upstream = await fetch(documentServerUrl(payload.url));
        if (!upstream.ok) throw new Error(`Oracle saved document fetch failed: ${upstream.status}`);
        const bytes = new Uint8Array(await upstream.arrayBuffer());
        await mkdir(config.outputDirectory, { recursive: true });
        await writeFile(resolve(config.outputDirectory, `oracle-saved.${config.extension || 'docx'}`), bytes);
        state = {
          ...state,
          saved: true,
          saved_bytes: bytes.byteLength,
          saved_sha256: await sha256(bytes),
        };
        states.set(feature, state);
      }
      return json(response, 200, { error: 0 });
    }
    if (request.method === 'POST' && operation === 'reset') {
      states.set(feature, initialState());
      return json(response, 200, { ok: true });
    }
    if (request.method === 'POST' && operation === 'capture-ctox') {
      const bytes = await readBody(request);
      const outputDirectory = resolve(root, 'output/playwright/ctox-office/ctox', feature);
      await mkdir(outputDirectory, { recursive: true });
      await writeFile(resolve(outputDirectory, `ctox-editor.${config.extension || 'docx'}`), bytes);
      return json(response, 200, { ok: true, bytes: bytes.byteLength, sha256: await sha256(bytes) });
    }
    if (request.method === 'GET' && operation === 'state') {
      return json(response, 200, states.get(feature));
    }
    if (request.method === 'GET' && operation === 'saved') {
      const bytes = await readFile(resolve(config.outputDirectory, `oracle-saved.${config.extension || 'docx'}`));
      response.writeHead(200, {
        'content-type': config.mimeType || 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        'content-length': bytes.byteLength,
        'cache-control': 'no-store',
      });
      return response.end(bytes);
    }
    if (request.method === 'GET' && operation === 'saved-ctox') {
      const bytes = await readFile(config.ctoxExportPath);
      response.writeHead(200, {
        'content-type': config.mimeType || 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        'content-length': bytes.byteLength,
        'cache-control': 'no-store',
      });
      return response.end(bytes);
    }
    return json(response, 405, { error: 'method_not_allowed' });
  } catch (error) {
    return json(response, 500, { error: error?.message || String(error) });
  }
});

server.listen(port, '0.0.0.0', () => {
  process.stdout.write(`CTOX product Oracle callback listening on http://0.0.0.0:${port}\n`);
});
