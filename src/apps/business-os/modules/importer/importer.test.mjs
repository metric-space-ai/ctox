import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie PLAYWRIGHT_MODULE in den v4-api-Tests). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');

// v4: index.js importiert 'ctox:shared/…' (nur über den Service Worker auflösbar) und Locales als JSON-Module →
// esbuild-Bündel mit demselben Umschreiben wie der SW (§16.15).
const bundled = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  logLevel: 'silent',
  plugins: [{
    name: 'ctox-shared',
    setup(b) {
      b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
    },
  }],
});
const {
  appIdentityFromSource,
  buildAppImportCommand,
  isImportableFile,
  importFileByteLimit,
  isTextFile,
  moduleIdFromSource,
  normalizeImportCategory,
  parseGitHubUrl,
  shouldSkipPath,
  standaloneHtmlEntryPath,
  validModuleId,
} = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);

test('parseGitHubUrl accepts only public GitHub repository URLs', () => {
  assert.deepEqual(parseGitHubUrl('https://github.com/AksharP5/omarchy-radio-atlas'), {
    owner: 'AksharP5',
    repo: 'omarchy-radio-atlas',
    ref: null,
    repositoryUrl: 'https://github.com/AksharP5/omarchy-radio-atlas',
  });
  assert.deepEqual(parseGitHubUrl('https://github.com/acme/app/tree/feature/radio'), {
    owner: 'acme',
    repo: 'app',
    ref: 'feature/radio',
    repositoryUrl: 'https://github.com/acme/app',
  });
  assert.equal(parseGitHubUrl('http://github.com/acme/x'), null);
  assert.equal(parseGitHubUrl('https://gitlab.com/acme/x'), null);
  assert.equal(parseGitHubUrl('https://github.com/acme/x/issues'), null);
  assert.equal(parseGitHubUrl('not a url'), null);
});

test('source filtering keeps agent-relevant runtimes and excludes generated trees', () => {
  for (const path of ['node_modules/a.js', '.git/HEAD', 'dist/a.js', 'build/a', '.next/a', 'yarn.lock']) {
    assert.equal(shouldSkipPath(path), true, path);
  }
  for (const path of ['shell.qml', 'main.py', 'install.sh', 'Makefile', 'src/radio']) {
    assert.equal(isImportableFile(path), true, path);
    assert.equal(isTextFile(path), true, path);
  }
  assert.equal(isImportableFile('assets/globe.png'), true);
  assert.equal(isTextFile('assets/globe.png'), false);
  assert.equal(isImportableFile('dist/generated.js'), false);
});

test('module ids follow the launcher slug contract', () => {
  assert.equal(moduleIdFromSource('omarchy-radio-atlas'), 'omarchy-radio-atlas');
  assert.equal(moduleIdFromSource('Radio Atlas!'), 'radio-atlas');
  assert.equal(moduleIdFromSource('black-hole-standalone-v6.html'), 'black-hole');
  assert.equal(moduleIdFromSource('my-noise-pet-crt-v5 2.html'), 'my-noise-pet-crt');
  assert.equal(validModuleId('radio-atlas'), true);
  assert.equal(validModuleId('-bad'), false);
  assert.equal(validModuleId('Bad'), false);
  assert.equal(validModuleId('x'), false);
});

test('standalone HTML imports derive clean app identity and an explicit profile', () => {
  assert.deepEqual(appIdentityFromSource(
    'blocks-ce-worldfixed-lighting-v2.html',
    '<title>Blocks CE – WebGL Castle Builder</title>',
  ), { moduleId: 'blocks-ce', appTitle: 'Blocks CE' });
  assert.deepEqual(appIdentityFromSource(
    'black-hole-standalone-v6.html',
    '<title>Black Hole Studio · Standalone</title>',
  ), { moduleId: 'black-hole-studio', appTitle: 'Black Hole Studio' });
  assert.equal(standaloneHtmlEntryPath(['black-hole-standalone-v6.html']), 'black-hole-standalone-v6.html');
  assert.equal(standaloneHtmlEntryPath(['index.html', 'icon.svg']), null);
  assert.equal(importFileByteLimit(['black-hole.html']), 8 * 1024 * 1024);
  assert.equal(importFileByteLimit(['main.qml']), 512 * 1024);
  assert.equal(importFileByteLimit(['index.html', 'icon.svg']), 512 * 1024);
  assert.equal(normalizeImportCategory('Unterhaltung'), 'entertainment');
});

test('GitHub import creates one durable harness command with the porting skill', () => {
  const command = buildAppImportCommand({
    moduleId: 'omarchy-radio-atlas',
    appTitle: 'Omarchy Radio Atlas',
    category: 'entertainment',
    now: 42,
    importSource: {
      kind: 'github',
      repository_url: 'https://github.com/AksharP5/omarchy-radio-atlas',
      ref: 'main',
    },
  });
  // v4: Befehls-ID vergibt api.command; keine Sync-Lease-Felder mehr.
  assert.equal(command.id, undefined);
  assert.equal(command.record_id, 'omarchy-radio-atlas');
  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
  assert.equal(command.payload.category, 'entertainment');
  assert.equal(command.payload.desired_version, '1.0.0');
  assert.equal(command.sync_flush_timeout_ms, undefined);
  assert.equal(command.allow_dependency_delivery_lag, undefined);
  assert.deepEqual(command.payload.required_skills, ['business-os-app-module-development']);
  assert.deepEqual(command.payload.import_source, {
    kind: 'github',
    repository_url: 'https://github.com/AksharP5/omarchy-radio-atlas',
    ref: 'main',
  });
  assert.equal(command.client_context.source, 'business-os-app-importer');
});

test('folder imports declare exact RxDB dependencies', () => {
  const command = buildAppImportCommand({
    moduleId: 'local-radio',
    now: 7,
    importSource: {
      kind: 'desktop-folder',
      snapshot_id: 'snapshot-1',
      files: [{
        file_id: 'file-1', generation_id: 'gen-1', relative_path: 'main.qml', sha256: 'abc', size_bytes: 12,
      }],
    },
  });
  assert.equal(command.sync_collections, undefined);
  assert.deepEqual(command.dependencies, [{
    collection: 'desktop_files', record_id: 'file-1', generation_id: 'gen-1', content_hash: 'abc', required: true,
  }]);
});

test('presentation is a one-click source, porting, live flow', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

  assert.match(html, /data-imp-step="source"/);
  assert.match(html, /data-imp-step="progress"/);
  assert.match(html, /data-imp-step="done"/);
  assert.match(html, /data-imp-category/);
  assert.equal((html.match(/data-imp-phase/g) || []).length, 5);
  assert.doesNotMatch(html, /data-imp-install/);
  assert.doesNotMatch(html, /data-imp-back/);
  assert.doesNotMatch(html, /data-imp-pick-files-input[^>]*\bmultiple\b/);

  assert.match(css, /\.imp-job-phases/);
  assert.match(css, /prefers-reduced-motion:\s*reduce/);

  // v4: Befehl über api.command ohne Warten; Projektion live über das business_commands-Dokument.
  assert.match(js, /api\.command\(command\.command_type,/);
  assert.match(js, /api\.get\('business_commands', commandId\)\.subscribe/);
  assert.doesNotMatch(js, /commandBus|until: 'accepted'|leaseCollection|location\.(?:hash|reload)/);
  assert.match(js, /showDirectoryPicker\(\{ mode: 'read' \}\)/);
  assert.doesNotMatch(js, /transcodeApp|scaffoldModule|createWritable|mode: 'readwrite'/);
  assert.match(js, /result\.live === true/);
  assert.match(js, /result\.smoke_status/);
  assert.match(js, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
  assert.doesNotMatch(js, /\?v=|ensureStyles/);

  assert.equal(manifest.layout.shell, 'windowed');
  assert.deepEqual(manifest.presentation.initial_size, { width: 860, height: 600 });
  assert.deepEqual(manifest.presentation.minimum_size, { width: 640, height: 480 });
});
