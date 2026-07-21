import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

// Bundle the browser module exactly as the shell would load it, then import
// the pure test hooks. This proves the module's logic evaluates without error
// and honours the write-only contract; the server side is covered by the Rust
// guard test ctox_secret_put_keeps_value_out_of_command_record_and_lists_metadata.
const bundled = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});
const [{ text: bundledSource }] = bundled.outputFiles;
const mod = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);
const hooks = mod.__credentialsTestHooks;

const readSource = (rel) => readFile(new URL(rel, import.meta.url), 'utf8');

test('credential key validation matches the server UPPER_SNAKE_CASE rule', () => {
  for (const ok of ['OPENAI_API_KEY', 'A', 'A1_B', 'CTO_IOT_MQTT_PASSWORD']) {
    assert.ok(hooks.KEY_RE.test(ok), `expected valid: ${ok}`);
  }
  for (const bad of ['', 'lower', '1ABC', 'HAS-DASH', 'HAS SPACE', '_LEADING']) {
    assert.ok(!hooks.KEY_RE.test(bad), `expected invalid: ${bad}`);
  }
  assert.ok(!hooks.KEY_RE.test('X'.repeat(65)), 'over 64 chars must be rejected');
});

test('credentialRow renders a pure selector shard and never emits a secret value', () => {
  const setRow = hooks.credentialRow(
    { name: 'OPENAI_API_KEY', description: 'OpenAI API key', is_set: true, updated_at: '2026-06-18T00:00:00Z', source: 'catalog' },
    { view: 'cards', selected: true },
  );
  assert.match(setRow, /OPENAI_API_KEY/);
  assert.match(setRow, /data-context-record-id="OPENAI_API_KEY"/, 'row carries the record id');
  assert.match(setRow, /data-context-record-type="credential"/, 'row carries the record type');
  assert.match(setRow, /data-context-label="/, 'row carries the record label');
  assert.match(setRow, /is-selected/, 'the selected row is marked');
  // A shard is a pure selector: no inline value input, no per-row buttons.
  assert.doesNotMatch(setRow, /value="/, 'no value attribute is ever rendered');
  assert.doesNotMatch(setRow, /type="password"/, 'the shard renders no value input');
  assert.doesNotMatch(setRow, /<button/, 'shards carry no per-row buttons');
  assert.doesNotMatch(setRow, /<details/, 'shards do not expand inline');

  const unsetRow = hooks.credentialRow({ name: 'ANTHROPIC_API_KEY', description: '', is_set: false, source: 'extra' }, { view: 'list' });
  assert.match(unsetRow, /ANTHROPIC_API_KEY/);
  assert.doesNotMatch(unsetRow, /value="/);
});

test('recordDetailHtml shows metadata only and never a secret value', () => {
  const detail = hooks.recordDetailHtml({ name: 'OPENAI_API_KEY', description: 'key', is_set: true, updated_at: '2026-06-18T00:00:00Z', source: 'catalog' });
  assert.match(detail, /OPENAI_API_KEY/);
  assert.match(detail, /data-action="delete"/, 'set credential offers delete');
  assert.match(detail, /data-action="collapse-detail"/, 'detail can be collapsed');
  assert.doesNotMatch(detail, /value="/, 'detail never renders a value attribute');
  assert.doesNotMatch(detail, /type="password"/, 'detail renders no value input');

  const unset = hooks.recordDetailHtml({ name: 'ANTHROPIC_API_KEY', is_set: false, source: 'extra' });
  assert.doesNotMatch(unset, /data-action="delete"/, 'nothing to remove when unset');
});

test('dispatched command targets the credentials module and a secret command type', () => {
  const doc = hooks.buildCommandDoc('ctox.secret.put', { name: 'OPENAI_API_KEY', value: 'x' }, 'cmd_test');
  assert.equal(doc.id, 'cmd_test');
  assert.equal(doc.module, 'credentials');
  assert.equal(doc.command_type, 'ctox.secret.put');
  assert.equal(doc.record_id, 'OPENAI_API_KEY');
  assert.equal(doc.inbound_channel, 'business_os.credentials');
});

test('mergeEntries tags catalog and extra and drops nameless rows', () => {
  const rows = hooks.mergeEntries(
    [{ name: 'A', is_set: true }, { name: '', is_set: false }],
    [{ name: 'B', is_set: false, description: 'custom' }],
  );
  assert.deepEqual(rows.map((r) => [r.name, r.source, r.is_set]), [['A', 'catalog', true], ['B', 'extra', false]]);
});

test('band + counts derive from is_set; filters narrow by band/source/search', () => {
  const rows = hooks.mergeEntries(
    [{ name: 'OPENAI_API_KEY', is_set: true }, { name: 'ANTHROPIC_API_KEY', is_set: false }],
    [{ name: 'CUSTOM_TOKEN', is_set: true, description: 'mine' }],
  );
  assert.equal(hooks.credentialBand({ is_set: true }), 'set');
  assert.equal(hooks.credentialBand({ is_set: false }), 'open');
  assert.deepEqual(hooks.countsFor(rows), { all: 3, set: 2, open: 1 });
  assert.equal(hooks.filterRows(rows, { band: 'set' }).length, 2);
  assert.equal(hooks.filterRows(rows, { band: 'open' }).length, 1);
  assert.equal(hooks.filterRows(rows, { source: 'extra' }).length, 1);
  assert.equal(hooks.filterRows(rows, { search: 'custom' }).length, 1);
  assert.equal(hooks.filterRows(rows, { search: 'api_key' }).length, 2);
});

test('EXPORT is metadata-only — the payload never contains a secret value', () => {
  const payload = hooks.buildExportPayload(
    [
      { name: 'OPENAI_API_KEY', description: 'k', is_set: true, source: 'catalog', updated_at: '2026-06-18T00:00:00Z' },
      { name: 'CUSTOM_TOKEN', description: '', is_set: false, source: 'extra' },
    ],
    1_781_990_000_000,
  );
  const serialized = JSON.stringify(payload);
  assert.doesNotMatch(serialized, /"value"/, 'export payload must not contain a value field');
  assert.match(payload._comment, /WRITE-ONLY/i, 'export header states the write-only contract');
  assert.equal(payload.kind, 'ctox-credentials-metadata');
  assert.equal(payload.credentials.length, 2);
  assert.equal(payload.credentials[0].name, 'OPENAI_API_KEY');
  for (const entry of payload.credentials) {
    assert.ok(!Object.prototype.hasOwnProperty.call(entry, 'value'), 'no credential entry carries a value');
  }
});

test('IMPORT keeps only entries with a valid key AND a value to write', () => {
  const entries = hooks.parseImportEntries({
    credentials: [
      { name: 'OPENAI_API_KEY', value: 'sk-live' },
      { key: 'ALT_KEY', value: 'v2' },
      { name: 'NO_VALUE', is_set: true },          // metadata export row — nothing to write
      { name: 'bad-key', value: 'x' },              // invalid key
      { name: 'OPENAI_API_KEY', value: 'dup' },     // duplicate name
    ],
  });
  assert.deepEqual(entries, [
    { name: 'OPENAI_API_KEY', value: 'sk-live' },
    { name: 'ALT_KEY', value: 'v2' },
  ]);
  // A bare array is accepted too.
  assert.deepEqual(hooks.parseImportEntries([{ name: 'TOKEN', value: 't' }]), [{ name: 'TOKEN', value: 't' }]);
});

test('auto-reveal follows hasSelection && !userCollapsed', () => {
  assert.equal(mod.shouldRevealRecord(true, false), true, 'selected + not collapsed → shown');
  assert.equal(mod.shouldRevealRecord(false, false), false, 'no selection → hidden');
  assert.equal(mod.shouldRevealRecord(true, true), false, 'user collapsed → hidden');
});

test('left column carries the canonical grammar markup pins', async () => {
  const html = await readSource('./index.html');
  assert.match(html, /data-pg-search/, 'grammar search input');
  assert.match(html, /data-pg-view="cards"/, 'shard view toggle');
  assert.match(html, /data-pg-view="list"/, 'list view toggle');
  assert.match(html, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(html, /data-pg-tray\b/, 'collapsed tray');
  assert.match(html, /data-pg-reset/, 'tray reset control');
  assert.match(html, /data-pg-footer/, 'one-line footer target');
  assert.match(html, /data-pg-filter[^>]*data-pg-name="source"/, 'source filter in tray');
  // Counted band with >= 2 real views.
  const bands = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bands.length >= 2, `expected >= 2 view band tabs, got ${bands.length}`);
  const counts = html.match(/data-pg-count="[^"]+"/g) || [];
  assert.ok(counts.length >= 2, 'each band tab exposes a count target');
});

test('header carries the standing Neu / Import / Export icon actions', async () => {
  const html = await readSource('./index.html');
  assert.match(html, /data-action="new"/, 'primary create action');
  assert.match(html, /data-action="import"[^>]*aria-label=/, 'import icon has aria-label');
  assert.match(html, /data-action="export"[^>]*aria-label=/, 'export icon has aria-label');
});

test('the manual refresh button is gone; the list is reactive via a subscription', async () => {
  const html = await readSource('./index.html');
  const indexJs = await readSource('./index.js');
  assert.doesNotMatch(html, /data-action="refresh"/, 'no refresh button in the markup');
  assert.doesNotMatch(indexJs, /=== 'refresh'/, 'no refresh action handler');
  // Reactive: subscribe to business_commands for secret put/delete landings.
  assert.match(indexJs, /command_type:\s*\{\s*\$in:\s*\[PUT_COMMAND,\s*DELETE_COMMAND\]/, 'subscription scoped to put/delete');
  assert.match(indexJs, /\.\$\?\.subscribe/, 'subscribes to the collection query');
});

test('import/export/new/delete/collapse handlers are wired in index.js', async () => {
  const indexJs = await readSource('./index.js');
  assert.match(indexJs, /=== 'import'/, 'import action handled');
  assert.match(indexJs, /=== 'export'/, 'export action handled');
  assert.match(indexJs, /=== 'new'/, 'new action handled');
  assert.match(indexJs, /=== 'delete'/, 'delete action handled');
  assert.match(indexJs, /=== 'collapse-detail'/, 'detail collapse handled');
  // Export = metadata-only JSON download via Blob URL (no HTTP, no value).
  assert.match(indexJs, /buildExportPayload\(/, 'export uses the metadata-only builder');
  assert.match(indexJs, /URL\.createObjectURL/, 'export uses an object URL');
  // Import = file input reading JSON, writing via the existing put command.
  assert.match(indexJs, /type = 'file'/, 'import creates a file input');
  assert.match(indexJs, /parseImportEntries\(/, 'import normalizes via the record helper');
});

test('existing ctox.secret.* command flows are untouched', async () => {
  const indexJs = await readSource('./index.js');
  assert.match(indexJs, /ctox\.secret\.list/, 'list command');
  assert.match(indexJs, /ctox\.secret\.put/, 'put command');
  assert.match(indexJs, /ctox\.secret\.delete/, 'delete command');
  // The value is redacted from the local pending command as before.
  assert.match(indexJs, /redactLocalCommand\(/, 'local command redaction retained');
});

test('selecting a row is an in-place flip, not a list rebuild', async () => {
  const indexJs = await readSource('./index.js');
  // The selection helper flips is-selected across existing rows.
  assert.match(indexJs, /function applyListSelection\(\)/, 'in-place selection helper exists');
  assert.match(indexJs, /classList\.toggle\('is-selected'/, 'selection toggles is-selected');
  // selectRecord must NOT rewrite the list well (a rebuild resets scroll).
  const selectStart = indexJs.indexOf('function selectRecord(');
  const selectEnd = indexJs.indexOf('function startNew(');
  assert.ok(selectStart !== -1 && selectEnd > selectStart, 'selectRecord is present');
  const selectBody = indexJs.slice(selectStart, selectEnd);
  assert.doesNotMatch(selectBody, /innerHTML/, 'selectRecord never rebuilds the list innerHTML');
  assert.match(selectBody, /applyListSelection\(\)/, 'selectRecord flips selection in place');
});

test('mount renders before the command-bus list round-trip completes', async () => {
  const source = await readSource('./index.js');
  const mountSource = source.slice(
    source.indexOf('export async function mount'),
    source.indexOf('function formatUpdated'),
  );
  // Non-blocking bootstrap: synchronous render, then a fire-and-forget refresh.
  assert.match(mountSource, /render\(\);[\s\S]{0,120}void refresh\(\);/, 'render precedes a non-blocking refresh');
});
