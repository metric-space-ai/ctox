import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const {
  buildAppCreateCommand,
  CREATOR_PROMPT_EXAMPLES,
  computeCreatorActionState,
  deriveModuleIdFromRequest,
  titleFromModuleId,
  normalizeCreatorInstalledApps,
  normalizeCreatorRequestSuggestions,
  normalizeCollectionName,
  normalizeInspirationUrl,
  normalizeModuleId,
  validateCreatorSpec,
  creatorLibraryCounts,
  filterCreatorLibrary,
  creatorRequestStatuses,
  prepareCreatorRequestImport,
} = await import('./index.js');

const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
const presentationSource = `${css}\n${html}`;
const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

test('empty request blocks app creation', () => {
  const state = computeCreatorActionState({
    request: '',
    appId: '',
    appTitle: '',
    appDesc: '',
    appCollections: [],
  });

  assert.equal(state.deployReady, false);
  assert.match(state.diagnostic, /Auftrag fehlt/);
});

test('plain request enables CTOX app creation without a local planning step', () => {
  const request = 'Erstelle eine App fuer Support Tickets';
  const state = computeCreatorActionState({
    request,
    appId: '',
    appTitle: '',
    appDesc: '',
    appCollections: [],
  });

  assert.equal(state.deployReady, true);
  assert.equal(state.validationErrors.length, 0);
});

test('manual advanced edits only add optional hints and still surface validation errors', () => {
  const request = 'Erstelle eine Support Desk App';
  const state = computeCreatorActionState({
    request,
    appId: 'supportdesk',
    appTitle: 'x'.repeat(121),
    appDesc: '',
    appCollections: [],
  });

  assert.equal(state.deployReady, false);
  assert.equal(state.validationErrors[0], 'Titel ist zu lang.');
  assert.match(state.diagnostic, /Titel ist zu lang/);
});

test('request metadata derivation only creates a neutral module id and title', () => {
  const moduleId = deriveModuleIdFromRequest('Eine CRM App fuer Kunden und Kontakte', 1234);

  assert.equal(moduleId, 'eine-crm-app-fuer-kunden');
  assert.equal(titleFromModuleId(moduleId), 'Eine Crm App Fuer Kunden');
});

test('slug and collection normalization reject unsafe names', () => {
  assert.equal(normalizeModuleId('  Meine App!! '), 'meine-app');
  assert.equal(normalizeCollectionName(' Neue Collection!! '), 'neue_collection');
  assert.deepEqual(
    validateCreatorSpec({
      appId: '',
      appTitle: '',
      appDesc: '',
      appCollections: [],
    }),
    [],
  );
});

test('creator offers a broad set of editable app prompt examples', () => {
  assert.ok(CREATOR_PROMPT_EXAMPLES.length >= 10);
  assert.equal(new Set(CREATOR_PROMPT_EXAMPLES.map((item) => item.id)).size, CREATOR_PROMPT_EXAMPLES.length);
  for (const example of CREATOR_PROMPT_EXAMPLES) {
    assert.ok(example.de.title);
    assert.ok(example.de.prompt.length > 100);
    assert.ok(example.en.title);
    assert.ok(example.en.prompt.length > 100);
  }
});

test('inspiration URLs accept only normalized web references', () => {
  assert.equal(normalizeInspirationUrl('https://linear.app/#features'), 'https://linear.app/');
  assert.equal(normalizeInspirationUrl('http://example.com/path'), 'http://example.com/path');
  assert.equal(normalizeInspirationUrl('javascript:alert(1)'), '');
  assert.equal(normalizeInspirationUrl('linear.app'), '');
});

test('creator right rail only treats installed modules as custom apps', () => {
  const apps = normalizeCreatorInstalledApps({
    modules: [
      { id: 'creator', title: 'App Creator', entry: 'modules/creator/index.html', source: 'core' },
      { id: 'notes', title: 'Notizen', entry: 'modules/notes/index.html', source: 'starter' },
      { id: 'crm-kontakte', title: 'CRM Kontakte', entry: 'installed-modules/crm-kontakte/index.html', source: 'installed', version: 'v2' },
    ],
  });

  assert.deepEqual(apps.map((app) => app.id), ['crm-kontakte']);
  assert.equal(apps[0].version, 'v2');
});

test('creator request suggestions are sorted and trimmed to actionable requests', () => {
  const requests = normalizeCreatorRequestSuggestions([
    { id: 'ignore', module: 'notes', command_type: 'notes.create', payload: { instruction: 'Keine Creator App' }, updated_at_ms: 10 },
    { id: 'old', module: 'creator', command_type: 'business_os.chat.task', payload: { instruction: 'Alte App', title: 'Alt' }, status: 'done', updated_at_ms: 20 },
    { id: 'new', command_type: 'ctox.business_os.app.modify', payload: { instruction: 'Neue App bauen', title: 'Neu' }, status: 'pending', updated_at_ms: 30 },
  ]);

  assert.deepEqual(requests.map((item) => item.id), ['new', 'old']);
  assert.equal(requests[0].request, 'Neue App bauen');
});

test('creator builds an app create command instead of writing module source directly', () => {
  const command = buildAppCreateCommand({
    appId: 'Meine Inventar App',
    appTitle: 'Inventar',
    appDesc: 'Bestand verwalten',
    appCategory: 'Management',
    appArchetype: 'queue-workflow',
    appLayout: 'windowed',
    appCollections: ['items', 'stock events'],
    appVersion: '0.2.0',
    inspirationUrls: ['https://linear.app/#product', 'https://linear.app/'],
    instruction: 'Baue eine Inventar App.',
    actor: { id: 'admin' },
    now: 1234,
  });

  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.record_id, 'meine-inventar-app');
  assert.equal(command.payload.module_id, 'meine-inventar-app');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
  assert.equal(command.payload.target, 'app');
  assert.deepEqual(command.payload.required_skills, ['business-os-app-module-development']);
  assert.deepEqual(command.payload.collections_hint, ['items', 'stock_events']);
  assert.deepEqual(command.payload.inspiration_urls, ['https://linear.app/']);
  assert.equal(command.payload.layout_hint, 'windowed');
  assert.equal(command.payload.archetype, 'queue-workflow');
  assert.equal(command.payload.presentation.default_mode, 'window');
  assert.deepEqual(command.payload.presentation.supported_modes, ['window', 'maximized', 'focus']);
  assert.deepEqual(command.payload.presentation.minimum_size, { width: 640, height: 480 });
  assert.equal(command.client_context.source, 'business-os-creator');
  assert.equal(command.client_context.install_target, 'runtime-installed-module');
  assert.equal(command.client_context.archetype, 'queue-workflow');
  assert.deepEqual(command.client_context.inspiration_urls, ['https://linear.app/']);
});

test('creator command keeps app structure agent-led when only a request is provided', () => {
  const command = buildAppCreateCommand({
    instruction: 'Baue eine Vertragsverwaltung mit Fristen und CTOX Follow-up.',
    now: 1234,
  });

  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.record_id, 'baue-eine-vertragsverwaltung-mit-fristen');
  assert.equal(command.payload.module_id, 'baue-eine-vertragsverwaltung-mit-fristen');
  assert.equal(command.payload.instruction, 'Baue eine Vertragsverwaltung mit Fristen und CTOX Follow-up.');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
  assert.deepEqual(command.payload.collections_hint, []);
  assert.equal(command.payload.layout_hint, '');
  assert.equal(command.payload.archetype, 'record-workbench');
  assert.deepEqual(command.payload.required_skills, ['business-os-app-module-development']);
});

test('presentation layer stays compact and shell-native', () => {
  assert.doesNotMatch(presentationSource, forbiddenSurfacePattern);
  assert.doesNotMatch(presentationSource, /backdrop-filter/);
  assert.doesNotMatch(presentationSource, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(presentationSource, /border-radius:\s*(?:8|10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(presentationSource, /box-shadow:\s*(?:0|inset|rgba|color-mix|var\(--panel-shadow\))/);
  // Frame is now the shared kit workspace with the shell-wired declarative
  // column resizer (was a module-owned grid + custom resizer before the
  // standard-class migration).
  assert.match(html, /class="ctox-workspace[^"]*"[^>]*data-resize-frame/);
  assert.match(html, /class="ctox-column-resizer"[^>]*data-resizer-var="--ctox-left-width"/);
  assert.match(css, /@container business-app-window \(max-width: 760px\)/);
  assert.match(css, /@container business-app-window \(max-width: 520px\)/);
  assert.match(html, /data-example-prompts/);
  assert.match(html, /id="creator-inspiration-url"/);
});

// --- IA-Karte: apps + requests selector column on the shell chrome block ------

test('left column carries the shell-wired canonical grammar pins', () => {
  // Search + shard/list toggle + collapsed filter tray with a status filter and
  // reset — all shell-wired via data-pg-*, no module chrome JS/CSS.
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-pg-view="list"/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray\b/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /data-pg-filter[^>]*data-pg-name="status"/);
  assert.match(html, /data-pg-footer/);
  assert.match(html, /data-creator-list/);
  // Left pane is annotated as a side pane so the agent learns the column.
  assert.match(html, /class="ctox-pane creator-library"[^>]*data-left-content/);
});

test('view band has exactly the two real views Apps + Aufträge, counted', () => {
  const band = html.match(/<div class="ctox-pane-tabs">([\s\S]*?)<\/div>/);
  assert.ok(band, 'ctox-pane-tabs band exists');
  const tabs = band[1].match(/class="[^"]*ctox-pane-tab[^"]*"/g) || [];
  assert.equal(tabs.length, 2, 'a band needs >= 2 real views (Apps / Aufträge)');
  assert.match(band[1], /data-pg-band="apps"[^>]*aria-selected="true"/);
  assert.match(band[1], /data-pg-band="auftraege"/);
  assert.match(band[1], /data-pg-count="apps"/);
  assert.match(band[1], /data-pg-count="auftraege"/);
});

test('left column header collects import and export icon actions (no refresh)', () => {
  assert.match(html, /data-action="import"/);
  assert.match(html, /data-action="export"/);
  // Reactive-only: no manual refresh/reload button in the pane.
  assert.doesNotMatch(html, /data-action="(?:refresh|reload)"/);
});

test('creatorLibraryCounts counts both views, zeros included', () => {
  assert.deepEqual(creatorLibraryCounts([], []), { apps: 0, auftraege: 0 });
  assert.deepEqual(
    creatorLibraryCounts([{ id: 'a' }, { id: 'b' }], [{ id: 'r' }]),
    { apps: 2, auftraege: 1 },
  );
});

test('filterCreatorLibrary segments Apps/Aufträge and applies search + status', () => {
  const apps = [
    { id: 'crm', title: 'CRM', category: 'Management', version: '1.0.0' },
    { id: 'lager', title: 'Lager', category: 'Utilities', version: '0.2.0' },
  ];
  const requests = [
    { id: 'r1', title: 'Support', request: 'Support desk bauen', status: 'pending' },
    { id: 'r2', title: 'Invoice', request: 'Rechnungen', status: 'completed' },
  ];

  // Apps band, default grammar → both apps, tagged kind app.
  const appItems = filterCreatorLibrary({ apps, requests }, { band: 'apps' });
  assert.deepEqual(appItems.map((i) => i.id), ['crm', 'lager']);
  assert.ok(appItems.every((i) => i.kind === 'app'));

  // Apps band search filters app fields; status is ignored for apps.
  assert.deepEqual(
    filterCreatorLibrary({ apps, requests }, { band: 'apps', search: 'lager', status: 'completed' }).map((i) => i.id),
    ['lager'],
  );

  // Aufträge band, status filter constrains requests.
  const reqItems = filterCreatorLibrary({ apps, requests }, { band: 'auftraege', status: 'completed' });
  assert.deepEqual(reqItems.map((i) => i.id), ['r2']);
  assert.ok(reqItems.every((i) => i.kind === 'request'));

  // Aufträge band search over title/request.
  assert.deepEqual(
    filterCreatorLibrary({ apps, requests }, { band: 'auftraege', search: 'desk' }).map((i) => i.id),
    ['r1'],
  );
});

test('creatorRequestStatuses lists only distinct statuses that exist', () => {
  assert.deepEqual(
    creatorRequestStatuses([{ status: 'pending' }, { status: 'pending' }, { status: 'completed' }, {}]),
    ['pending', 'completed'],
  );
});

test('prepareCreatorRequestImport builds a local draft or rejects empty input', () => {
  const draft = prepareCreatorRequestImport({ title: 'Aus Datei', request: 'Baue eine App' }, 3);
  assert.equal(draft.request, 'Baue eine App');
  assert.equal(draft.status, 'lokal');
  assert.equal(draft.imported, true);
  assert.equal(prepareCreatorRequestImport({ title: 'Leer' }, 0), null);
  // Falls back to instruction/prompt keys and a synthetic id.
  assert.equal(prepareCreatorRequestImport({ instruction: 'x'.repeat(3) }, 5).id, 'import-5');
});

test('import/export handlers are honest and small (Blob download + file input)', () => {
  assert.match(js, /=== 'import'/);
  assert.match(js, /=== 'export'/);
  assert.match(js, /URL\.createObjectURL/);
  assert.match(js, /input\.type = 'file'/);
});

test('selecting a shard is an in-place flip, never a list rebuild', () => {
  // Selection toggles is-selected / aria-selected across existing rows and does
  // NOT call renderLibrary (which would rebuild the well and reset scroll).
  assert.match(js, /function applyLibrarySelection/);
  assert.match(js, /classList\.toggle\('is-selected'/);
  const selectBody = js.match(/function selectLibraryItem\([\s\S]*?\n}/);
  assert.ok(selectBody, 'selectLibraryItem exists');
  assert.match(selectBody[0], /applyLibrarySelection/);
  assert.doesNotMatch(selectBody[0], /renderLibrary/);
});

test('composer flow is preserved: app select prefills upgrade, request adopts text', () => {
  assert.match(js, /prefillUpgrade/);
  assert.match(js, /adoptRequest/);
  // The signature composer surface (request textarea + advanced options
  // collapsed by default + deploy footer) is untouched.
  assert.match(html, /creator-request-textarea/);
  assert.match(html, /<details class="creator-options">/);
  assert.doesNotMatch(html, /<details class="creator-options" open>/);
  assert.match(html, /id="btn-deploy-app"/);
});
