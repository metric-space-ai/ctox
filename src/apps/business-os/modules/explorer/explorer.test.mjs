import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { describe, it } from 'node:test';

const explorerUrl = new URL('./index.js', import.meta.url);
const explorerSource = await readFile(explorerUrl, 'utf8');
const explorerCss = await readFile(new URL('./index.css', import.meta.url), 'utf8');
const explorerHtml = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
const collectionSchemas = JSON.parse(await readFile(new URL('./collections.schema.json', import.meta.url), 'utf8'));
const bundledSource = explorerSource;
const { __explorerTestHooks: explorer } = await import(explorerUrl);

describe('Explorer module contract', () => {
  it('uses the Business OS module entry contract and a fragment-only view', () => {
    // App-API 4.0 (V4-PORT.md): mount(root, api, {restored}) → {unmount, snapshot}, kein ctx.
    assert.match(explorerSource, /export async function mount\(root, api, \{ restored \} = \{\}\)/);
    assert.doesNotMatch(explorerSource, /\bctx\.|desktop_file_chunks|demandFileLoader|\?v=/);
    assert.doesNotMatch(explorerSource, /export const manifest/);
    assert.doesNotMatch(explorerSource, /desktop-apps\//);
    // Single-Column-Workspace: ohne --single erbt die Root die 5-Spuren-
    // Spalten aus base.css (.ctox-workspace) und die drei gestapelten Zeilen
    // kollidieren mit dem Spaltenmodell (Befund 31.08.2026).
    assert.match(explorerHtml, /^<main class="ctox-workspace ctox-workspace--single app-explorer"/);
    assert.doesNotMatch(explorerHtml, /<!doctype|<(?:html|head|script|style)\b/i);
  });

  it('declares the full core system-module manifest and source collections', () => {
    assert.equal(manifest.id, 'explorer');
    assert.equal(manifest.title, 'Explorer');
    assert.equal(manifest.install_scope, 'core');
    assert.equal(manifest.default_installed, true);
    assert.equal(manifest.launch_kind, 'desktop-app');
    assert.equal(manifest.layout.shell_contract, 'v2');
    assert.equal(manifest.layout.shell_geometry_contract, 'business-os-v2-global-1');
    assert.equal(manifest.store.source_path, 'modules/explorer');
    assert.deepEqual(manifest.collections, [
      'desktop_file_chunks', 'desktop_files', 'documents', 'knowledge_items',
      'matching_objects', 'outbound_companies', 'spreadsheets',
    ]);
    assert.deepEqual(Object.keys(collectionSchemas.collections), manifest.collections);
  });

  it('uses only shell-sanctioned container breakpoints', () => {
    assert.match(explorerCss, /@container business-app-window \(max-width: 1024px\)/);
    assert.match(explorerCss, /@container business-app-window \(max-width: 768px\)/);
    assert.doesNotMatch(explorerCss, /@media\s*\([^)]*(?:max|min)-width/);
  });
});

describe('Explorer helpers', () => {
  it('keeps existing records visible unless they are explicitly deleted', () => {
    const rows = explorer.normalizeRowsForSource([
      { id: 'doc-1', title: 'Proposal', updated_at_ms: 1000 },
      { id: 'doc-2', title: 'Deleted', is_deleted: true, updated_at_ms: 2000 },
      { id: 'doc-3', title: 'Legacy', is_deleted: undefined, updated_at_ms: 3000 },
    ], explorer.SOURCES.find((source) => source.id === 'documents'));

    assert.deepEqual(rows.map((row) => row.label), ['Proposal', 'Legacy']);
  });

  it('filters filesystem rows by the current folder and preserves folders', () => {
    const rows = explorer.normalizeRowsForSource([
      { id: 'fs_root', parent_id: '', path: '/', name: 'Files', kind: 'folder' },
      { id: 'folder-a', parent_id: 'fs_root', path: '/A', name: 'A', kind: 'folder' },
      { id: 'file-a', parent_id: 'folder-a', path: '/A/a.txt', name: 'a.txt', kind: 'file', size_bytes: 12 },
      { id: 'file-root', parent_id: 'fs_root', path: '/root.txt', name: 'root.txt', kind: 'file', size_bytes: 20 },
    ], explorer.FILE_SOURCE, 'fs_root');

    assert.deepEqual(rows.map((row) => [row.label, row.isFolder]), [
      ['A', true],
      ['root.txt', false],
    ]);
  });

  it('keeps the CTOX-published file area visible at the Files root', () => {
    const puts = [];
    const api = { put: (collection, doc) => { puts.push({ collection, doc }); return {}; }, patch: () => ({}) };
    explorer.ensureFileSystem(api, []);
    assert.ok(puts.some(({ collection, doc }) => (
      collection === 'desktop_files'
      && doc.id === 'fs_ctox'
      && doc.parent_id === 'fs_root'
      && doc.path === '/CTOX'
      && doc.name === 'CTOX'
    )));
  });

  it('seeds system folders from the live desktop_files emission instead of waiting for replication', () => {
    assert.doesNotMatch(explorerSource, /startCollection|awaitInSync|awaitInitialReplication/);
    assert.match(explorerSource, /collectionId === 'desktop_files' && !state\.query && !state\.seeded/);
  });

  it('reads every selected source through a live api.query on its declared collection', () => {
    assert.match(explorerSource, /const collectionId = sourceCollectionId\(state\.activeSource\);[\s\S]*?api\.query\(collectionId, activeDocumentQueryForSource/);
    for (const collection of ['documents', 'spreadsheets', 'knowledge_items', 'matching_objects', 'outbound_companies']) {
      assert.ok(manifest.collections.includes(collection));
      assert.ok(collectionSchemas.collections[collection]);
    }
  });

  it('offers discoverable recent-file views and explicit sort choices', () => {
    assert.match(explorerSource, /get label\(\) \{ return T\.recentCreated; \}[\s\S]*?recentSort: 'created'/);
    assert.match(explorerSource, /get label\(\) \{ return T\.recentModified; \}[\s\S]*?recentSort: 'modified'/);
    assert.match(explorerHtml, /data-explorer-sort/);
    assert.match(explorerHtml, /<option value="created"/);
    assert.match(explorerSource, /activeData\.filter\(\(item\) => item\.kind !== 'folder'\)\.map\(normalizeFileRow\)/);
    const q = explorer.activeDocumentQueryForSource(explorer.FILE_SOURCE, 'Rep');
    assert.deepEqual(q.selector.$or[0], { name: { $regex: 'Rep', $options: 'i' } });
    assert.equal(q.limit, 250);
    assert.deepEqual(explorer.activeDocumentQueryForSource(explorer.FILE_SOURCE, ''), {});
  });

  it('validates new folder and rename inputs before persistence', () => {
    const existing = new Set(['reports']);

    assert.equal(explorer.validateEntryName('', existing), 'Name ist erforderlich.');
    assert.equal(explorer.validateEntryName('../x', existing), 'Name darf keine Schrägstriche enthalten.');
    assert.equal(explorer.validateEntryName('reports', existing), 'Name existiert bereits in diesem Ordner.');
    assert.equal(explorer.validateEntryName('Quarterly Reports', existing), '');
  });

  it('creates deterministic unique names for uploads', () => {
    assert.equal(explorer.uniqueName('Report.pdf', ['Report.pdf']), 'Report 2.pdf');
    assert.equal(explorer.uniqueName('Report.pdf', ['Report.pdf', 'Report 2.pdf']), 'Report 3.pdf');
  });

  it('keeps grid rows inside the visible explorer main column', () => {
    assert.match(explorerCss, /\.app-explorer-grid \{[\s\S]*?min-width: 0;/);
    assert.match(explorerCss, /\.app-explorer-row \{[\s\S]*?min-width: 0;/);
  });

  it('offers a direct Files download action independent of the file viewer', () => {
    assert.match(bundledSource, /data-preview-download/);
    assert.match(bundledSource, /anchor\.download\s*=\s*row\.label/);
    assert.match(bundledSource, /Herunterladen/);
    assert.match(bundledSource, /Download fehlgeschlagen/);
    assert.match(bundledSource, /report\('download'/);
  });

  it('supports bidirectional desktop file drag without an HTTP data path', () => {
    assert.equal(explorer.dataTransferContainsFiles({ files: [{ name: 'input.csv' }], types: [] }), true);
    assert.equal(explorer.dataTransferContainsFiles({ files: [], types: ['Files'] }), true);
    assert.equal(explorer.dataTransferContainsFiles({ files: [], types: ['text/plain'] }), false);
    assert.equal(explorer.safeDownloadName('../unsafe:report?.csv'), '_unsafe_report_.csv');
    assert.match(explorerSource, /state\.activeSource\.recentSort/);
    assert.match(explorerSource, /setData\('DownloadURL'/);
    assert.match(explorerSource, /ctoxBusinessOsDesktop[\s\S]*?startFileDrag/);
    assert.doesNotMatch(bundledSource, /fetch\([^)]*desktop_files/);
  });

  it('routes office files to their editing apps and keeps media in the viewer', () => {
    assert.equal(explorer.associatedAppFor({ label: 'loads.csv', mimeType: 'text/plain' }), 'spreadsheets');
    assert.equal(explorer.associatedAppFor({ label: 'loads.xlsx', mimeType: 'application/octet-stream' }), 'spreadsheets');
    assert.equal(explorer.associatedAppFor({ label: 'report.docx', mimeType: 'application/octet-stream' }), 'documents');
    assert.equal(explorer.associatedAppFor({ label: 'notes.txt', mimeType: 'text/plain' }), 'documents');
    assert.equal(explorer.associatedAppFor({ label: 'manual.pdf', mimeType: 'application/pdf' }), '');
    assert.equal(explorer.normalizedMimeType({ label: 'loads.csv', mimeType: 'text/plain' }), 'text/csv');
    assert.equal(explorer.mimeFromName('report.docx'), 'application/vnd.openxmlformats-officedocument.wordprocessingml.document');
    assert.match(explorerSource, /associatedAppFor\(row\)[\s\S]*?api\.shell\.navigate\(associatedApp/);
  });

  it('stores uploads through api.files.put and links the ref in desktop_files without DataURL', async () => {
    assert.doesNotMatch(bundledSource, /readAsDataURL/);
    const uploads = [];
    const writes = [];
    const api = {
      files: { put: (blob, meta) => { uploads.push(meta); return { ref: Promise.resolve('a'.repeat(64)), local: Promise.resolve() }; } },
      put: (collection, doc) => { writes.push({ collection, doc }); return { local: Promise.resolve() }; },
    };
    const bytes = new Uint8Array(24 * 1024).fill(65);
    const file = new File([bytes], 'bulk.txt', { type: 'text/plain' });
    const res = await explorer.storeFile(api, 'fs_root', '/', 'bulk.txt', file);
    assert.equal(uploads.length, 1, 'one blob upload');
    assert.equal(uploads[0].id, res.id);
    assert.equal(writes.length, 1, 'file metadata is written once');
    assert.equal(writes[0].collection, 'desktop_files');
    assert.equal(writes[0].doc.content_ref, 'a'.repeat(64));
    assert.equal(writes[0].doc.content_hash_scheme, 'sha256-bytes-v1');
  });
});
