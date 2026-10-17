// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import assert from 'node:assert/strict';
import { link, mkdtemp, mkdir, readFile, readdir, rm, symlink, truncate, unlink, utimes, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { gunzipSync } from 'node:zlib';

import {
  ROOT_RUNTIME_FILES,
  RUNTIME_TREES,
  SHELL_SCHEMA,
  MAX_RUNTIME_FILE_BYTES,
  assertNoCustomerReleaseContent,
  buildShellArtifact,
  commitStagedOutputs,
  createDeterministicGzip,
  createTarArchive,
  isExcludedRuntimePath,
  payloadImportClosure,
  sha256,
  stampShellDocument,
  validateRelativePath,
  validateSemVer,
  validateSourceCommit,
} from './build-shell-artifact.mjs';

const VERSION = '1.2.3-beta.1+shell.5';
const SOURCE_COMMIT = '0123456789abcdef0123456789abcdef01234567';

async function makeFixture() {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'ctox-shell-artifact-'));
  const sourceRoot = join(fixtureRoot, 'source');
  await mkdir(sourceRoot);
  for (const filename of ROOT_RUNTIME_FILES) {
    await writeFile(join(sourceRoot, filename), `runtime:${filename}\n`);
  }
  await writeFile(join(sourceRoot, 'index.html'), '<!doctype html><html><head><title>CTOX</title></head><body></body></html>');
  for (const tree of RUNTIME_TREES) {
    await mkdir(join(sourceRoot, tree), { recursive: true });
    await writeFile(join(sourceRoot, tree, 'runtime.txt'), `${tree}\n`);
  }
  // E38: v4 im Slot verlangt den Einstieg (v4/index.html + v4/boot.js).
  await writeFile(join(sourceRoot, 'v4', 'index.html'), '<!doctype html><html><head><title>CTOX</title></head><body></body></html>\n');
  await writeFile(join(sourceRoot, 'v4', 'boot.js'), 'export {};\n');
  // M7: Boot-Import-Abschluss. kernel/cache.mjs heißt wie ein Cache-Artefakt, ist aber von kernel/boot.mjs importiert;
  // kernel/release.mjs ist nicht erreichbar und fällt weiter unter die Namensregel.
  await writeFile(join(sourceRoot, 'v4', 'index.html'), '<!doctype html><html><head><title>CTOX</title><script type="module" src="/business-os/v4/boot.js"></script></head><body></body></html>\n');
  await mkdir(join(sourceRoot, 'v4', 'frame'), { recursive: true });
  await writeFile(join(sourceRoot, 'v4', 'frame', 'index.js'), "import 'ctox:shared/contest.js';\nexport {};\n");
  await mkdir(join(sourceRoot, 'kernel'), { recursive: true });
  await writeFile(join(sourceRoot, 'kernel', 'boot.mjs'), "import { cache } from './cache.mjs';\nexport const boot = () => cache;\n");
  await writeFile(join(sourceRoot, 'kernel', 'cache.mjs'), 'export const cache = new Map();\n');
  await writeFile(join(sourceRoot, 'kernel', 'release.mjs'), '// Entwicklerwerkzeug, kein Laufzeitinhalt\n');
  await mkdir(join(sourceRoot, 'ui-contract', 'v1'), { recursive: true });
  await writeFile(join(sourceRoot, 'ui-contract', 'v1', 'workjet-ui-contract.css'), ':root {}\n');
  await writeFile(join(sourceRoot, 'ui-contract', 'v1', 'workjet-ui-contract.json'), '{"schema":"workjet.ui.contract.v1"}\n');
  await mkdir(join(sourceRoot, 'public', 'calendar-booking'), { recursive: true });
  await writeFile(join(sourceRoot, 'public', 'calendar-booking', 'index.html'), 'unwired public intake must not ship\n');
  await mkdir(join(sourceRoot, 'installed-modules', 'tenant-local'), { recursive: true });
  await writeFile(
    join(sourceRoot, 'installed-modules', 'tenant-local', 'index.js'),
    'throw new Error("instance-local module must not ship");\n',
  );

  await mkdir(join(sourceRoot, 'vendor', 'library'), { recursive: true });
  await writeFile(join(sourceRoot, 'vendor', 'library', 'LICENSE'), 'Representative runtime license\n');
  await writeFile(join(sourceRoot, 'vendor', 'library', 'third-party-test.LICENSE'), 'Runtime license despite test-like vendor name\n');
  await writeFile(join(sourceRoot, 'vendor', 'library', 'provenance.json'), '{"source":"upstream"}\n');
  await mkdir(join(sourceRoot, 'modules', 'example', 'nested'), { recursive: true });
  await writeFile(join(sourceRoot, 'modules', 'example', 'nested', 'runtime.js'), 'export const runtime = true;\n');
  await writeFile(join(sourceRoot, 'modules', 'example', 'nested', 'runtime.test.mjs'), 'throw new Error("excluded");\n');
  await writeFile(join(sourceRoot, 'modules', 'example', 'nested', 'runtime.spec.mjs'), 'throw new Error("excluded");\n');
  await writeFile(join(sourceRoot, 'modules', 'example', 'nested', 'runtime-smoke.mjs'), 'throw new Error("excluded");\n');
  await writeFile(join(sourceRoot, 'modules', 'example', 'nested', 'fixture.json'), '{}\n');
  await mkdir(join(sourceRoot, 'modules', 'example', 'tests'), { recursive: true });
  await writeFile(join(sourceRoot, 'modules', 'example', 'tests', 'hidden.js'), 'excluded\n');
  await mkdir(join(sourceRoot, 'modules', 'example', 'node_modules', 'dependency'), { recursive: true });
  await writeFile(join(sourceRoot, 'modules', 'example', 'node_modules', 'dependency', 'index.js'), 'excluded\n');
  await mkdir(join(sourceRoot, 'shared', 'qa'), { recursive: true });
  await writeFile(join(sourceRoot, 'shared', 'qa', 'matrix.json'), '{}\n');
  await mkdir(join(sourceRoot, 'shared', 'scripts'), { recursive: true });
  await writeFile(join(sourceRoot, 'shared', 'scripts', 'generate.mjs'), 'excluded\n');
  for (const generatedDirectory of ['build-output', 'release-output', 'cache-output', 'local-state']) {
    await mkdir(join(sourceRoot, 'rxdb', generatedDirectory), { recursive: true });
    await writeFile(join(sourceRoot, 'rxdb', generatedDirectory, 'state.json'), '{}\n');
  }
  await writeFile(join(sourceRoot, 'rxdb', 'capture-release-evidence.mjs'), 'excluded generated release content\n');
  await writeFile(join(sourceRoot, 'rxdb', 'AGENTS.md'), 'excluded instructions\n');
  await writeFile(join(sourceRoot, 'assets', '.DS_Store'), 'excluded\n');
  await writeFile(join(sourceRoot, 'assets', '.gitkeep'), 'excluded\n');
  await writeFile(join(sourceRoot, 'vendor', 'package-lock.json'), '{}\n');
  await writeFile(join(sourceRoot, 'shared', 'contest.js'), 'export const retained = true;\n');
  await writeFile(join(sourceRoot, 'README.md'), 'root source is not runtime\n');
  await writeFile(join(sourceRoot, 'ARCHITECTURE.md'), 'root source is not runtime\n');
  await writeFile(join(sourceRoot, 'design-lab.html'), 'root source is not runtime\n');
  return { fixtureRoot, sourceRoot };
}

function readString(buffer, offset, length) {
  return buffer.subarray(offset, offset + length).toString('utf8').replace(/\0.*$/s, '');
}

function parseOctal(buffer, offset, length) {
  const value = readString(buffer, offset, length).trim();
  return value === '' ? 0 : Number.parseInt(value, 8);
}

function parseTar(gzipBytes) {
  const tar = gunzipSync(gzipBytes);
  const entries = [];
  let offset = 0;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      assert.ok(tar.subarray(offset, offset + 1024).every((byte) => byte === 0), 'tar ends with two zero blocks');
      return entries;
    }
    const storedChecksum = parseOctal(header, 148, 8);
    const checksumHeader = Buffer.from(header);
    checksumHeader.fill(0x20, 148, 156);
    assert.equal(checksumHeader.reduce((sum, byte) => sum + byte, 0), storedChecksum, 'USTAR header checksum is valid');
    assert.equal(readString(header, 257, 6), 'ustar', 'USTAR magic is present');
    const name = readString(header, 0, 100);
    const prefix = readString(header, 345, 155);
    const path = prefix ? `${prefix}/${name}` : name;
    const size = parseOctal(header, 124, 12);
    const type = String.fromCharCode(header[156]);
    offset += 512;
    const data = tar.subarray(offset, offset + size);
    entries.push({ path, size, type, data: Buffer.from(data) });
    offset += Math.ceil(size / 512) * 512;
  }
  throw new Error('tar did not contain an end marker');
}

async function cleanup(fixtureRoot) {
  await rm(fixtureRoot, { recursive: true, force: true });
}

test('document identity rejects ambiguous input and changes with the release', () => {
  const source = Buffer.from('<!doctype html><html><head></head><body></body></html>');
  const stamped = stampShellDocument(source, { version: VERSION, sourceCommit: SOURCE_COMMIT });
  assert.ok(stamped.includes(Buffer.from(`content="${VERSION}"`)));
  assert.notDeepEqual(stamped, stampShellDocument(source, { version: '1.2.4', sourceCommit: SOURCE_COMMIT }));
  assert.notDeepEqual(stamped, stampShellDocument(source, { version: VERSION, sourceCommit: 'a'.repeat(40) }));
  for (const invalid of [Buffer.from('missing head'), Buffer.from('<head></head><head></head>'), stamped]) {
    assert.throws(() => stampShellDocument(invalid, { version: VERSION, sourceCommit: SOURCE_COMMIT }), /one head/);
  }
});

test('identity and archive path validation is strict', () => {
  for (const version of ['1.2.3', '0.0.0-alpha.1+build.7']) assert.equal(validateSemVer(version), version);
  for (const version of ['', 'v1.2.3', '01.2.3', '1.02.3', '1.2', '1.2.3-01', '1.2.3+']) {
    assert.throws(() => validateSemVer(version), /SemVer/);
  }
  assert.equal(validateSourceCommit(SOURCE_COMMIT), SOURCE_COMMIT);
  for (const commit of ['abc', 'A'.repeat(40), 'g'.repeat(40), 'a'.repeat(41)]) {
    assert.throws(() => validateSourceCommit(commit), /40 lowercase hexadecimal/);
  }
  for (const invalidPath of ['/absolute', '../escape', 'a/../b', 'a//b', './a', 'C:/absolute', 'a\\b', 'a\nb', 'cafe\u0301.txt']) {
    assert.throws(() => validateRelativePath(invalidPath), /Archive path/);
  }
  assert.equal(validateRelativePath('vendor/café/LICENSE'), 'vendor/café/LICENSE');
});
test('exclusion policy retains runtime notices while removing non-runtime content', () => {
  assert.equal(isExcludedRuntimePath('vendor/library/LICENSE'), false);
  assert.equal(isExcludedRuntimePath('vendor/library/third-party-test.LICENSE'), false);
  assert.equal(isExcludedRuntimePath('vendor/library/THIRD_PARTY_NOTICES.md'), false);
  assert.equal(isExcludedRuntimePath('vendor/library/provenance.json'), false);
  assert.equal(isExcludedRuntimePath('modules/appsec-pentest/appsec-pentest.js'), false);
  assert.equal(isExcludedRuntimePath('shared/shell-release-status.js'), false);
  assert.equal(isExcludedRuntimePath('public/calendar-booking', { directory: true }), true);
  assert.equal(isExcludedRuntimePath('public/calendar-booking/index.html'), true);
  assert.equal(isExcludedRuntimePath('modules/example/tests', { directory: true }), true);
  assert.equal(isExcludedRuntimePath('modules/example/node_modules/pkg.js'), true);
  assert.equal(isExcludedRuntimePath('shared/runtime.spec.mjs'), true);
  assert.equal(isExcludedRuntimePath('rxdb/cache-output', { directory: true }), true);
  assert.equal(isExcludedRuntimePath('rxdb/CLAUDE.md'), true);
});

test('global shell releases reject customer-bound modules and runtime state', () => {
  const bytes = (value) => Buffer.from(JSON.stringify(value));
  assert.throws(
    () => assertNoCustomerReleaseContent([{ path: 'modules/rem-private/module.json', data: bytes({ id: 'rem-private' }) }]),
    /private instance-bound distribution/,
  );
  assert.throws(
    () => assertNoCustomerReleaseContent([{ path: 'modules/private/module.json', data: bytes({ id: 'private', distribution: 'customer' }) }]),
    /private instance-bound distribution/,
  );
  assert.throws(
    () => assertNoCustomerReleaseContent([{ path: 'installed-modules/private/index.js', data: Buffer.from('') }]),
    /must not ship/,
  );
  assert.doesNotThrow(() => assertNoCustomerReleaseContent([
    { path: 'modules/tickets/module.json', data: bytes({ id: 'tickets', distribution: 'public' }) },
  ]));
});

test('deterministic builds have byte-identical archives and complete sorted inventories', async () => {
  const { fixtureRoot, sourceRoot } = await makeFixture();
  try {
    const firstOutput = join(fixtureRoot, 'first');
    const secondOutput = join(fixtureRoot, 'second');
    const first = await buildShellArtifact({ sourceRoot, outputDir: firstOutput, version: VERSION, sourceCommit: SOURCE_COMMIT });
    await utimes(join(sourceRoot, 'app.js'), new Date('2035-01-01T00:00:00Z'), new Date('2035-01-01T00:00:00Z'));
    const second = await buildShellArtifact({ sourceRoot, outputDir: secondOutput, version: VERSION, sourceCommit: SOURCE_COMMIT });
    const firstArchive = await readFile(first.archivePath);
    const secondArchive = await readFile(second.archivePath);
    assert.deepEqual(firstArchive, secondArchive);

    const entries = parseTar(firstArchive);
    const paths = entries.map((entry) => entry.path);
    assert.deepEqual(paths, [...paths].sort(), 'archive entries are sorted');
    const root = `ctox-business-os-shell-${VERSION}`;
    assert.equal(paths[0], `${root}/`);
    for (const filename of ROOT_RUNTIME_FILES) assert.ok(paths.includes(`${root}/${filename}`));
    for (const tree of RUNTIME_TREES) assert.ok(paths.includes(`${root}/${tree}/`));
    assert.ok(paths.includes(`${root}/vendor/library/LICENSE`));
    assert.ok(paths.includes(`${root}/vendor/library/third-party-test.LICENSE`));
    assert.ok(paths.includes(`${root}/vendor/library/provenance.json`));
    assert.ok(paths.includes(`${root}/shared/contest.js`));
    assert.ok(paths.includes(`${root}/kernel/cache.mjs`), 'importiertes kernel/cache.mjs fehlt (Namensregel)');
    assert.ok(!paths.includes(`${root}/kernel/release.mjs`), 'nicht erreichbares kernel/release.mjs im Slot');
    assert.ok(paths.includes(`${root}/ctox-shell-manifest.json`));
    assert.ok(!paths.includes(`${root}/public/calendar-booking/index.html`));
    for (const excluded of [
      'modules/example/nested/runtime.test.mjs',
      'modules/example/nested/runtime.spec.mjs',
      'modules/example/nested/runtime-smoke.mjs',
      'modules/example/nested/fixture.json',
      'modules/example/tests/hidden.js',
      'modules/example/node_modules/dependency/index.js',
      'installed-modules/tenant-local/index.js',
      'shared/qa/matrix.json',
      'shared/scripts/generate.mjs',
      'rxdb/build-output/state.json',
      'rxdb/release-output/state.json',
      'rxdb/cache-output/state.json',
      'rxdb/local-state/state.json',
      'rxdb/capture-release-evidence.mjs',
      'rxdb/AGENTS.md',
      'assets/.DS_Store',
      'assets/.gitkeep',
      'vendor/package-lock.json',
      'README.md',
      'ARCHITECTURE.md',
      'design-lab.html',
    ]) assert.ok(!paths.includes(`${root}/${excluded}`), `${excluded} is excluded`);

    const embeddedEntry = entries.find((entry) => entry.path === `${root}/ctox-shell-manifest.json`);
    const embedded = JSON.parse(embeddedEntry.data.toString('utf8'));
    const html = entries.find((entry) => entry.path === `${root}/index.html`).data.toString('utf8');
    assert.ok(html.includes(`<meta name="ctox-shell-version" content="${embedded.version}">`));
    assert.ok(html.includes(`<meta name="ctox-shell-source-commit" content="${embedded.sourceCommit}">`));
    assert.equal(embedded.files.find((file) => file.path === 'index.html').sha256, sha256(Buffer.from(html)));
    assert.doesNotMatch(await readFile(join(sourceRoot, 'index.html'), 'utf8'), /ctox-shell-version/);
    assert.equal(embedded.schema, SHELL_SCHEMA);
    assert.equal(embedded.version, VERSION);
    assert.equal(embedded.sourceCommit, SOURCE_COMMIT);
    assert.equal(embedded.entry, 'index.html');
    assert.equal(embedded.archiveRoot, root);
    assert.deepEqual(embedded.files.map((record) => record.path), [...embedded.files.map((record) => record.path)].sort());

    const archivedFiles = entries
      .filter((entry) => entry.type === '0' && entry.path !== `${root}/ctox-shell-manifest.json`)
      .map((entry) => ({ path: entry.path.slice(root.length + 1), byteSize: entry.size, sha256: sha256(entry.data) }));
    assert.deepEqual(embedded.files, archivedFiles);

    const detached = JSON.parse(await readFile(first.manifestPath, 'utf8'));
    assert.deepEqual(detached.files, embedded.files);
    assert.equal(detached.archiveFilename, first.archiveFilename);
    assert.equal(detached.archiveByteLength, firstArchive.length);
    assert.equal(detached.archiveSha256, sha256(firstArchive));
    assert.equal(detached.embeddedManifestSha256, sha256(embeddedEntry.data));
    assert.equal(await readFile(first.checksumPath, 'utf8'), `${sha256(firstArchive)}  ${first.archiveFilename}\n`);
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('USTAR and gzip helpers produce parseable deterministic bytes', () => {
  const longDirectory = `root/${'prefix/'.repeat(12)}directory`;
  const entries = [
    { path: 'root', type: 'directory' },
    { path: 'root/a.txt', type: 'file', data: Buffer.from('alpha') },
    { path: longDirectory, type: 'directory' },
    { path: `${longDirectory}/long-name.txt`, type: 'file', data: Buffer.from('long') },
  ];
  const first = createDeterministicGzip(createTarArchive(entries));
  const second = createDeterministicGzip(createTarArchive([...entries].reverse()));
  assert.deepEqual(first, second);
  assert.equal(first.subarray(4, 8).readUInt32LE(), 0, 'gzip mtime is fixed');
  assert.equal(first[9], 0xff, 'gzip OS marker is platform-neutral');
  assert.deepEqual(parseTar(first).map((entry) => entry.path), [
    'root/',
    'root/a.txt',
    `${longDirectory}/`,
    `${longDirectory}/long-name.txt`,
  ]);
  assert.throws(() => createTarArchive([{ path: `root/${'x'.repeat(256)}`, type: 'file', data: '' }]), /USTAR/);
});

test('missing required runtime entries fail without final or partial outputs', async () => {
  const { fixtureRoot, sourceRoot } = await makeFixture();
  try {
    await rm(join(sourceRoot, 'index.html'));
    const outputDir = join(fixtureRoot, 'missing-output');
    await assert.rejects(
      buildShellArtifact({ sourceRoot, outputDir, version: '1.0.0', sourceCommit: SOURCE_COMMIT }),
      /Required runtime file is missing: index.html/,
    );
    await assert.rejects(readdir(outputDir), { code: 'ENOENT' });
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('shared Workjet UI contract is mandatory in every shell payload', async () => {
  const { fixtureRoot, sourceRoot } = await makeFixture();
  try {
    await rm(join(sourceRoot, 'ui-contract', 'v1', 'workjet-ui-contract.css'));
    const outputDir = join(fixtureRoot, 'missing-contract-output');
    await assert.rejects(
      buildShellArtifact({ sourceRoot, outputDir, version: '1.0.0', sourceCommit: SOURCE_COMMIT }),
      /Required runtime file is missing: ui-contract\/v1\/workjet-ui-contract\.css/,
    );
    await assert.rejects(readdir(outputDir), { code: 'ENOENT' });
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('symlinks in runtime trees are rejected and never archived', async () => {
  const { fixtureRoot, sourceRoot } = await makeFixture();
  try {
    await symlink(join(sourceRoot, 'app.js'), join(sourceRoot, 'shared', 'linked.js'));
    const outputDir = join(fixtureRoot, 'symlink-output');
    await assert.rejects(
      buildShellArtifact({ sourceRoot, outputDir, version: '1.0.0', sourceCommit: SOURCE_COMMIT }),
      /Symlinks are forbidden: shared\/linked.js/,
    );
    await assert.rejects(readdir(outputDir), { code: 'ENOENT' });
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('oversized runtime files fail before archive allocation', async () => {
  const { fixtureRoot, sourceRoot } = await makeFixture();
  try {
    await truncate(join(sourceRoot, 'app.js'), MAX_RUNTIME_FILE_BYTES + 1);
    const outputDir = join(fixtureRoot, 'oversized-output');
    await assert.rejects(
      buildShellArtifact({ sourceRoot, outputDir, version: '1.0.0', sourceCommit: SOURCE_COMMIT }),
      /Runtime file exceeds the .* byte limit: app\.js/,
    );
    await assert.rejects(readdir(outputDir), { code: 'ENOENT' });
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('atomic no-clobber publication rolls back owned finals and preserves competing outputs', async () => {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'ctox-shell-atomic-'));
  try {
    const outputs = ['archive', 'manifest', 'checksum'].map((name) => ({
      stagedPath: join(fixtureRoot, `.${name}.tmp`),
      finalPath: join(fixtureRoot, name),
    }));
    for (const output of outputs) await writeFile(output.stagedPath, output.finalPath);
    let linkCount = 0;
    await assert.rejects(commitStagedOutputs(outputs, {
      link: async (from, to) => {
        linkCount += 1;
        if (linkCount === 2) throw new Error('injected link failure');
        await link(from, to);
      },
      unlink,
    }), /injected link failure/);
    assert.deepEqual(await readdir(fixtureRoot), []);

    const stagedPath = join(fixtureRoot, '.archive.tmp');
    const finalPath = join(fixtureRoot, 'archive');
    await writeFile(stagedPath, 'ours');
    await writeFile(finalPath, 'competitor');
    await assert.rejects(
      commitStagedOutputs([{ stagedPath, finalPath }]),
      { code: 'EEXIST' },
    );
    assert.equal(await readFile(finalPath, 'utf8'), 'competitor');
    assert.deepEqual(await readdir(fixtureRoot), ['archive']);
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('Shell v4 im Slot: jede Datei aus v4/manifests/shell.manifest.json liegt im Laufzeitinhalt, nichts aus Labor, Werkzeugen oder Doku', async () => {
  const { collectRuntimePayload: collect } = await import('./build-shell-artifact.mjs');
  const root = new URL('..', import.meta.url).pathname;
  const { files } = await collect(root);
  const paths = new Set(files.map((f) => f.path));
  const shell = JSON.parse(await readFile(join(root, 'v4/manifests/shell.manifest.json'), 'utf8'));
  const missing = Object.keys(shell.files).filter((p) => !paths.has(p));
  assert.deepEqual(missing, [], 'Shell-Manifest verweist auf Dateien, die nicht im Slot liegen');
  // E31.3: App-Manifeste, ihre Dateien, Schemas, Eigentumszuordnung und (falls vorhanden) der v4-Loader liegen im Slot.
  const { readdir: rd, stat } = await import('node:fs/promises');
  const appMissing = [];
  for (const name of (await rd(join(root, 'v4/manifests'))).filter((n) => n.endsWith('.manifest.json'))) {
    const m = JSON.parse(await readFile(join(root, 'v4/manifests', name), 'utf8'));
    if (!String(m._source_dir || '').includes('customer-modules/') && !paths.has(`v4/manifests/${name}`)) appMissing.push(`v4/manifests/${name}`);
    if (name === 'shell.manifest.json' || !m._source_dir) continue;
    const base = String(m._source_dir).replace(/^src\/apps\/business-os\/?/, '');
    // Kunden-Apps gehören nicht in den globalen Slot – auch ihr Manifest nicht.
    if (base.startsWith('customer-modules/')) { assert.ok(!paths.has(`v4/manifests/${name}`), `Kunden-Manifest ${name} im globalen Slot`); continue; }
    for (const f of Array.isArray(m.files) ? m.files : Object.keys(m.files || {})) {
      const rel = base ? `${base}/${f}` : f;
      if (!paths.has(rel) && !/\.md$/i.test(rel)) appMissing.push(rel);
    }
  }
  assert.deepEqual(appMissing, [], 'App-Manifeste verweisen auf Dateien, die nicht im Slot liegen');
  assert.ok(paths.has('v4/tools/manifest/out/ownership.json'), 'ownership.json (load_base_release) fehlt');
  for (const name of (await rd(join(root, 'v4/schemas'))).filter((n) => n.endsWith('.collections.schema.json'))) {
    assert.ok(paths.has(`v4/schemas/${name}`), `v4/schemas/${name} fehlt`);
  }
  if (await stat(join(root, 'v4/boot/index.html')).then(() => true, () => false)) assert.ok(paths.has('v4/boot/index.html'), 'v4-Loader fehlt');
  const v4 = [...paths].filter((p) => p.startsWith('v4/'));
  assert.ok(v4.includes('v4/frame/index.js') && v4.includes('v4/shell/workjet/index.js') && v4.includes('v4/sw.js'), 'v4-Shell fehlt im Slot');
  const unwanted = v4.filter((p) => p !== 'v4/tools/manifest/out/ownership.json' && (/^v4\/(frame\/lab|lab|tools|debt|k24|os-primitives)\//.test(p) || /\.md$/i.test(p) || /\.test\.m?js$/.test(p)));
  assert.deepEqual(unwanted, [], 'Labor, Werkzeuge, Doku oder Tests im Slot');
});

test('E38: Kern-Bootstrap im Slot ohne Tests, Testkit, Fake-Core und Doku; v4-Einstieg Pflicht', async () => {
  const { isExcludedRuntimePath, V4_ENTRY_FILES, RUNTIME_TREES } = await import('./build-shell-artifact.mjs');
  assert.ok(RUNTIME_TREES.includes('kernel'));
  for (const path of ['kernel/boot.mjs', 'kernel/loader.mjs', 'kernel/kernel.mjs', 'kernel/transport-webrtc.mjs', 'kernel/index.d.ts']) {
    assert.equal(isExcludedRuntimePath(path), false, path);
  }
  for (const path of ['kernel/tests/kernel.test.mjs', 'kernel/testkit/app-host.mjs', 'kernel/fake-core.mjs', 'kernel/fake-core-runtime.mjs', 'kernel/README.md']) {
    assert.equal(isExcludedRuntimePath(path), true, path);
  }
  assert.equal(isExcludedRuntimePath('kernel/tests', { directory: true }), true);
  assert.deepEqual([...V4_ENTRY_FILES], ['v4/index.html', 'v4/boot.js']);
  for (const path of V4_ENTRY_FILES) assert.equal(isExcludedRuntimePath(path), false, path);
});

test('M7: Boot-Import-Abschluss – eine importierte Datei außerhalb der Nutzlast bricht den Bau ab', async () => {
  const { fixtureRoot, sourceRoot } = await makeFixture();
  try {
    await mkdir(join(sourceRoot, 'kernel', 'tests'), { recursive: true });
    await writeFile(join(sourceRoot, 'kernel', 'tests', 'probe.mjs'), 'export {};\n');
    await writeFile(join(sourceRoot, 'kernel', 'boot.mjs'), "import './cache.mjs';\nimport './tests/probe.mjs';\n");
    const outputDir = join(fixtureRoot, 'closure-output');
    await assert.rejects(
      buildShellArtifact({ sourceRoot, outputDir, version: '1.0.0', sourceCommit: SOURCE_COMMIT }),
      /Shell import closure is incomplete in the runtime payload \(1\): kernel\/boot\.mjs → \.\/tests\/probe\.mjs/,
    );
    await assert.rejects(readdir(outputDir), { code: 'ENOENT' });
  } finally {
    await cleanup(fixtureRoot);
  }
});

test('M7: Boot-Import-Abschluss des echten Baums liegt vollständig in der Nutzlast (kernel/cache.mjs eingeschlossen)', async () => {
  const { collectRuntimePayload: collect } = await import('./build-shell-artifact.mjs');
  const root = new URL('..', import.meta.url).pathname;
  const { files } = await collect(root);
  const { reachable, missing } = await payloadImportClosure(files);
  assert.deepEqual(missing, []);
  for (const path of ['v4/index.html', 'v4/boot.js', 'kernel/boot.mjs', 'kernel/kernel.mjs', 'kernel/cache.mjs', 'v4/frame/index.js']) {
    assert.ok(reachable.has(path), `${path} nicht im Abschluss`);
  }
  // Gegenprobe: ohne kernel/cache.mjs (alte Namensregel) meldet der Abschluss genau diese Lücke.
  const without = await payloadImportClosure(files.filter((f) => f.path !== 'kernel/cache.mjs'));
  assert.deepEqual(without.missing.map((m) => m.path), ['kernel/cache.mjs']);
});
