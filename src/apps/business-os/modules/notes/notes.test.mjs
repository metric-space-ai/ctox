import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __notesTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

const t = (_key, fallback) => fallback;

test('empty state distinguishes sync diagnostics from real empty lists', () => {
  const diagnostic = hooks.buildNotesEmptyState({
    totalNotes: 0,
    scopedNotes: 0,
    hasSearch: false,
    activeLabel: 'Alle Notizen',
    diagnostics: { kind: 'missing', message: 'notes collection missing' },
    t,
  });

  assert.equal(diagnostic.kind, 'unavailable');
  assert.doesNotMatch(diagnostic.body, /missing|collection/i);

  const empty = hooks.buildNotesEmptyState({
    totalNotes: 0,
    scopedNotes: 0,
    hasSearch: false,
    activeLabel: 'Alle Notizen',
    diagnostics: { kind: 'ok-empty', message: '' },
    t,
  });

  assert.equal(empty.kind, 'empty');
});

test('search misses are not rendered as missing notes', () => {
  const state = hooks.buildNotesEmptyState({
    totalNotes: 3,
    scopedNotes: 3,
    hasSearch: true,
    activeLabel: 'Alle Notizen',
    diagnostics: { kind: 'ok', message: '' },
    t,
  });

  assert.equal(state.kind, 'no-results');
});

test('default notes provide notebooks, tags, and favorites for first render', () => {
  const notes = hooks.createDefaultNotes(1000);

  assert.equal(notes.length >= 3, true);
  assert.equal(notes.some(note => note.is_favorite), true);
  assert.equal(notes.every(note => note.notebook && note.tags), true);
});

test('draft and destructive note actions expose safe modes', () => {
  const draft = { id: 'draft-1', [hooks.DRAFT_NOTE_MARKER]: true };
  assert.deepEqual(hooks.noteActionAvailability(draft), {
    canDelete: true,
    canFavorite: false,
    canLock: false,
    deleteMode: 'discard-draft',
    lockMode: 'save-draft-first',
  });

  const note = { id: 'note-1', is_trashed: false, is_locked: false };
  assert.equal(hooks.noteActionAvailability(note).deleteMode, 'confirm-trash-with-undo');
  assert.equal(hooks.noteActionAvailability({ ...note, is_trashed: true }).deleteMode, 'confirm-permanent-delete');
});
