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
});
