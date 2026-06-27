import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

import { build } from 'esbuild';

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
