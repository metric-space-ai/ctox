import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

import { build } from 'esbuild';

const explorerSource = await readFile(new URL('./app.js', import.meta.url), 'utf8');
const shellSource = await readFile(new URL('../../app.js', import.meta.url), 'utf8');

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./app.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __explorerTestHooks: explorer } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

describe('Explorer app helpers', () => {
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

  it('keeps the CTOX-published file area visible at the Files root', async () => {
    const upserts = [];
    const db = {
      collection(name) {
        if (name !== 'desktop_files') return null;
        return {
          findOne: () => ({ exec: async () => null }),
          upsert: async (doc) => { upserts.push(doc); },
        };
      },
    };

    await explorer.ensureFileSystem(db);

    assert.ok(upserts.some((doc) => (
      doc.id === 'fs_ctox'
      && doc.parent_id === 'fs_root'
      && doc.path === '/CTOX'
      && doc.name === 'CTOX'
    )));
  });

  it('waits for desktop file replication before seeding and rendering Files', () => {
    const replicationStart = explorerSource.indexOf("startCollection?.('desktop_files')");
    const seedStart = explorerSource.indexOf('await ensureFileSystem(ctx.db)');
    const renderStart = explorerSource.indexOf('await selectSource(FILE_SOURCE)');

    assert.ok(replicationStart >= 0, 'Files must explicitly start desktop_files replication');
    assert.ok(replicationStart < seedStart, 'native file metadata must arrive before browser seeds');
    assert.ok(seedStart < renderStart, 'the populated collection must be rendered last');
  });

  it('replicates each selected Business OS source through the scoped Files facade', () => {
    assert.match(explorerSource, /const collectionId = sourceCollectionId\(state\.activeSource\)[\s\S]*?startCollection\?\.\(collectionId\)/);
    const scope = shellSource.match(/explorer: \[([\s\S]*?)\],/);
    assert.ok(scope, 'Files must have an explicit packaged-system data scope');
    for (const collection of ['documents', 'spreadsheets', 'knowledge_items', 'matching_objects', 'outbound_companies']) {
      assert.match(scope[1], new RegExp(`'${collection}'`));
    }
  });

  it('offers discoverable recent-file views and explicit sort choices', () => {
    assert.match(explorerSource, /label: 'Zuletzt erstellt'[\s\S]*?recentSort: 'created'/);
    assert.match(explorerSource, /label: 'Zuletzt geändert'[\s\S]*?recentSort: 'modified'/);
    assert.match(explorerSource, /data-explorer-sort aria-label="Sortieren"/);
    assert.match(explorerSource, /<option value="created">Erstellt: neueste<\/option>/);
    assert.match(explorerSource, /activeData\.filter\(\(item\) => item\.kind !== 'folder'\)\.map\(normalizeFileRow\)/);
  });

  it('registers installed source schemas before mounting Files', () => {
    const schemaModules = shellSource.match(/DESKTOP_APP_SCHEMA_MODULE_IDS = Object\.freeze\(\{[\s\S]*?explorer: Object\.freeze\(\[([\s\S]*?)\]\),/);
    assert.ok(schemaModules, 'Files must declare the module schemas used by its source providers');
    for (const moduleId of ['documents', 'spreadsheets', 'knowledge', 'matching', 'outbound']) {
      assert.match(schemaModules[1], new RegExp(`'${moduleId}'`));
    }
    assert.match(shellSource, /schemaModuleIds\.map\(async \(moduleId\) => \{[\s\S]*?registerModuleSchemas\(schemaModule\)/);
  });

  it('ensures a source module schema again when its Files provider is selected', () => {
    assert.match(explorerSource, /if \(source\.moduleId\) await ctx\.ensureModuleData\?\.\(source\.moduleId\)/);
    assert.match(shellSource, /ensureModuleData: async \(moduleId\) => \{[\s\S]*?registerModuleSchemas\(sourceModule\)/);
  });

  it('gives the packaged Spreadsheets app only its declared system collections', () => {
    const scope = shellSource.match(/spreadsheets: Object\.freeze\(\[([\s\S]*?)\]\),/);
    assert.ok(scope, 'Spreadsheets must have an explicit packaged-system data scope');
    for (const collection of ['spreadsheets', 'spreadsheet_versions', 'spreadsheet_blob_chunks', 'spreadsheet_runbooks']) {
      assert.match(scope[1], new RegExp(`'${collection}'`));
    }
    assert.doesNotMatch(scope[1], /desktop_files|documents|knowledge_items/);
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
    assert.match(bundledSource, /\.app-explorer-grid \{[\s\S]*min-width: 0;/);
    assert.match(bundledSource, /\.app-explorer-row \{[\s\S]*min-width: 0;/);
  });

  it('offers a direct Files download action independent of the file viewer', () => {
    assert.match(bundledSource, /data-preview-download/);
    assert.match(bundledSource, /anchor\.download\s*=\s*row\.label/);
    assert.match(bundledSource, /Herunterladen/);
    assert.match(bundledSource, /Download fehlgeschlagen/);
    assert.match(bundledSource, /reportFileIntegrityError/);
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
  });

  it('delivers file-open intents to an already running module window', () => {
    assert.match(shellSource, /options\.args\?\.openFile[\s\S]*?'desktop-app:open-file'/);
    assert.match(explorerSource, /associatedAppFor\(row\)[\s\S]*?openDesktopApp\(associatedApp/);
  });

  it('stores uploaded file chunks in one bulk write without DataURL materialization', async () => {
    assert.doesNotMatch(bundledSource, /readAsDataURL/);
    const chunkWrites = [];
    const fileWrites = [];
    const db = {
      collection(name) {
        if (name === 'desktop_file_chunks') {
          return {
            bulkUpsert: async (docs) => { chunkWrites.push(docs); },
            upsert: async () => { throw new Error('desktop_file_chunks upsert must not run per chunk'); },
          };
        }
        if (name === 'desktop_files') {
          return {
            upsert: async (doc) => { fileWrites.push(doc); },
          };
        }
        return null;
      },
    };
    const bytes = new Uint8Array(24 * 1024);
    bytes.fill(65);
    await explorer.storeFile(db, 'fs_root', '/', 'bulk.txt', new File([bytes], 'bulk.txt', { type: 'text/plain' }));

    assert.equal(chunkWrites.length, 1, 'chunks are written through one bulkUpsert call');
    assert.ok(chunkWrites[0].length > 1, 'test payload spans multiple chunk documents');
    assert.equal(fileWrites.length, 1, 'file metadata is written once');
  });
});
