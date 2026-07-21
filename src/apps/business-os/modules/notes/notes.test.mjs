import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
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

test('locked notes never persist cleartext passcode or title to the synced store', () => {
  // A locked note whose in-memory object still carries a cleartext title and a
  // passcode must serialize to a payload that leaks neither — the synced store
  // is the threat surface (zero-knowledge claim in index.html).
  const locked = {
    id: 'n-locked',
    title: 'My secret first line',
    is_locked: true,
    lock_passcode: 'hunter2',
    notebook: 'Privat',
    tags: 'a,b',
    is_favorite: true,
    is_trashed: false,
  };
  const payload = hooks.buildNotePersistPayload(locked, t);

  assert.equal(payload.lock_passcode, '');
  assert.notEqual(payload.title, 'My secret first line');
  assert.equal(payload.title, 'Gesperrte Notiz');
  assert.equal(payload.is_locked, true);
  // Non-secret metadata is preserved.
  assert.equal(payload.notebook, 'Privat');
  assert.equal(payload.tags, 'a,b');
  assert.equal(payload.is_favorite, true);
});

test('unlocked notes keep a content-derived title and empty passcode', () => {
  const open = {
    id: 'n-open',
    title: 'Visible title',
    is_locked: false,
    lock_passcode: '',
  };
  const payload = hooks.buildNotePersistPayload(open, t);

  assert.equal(payload.title, 'Visible title');
  assert.equal(payload.lock_passcode, '');
  assert.equal(payload.is_locked, false);

  assert.equal(hooks.deriveTitleFromPlainText('# Heading line\nbody', t), 'Heading line');
  assert.equal(hooks.deriveTitleFromPlainText('   \n  \n', t), 'Unbenannte Notiz');
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

test('notes presentation follows compact Business OS editor contract', async () => {
  const css = await readFile(fileURLToPath(new URL('./index.css', import.meta.url)), 'utf8');
  const html = await readFile(fileURLToPath(new URL('./index.html', import.meta.url)), 'utf8');

  assert.doesNotMatch(html, /ctox-pane--glass/);
  assert.doesNotMatch(css, /Premium Port/);
  assert.doesNotMatch(css, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
  assert.doesNotMatch(css, /box-shadow:\s*inset\s+3px\s+0\s+0/);
  assert.doesNotMatch(css, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
  assert.match(css, /--nn-shadow:\s*none;/);
  assert.match(css, /--nn-paper-shadow:\s*none;/);
  // Frame and columns are the shared kit: workspace grid + declarative,
  // shell-owned column resizers (no module-built resizer).
  assert.match(html, /ctox-workspace[^"]*notes-module/);
  assert.match(html, /data-resize-frame/);
  assert.match(html, /ctox-column-resizer[^>]*data-resizer-var="--ctox-left-width"/);
  // Two-pane IA (grammar column + editor): exactly ONE shell resizer, and no
  // module-built resizer.
  assert.equal((html.match(/ctox-column-resizer/g) || []).length, 1);
  assert.doesNotMatch(html, /data-resizer-var="--ctox-right-width"/);
  assert.doesNotMatch(css, /\.notes-resizer/);
  assert.doesNotMatch(css, /--notes-left-width|--notes-right-width/);
  // List rows and status pills use the kit; the module keeps only content
  // layout for cards.
  assert.match(css, /\.notes-card\s*\{[\s\S]*?display:\s*flex/);
  assert.doesNotMatch(css, /notebook-badge|tag-badge|presence-badge/);
  assert.match(css, /\.nn-paper-sheet\s*\{[\s\S]*?border-radius:\s*var\(--control-radius\)/);
  assert.match(css, /\.nn-pin-container\s*\{[\s\S]*?border-radius:\s*var\(--control-radius\)/);
});

// --- Canonical column grammar / IA (books/tags nav + note list + editor) ---

const notesHtml = await readFile(fileURLToPath(new URL('./index.html', import.meta.url)), 'utf8');
const notesCss = await readFile(fileURLToPath(new URL('./index.css', import.meta.url)), 'utf8');
const notesJs = await readFile(fileURLToPath(new URL('./index.js', import.meta.url)), 'utf8');

test('left column carries the full shell-wired grammar (data-pg-*)', () => {
  // Row 1: header actions collected top-right — Neu + Import + Export are standing.
  assert.match(notesHtml, /data-action="create-note"/);
  assert.match(notesHtml, /data-action="import"/);
  assert.match(notesHtml, /data-action="export"/);
  // Row 2: search + shard/list toggle + collapsed tray with reset.
  assert.match(notesHtml, /class="ctox-filterbar"/);
  assert.match(notesHtml, /data-pg-search/);
  assert.match(notesHtml, /data-pg-view="cards"/);
  assert.match(notesHtml, /data-pg-view="list"/);
  assert.match(notesHtml, /data-pg-tray-toggle/);
  assert.match(notesHtml, /data-pg-tray\b/);
  assert.match(notesHtml, /data-pg-reset/);
  // Book/tag scopes and sort live in the tray as dropdowns (never standing rows).
  assert.match(notesHtml, /data-pg-filter[^>]*data-pg-name="notebook"/);
  assert.match(notesHtml, /data-pg-filter[^>]*data-pg-name="tag"/);
  assert.match(notesHtml, /data-pg-filter[^>]*data-pg-name="sort"/);
  // Recessed well + one-line footer.
  assert.match(notesHtml, /ctox-pane-body ctox-well/);
  assert.match(notesHtml, /class="ctox-pane-footer"[^>]*>\s*<span data-pg-footer>/);
  // Module writes NO chrome CSS: no per-app filterbar/tray/band/well/footer rules.
  assert.doesNotMatch(notesCss, /\.ctox-filterbar\s*\{/);
  assert.doesNotMatch(notesCss, /\.ctox-filter-tray\s*\{/);
  assert.doesNotMatch(notesCss, /\.ctox-view-switch\s*\{/);
  assert.doesNotMatch(notesCss, /\.ctox-pane-tabs\s*\{/);
  assert.doesNotMatch(notesCss, /\.ctox-well\s*\{/);
});

test('the counted band has >= 2 real views with counts (zeros included)', () => {
  const band = notesHtml.match(/class="[^"]*ctox-pane-tabs[^"]*"[\s\S]*?<\/div>/);
  assert.ok(band, 'ctox-pane-tabs band present');
  const tabs = band[0].match(/class="[^"]*ctox-pane-tab[^"]*"/g) || [];
  assert.ok(tabs.length >= 2, `band needs >= 2 tabs, found ${tabs.length}`);
  for (const key of ['notes', 'favorites', 'trash']) {
    assert.match(notesHtml, new RegExp(`data-pg-band="${key}"`));
    assert.match(notesHtml, new RegExp(`data-pg-count="${key}"`));
  }
});

test('band counts include zeros and honour the trashed/favorite predicates', () => {
  const notes = [
    { id: 'a', is_trashed: false, is_favorite: true },
    { id: 'b', is_trashed: false, is_favorite: false },
    { id: 'c', is_trashed: true, is_favorite: false },
  ];
  assert.deepEqual(hooks.bandCountsFor(notes), { notes: 2, favorites: 1, trash: 1 });
  // Zeros are rendered, never hidden.
  assert.deepEqual(hooks.bandCountsFor([]), { notes: 0, favorites: 0, trash: 0 });
});

test('export is honest JSON and never leaks draft or decrypted content', () => {
  const rows = hooks.buildNotesExport([
    { id: 'n1', title: 'Visible', content: '<p>body</p>', notebook: 'Ops', tags: 'x', is_favorite: true, updated_at_ms: 5 },
    { id: 'draft', title: 'Draft', [hooks.DRAFT_NOTE_MARKER]: true },
    { id: 'locked', title: 'Gesperrte Notiz', content: '{"cipherText":"..."}', is_locked: true },
  ]);
  assert.equal(rows.length, 2, 'drafts are excluded from export');
  assert.equal(rows.find(r => r.id === 'draft'), undefined);
  const locked = rows.find(r => r.id === 'locked');
  assert.equal(locked.is_locked, true);
  // Only the stored ciphertext content ships — the export never invents plaintext.
  assert.equal(locked.content, '{"cipherText":"..."}');
});

test('import parses arrays or a single object and never trusts a lock flag', () => {
  const parsed = hooks.parseNotesImport([
    { title: 'One', content: '<p>a</p>', notebook: 'Ops', tags: 'x', is_favorite: true, is_locked: true, lock_passcode: 'hunter2' },
    { title: '   ' },              // dropped: no title and no content
    { notebook: 'only-meta' },     // dropped: no title and no content
    { content: '<p>only content</p>' },
  ]);
  assert.equal(parsed.length, 2);
  assert.equal(parsed[0].title, 'One');
  assert.equal('is_locked' in parsed[0], false, 'imported notes never carry a lock flag');
  assert.equal('lock_passcode' in parsed[0], false);
  // A single object is accepted too.
  assert.equal(hooks.parseNotesImport({ title: 'solo' }).length, 1);
  assert.deepEqual(hooks.parseNotesImport('nonsense'), []);
});

test('selecting a note is an in-place flip, never a list rebuild', () => {
  const selectNote = notesJs.match(/function selectNote\(id\)\s*\{[\s\S]*?\n\}/);
  assert.ok(selectNote, 'selectNote present');
  const body = selectNote[0];
  // Flip + editor only — no list innerHTML rebuild and no full scheduleRender.
  assert.match(body, /applyListSelection\(\)/);
  assert.match(body, /renderEditor\(\)/);
  assert.doesNotMatch(body, /renderNotesList\(\)/);
  assert.doesNotMatch(body, /scheduleRender\(\)/);
  // applyListSelection toggles the class across existing rows in place.
  const flip = notesJs.match(/function applyListSelection\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(flip, 'applyListSelection present');
  assert.match(flip[0], /classList\.toggle\('is-selected'/);
  assert.doesNotMatch(flip[0], /innerHTML/);
});

test('auto-reveal: editor visible only with a selection that is not collapsed', () => {
  assert.equal(hooks.computeEditorVisible({ hasSelection: true, userCollapsed: false }), true);
  assert.equal(hooks.computeEditorVisible({ hasSelection: false, userCollapsed: false }), false);
  assert.equal(hooks.computeEditorVisible({ hasSelection: true, userCollapsed: true }), false);
});
