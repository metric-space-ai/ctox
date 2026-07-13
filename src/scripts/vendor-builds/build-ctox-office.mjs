import { createHash } from 'node:crypto';
import { execFile as execFileCallback } from 'node:child_process';
import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const execFile = promisify(execFileCallback);

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..', '..', '..');
const businessOsRoot = path.join(repoRoot, 'src', 'apps', 'business-os');
const engineRoot = path.join(businessOsRoot, 'office-engine');
const sourceRoot = path.join(engineRoot, 'src');
const outputRoot = path.join(businessOsRoot, 'vendor', 'ctox-office');
const pinPath = path.join(engineRoot, 'upstream', 'euro-office-v9.3.1.json');
const sdkPatchPath = path.join(sourceRoot, 'adapters', 'sdkjs-ctox-hooks.patch');
const upstreamBuildArg = process.argv.find((value) => value.startsWith('--upstream-build='));
const upstreamSourceArg = process.argv.find((value) => value.startsWith('--upstream-source='));
const bootstrapOnly = process.argv.includes('--bootstrap-only');
const reuseVerifiedUpstream = process.argv.includes('--reuse-verified-upstream');
if (!upstreamBuildArg && !bootstrapOnly && !reuseVerifiedUpstream) {
  throw new Error('A real Euro-Office build is required. Pass --upstream-build=/absolute/path and --upstream-source=/absolute/path, use --reuse-verified-upstream for an adapter-only rebuild, or explicitly use --bootstrap-only.');
}
if (bootstrapOnly && reuseVerifiedUpstream) throw new Error('--bootstrap-only and --reuse-verified-upstream are mutually exclusive.');
if (upstreamBuildArg && !upstreamSourceArg) throw new Error('A real Euro-Office build must be tied to its pinned source. Pass --upstream-source=/absolute/path.');
const upstreamBuildRoot = upstreamBuildArg ? path.resolve(upstreamBuildArg.slice('--upstream-build='.length)) : null;
const upstreamSourceRoot = upstreamSourceArg ? path.resolve(upstreamSourceArg.slice('--upstream-source='.length)) : null;
const pin = JSON.parse(await readFile(pinPath, 'utf8'));
let upstreamPatchResult = null;
let reusedProvenance = null;
let reusedUpstreamRoot = null;
if (reuseVerifiedUpstream) {
  reusedProvenance = JSON.parse(await readFile(path.join(outputRoot, 'provenance.json'), 'utf8'));
  if (reusedProvenance.upstream_source_status !== 'pinned-web-apps-sdkjs-document-spreadsheet-closure') {
    throw new Error('Only a pinned production Euro-Office closure may be reused.');
  }
  if (reusedProvenance.oracle_release !== pin.release
    || reusedProvenance.upstream_source?.sdkjs_sha !== pin.submodules.sdkjs
    || reusedProvenance.upstream_source?.web_apps_sha !== pin.submodules['web-apps']
    || reusedProvenance.upstream_source?.core_fonts_sha !== pin.submodules['core-fonts']) {
    throw new Error('Existing Euro-Office closure does not match the current pin.');
  }
  const reusedInputs = reusedProvenance.upstream_static_inputs || [];
  if (reusedInputs.length < 500) throw new Error('Existing Euro-Office closure inventory is incomplete.');
  for (const input of reusedInputs) {
    const staged = path.join(outputRoot, input.staged_path || '');
    if (!input.staged_path?.startsWith('upstream/') || await fileSha256(staged).catch(() => '') !== input.sha256) {
      throw new Error(`Existing Euro-Office closure failed provenance verification: ${input.staged_path || input.path}`);
    }
  }
  reusedUpstreamRoot = path.join(repoRoot, 'runtime', 'build', `ctox-office-upstream-reuse-${process.pid}`);
  await rm(reusedUpstreamRoot, { recursive: true, force: true });
  await mkdir(reusedUpstreamRoot, { recursive: true });
  await cp(path.join(outputRoot, 'upstream'), reusedUpstreamRoot, { recursive: true });
}
if (upstreamSourceRoot) {
  const coreFontsRoot = path.join(upstreamSourceRoot, 'core-fonts');
  const { stdout: coreFontsSha } = await execFile('git', ['-C', coreFontsRoot, 'rev-parse', 'HEAD']);
  if (coreFontsSha.trim() !== pin.submodules['core-fonts']) {
    throw new Error(`Pinned core-fonts checkout mismatch: expected ${pin.submodules['core-fonts']}, got ${coreFontsSha.trim()}`);
  }
  const patchScript = path.join(scriptDir, 'apply-ctox-office-upstream-patches.mjs');
  const { stdout } = await execFile(process.execPath, [patchScript, `--source=${upstreamSourceRoot}`, '--check-only']);
  upstreamPatchResult = JSON.parse(stdout);
  if (upstreamPatchResult.status !== 'already-applied') {
    throw new Error(`Pinned source is not patched for CTOX build (status ${upstreamPatchResult.status}). Run ${path.relative(repoRoot, patchScript)} first.`);
  }
}
const { build } = await import(pathToFileUrl(path.join(businessOsRoot, 'node_modules', 'esbuild', 'lib', 'main.js')));

const entries = [
  ['document', path.join(sourceRoot, 'document.mjs'), 'ctox-office-document.mjs'],
  ['spreadsheet', path.join(sourceRoot, 'spreadsheet.mjs'), 'ctox-office-spreadsheet.mjs'],
];
const excludedSegments = [
  '/presentationeditor/', '/pdfeditor/', '/visioeditor/', '/mobile/', '/adminpanel/', '/wopi/',
];
const forbiddenRuntimePatterns = [
  [/\bDocumentServer\b/i, 'DocumentServer'],
  [/socket\.io/i, 'socket.io'],
  [/new\s+WebSocket\s*\(/i, 'WebSocket'],
  [/XMLHttpRequest/i, 'XMLHttpRequest'],
  [/\bfetch\s*\(/, 'fetch'],
];

await rm(outputRoot, { recursive: true, force: true });
await mkdir(path.join(outputRoot, 'runtime'), { recursive: true });

const outputs = [];
const bundledInputs = new Set();
for (const [kind, entry, filename] of entries) {
  const result = await build({
    entryPoints: [entry],
    outfile: path.join(outputRoot, filename),
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    sourcemap: false,
    minify: false,
    metafile: true,
    logLevel: 'info',
    legalComments: 'inline',
  });
  for (const input of Object.keys(result.metafile.inputs)) bundledInputs.add(path.resolve(repoRoot, input));
  outputs.push(await outputDescriptor(path.join(outputRoot, filename), kind));
}

for (const relative of ['frame.html', 'frame.css', 'frame-runtime.mjs', 'rpc.mjs']) {
  await cp(path.join(sourceRoot, relative), path.join(outputRoot, relative));
}
for (const relative of ['ctox-documents.mjs', 'ctox-spreadsheets.mjs', 'ctox-fork-core.mjs']) {
  await cp(path.join(sourceRoot, 'runtime', relative), path.join(outputRoot, 'runtime', relative));
}
await cp(path.join(sourceRoot, 'forks'), path.join(outputRoot, 'forks'), { recursive: true });
let upstreamStaticInputs = [];
if (upstreamBuildRoot) upstreamStaticInputs = await stageOfficeUpstream(upstreamBuildRoot);
else if (reusedUpstreamRoot) {
  await cp(reusedUpstreamRoot, path.join(outputRoot, 'upstream'), { recursive: true });
  const socketStagedPath = 'upstream/web-apps/vendor/socketio/socket.io.min.js';
  const socketTarget = path.join(outputRoot, socketStagedPath);
  await cp(path.join(sourceRoot, 'adapters/socketio-stub.js'), socketTarget);
  upstreamStaticInputs = await Promise.all(reusedProvenance.upstream_static_inputs.map(async (input) => (
    input.staged_path === socketStagedPath
      ? { ...input, sha256: await fileSha256(socketTarget) }
      : input
  )));
}

for (const input of bundledInputs) {
  const normalized = input.toLowerCase().replaceAll('\\', '/');
  if (excludedSegments.some((segment) => normalized.includes(segment))) {
    throw new Error(`Excluded Euro-Office surface entered the CTOX product-fork bundle: ${input}`);
  }
}

for (const file of (await listFiles(outputRoot)).filter((file) => !file.includes(`${path.sep}upstream${path.sep}`))) {
  const source = await readFile(file, 'utf8').catch(() => '');
  for (const [pattern, label] of forbiddenRuntimePatterns) {
    if (pattern.test(source)) {
      throw new Error(`Forbidden ${label} network/DocumentServer dependency entered ${path.relative(repoRoot, file)}`);
    }
  }
}

const sourceFiles = await listFiles(sourceRoot);
const forkProducts = await Promise.all(['ctox-documents', 'ctox-spreadsheets'].map(async (product) =>
  JSON.parse(await readFile(path.join(sourceRoot, 'forks', product, 'manifest.json'), 'utf8'))));
const provenance = {
  schema_version: 'ctox-office-bundle-provenance-v1',
  generator: 'src/scripts/vendor-builds/build-ctox-office.mjs',
  format: 'browser-esm',
  target: 'es2022',
  runtime_package_manager: 'none',
  oracle_release: pin.release,
  oracle_commit_sha: pin.commit_sha,
  oracle_image_digest: pin.oracle_image.index_digest,
  upstream_source_status: upstreamBuildRoot || reusedUpstreamRoot ? 'pinned-web-apps-sdkjs-document-spreadsheet-closure' : 'bootstrap-only-explicit',
  upstream_source: upstreamSourceRoot ? {
    sdkjs_sha: pin.submodules.sdkjs,
    web_apps_sha: pin.submodules['web-apps'],
    core_fonts_sha: pin.submodules['core-fonts'],
    patch_path: path.relative(repoRoot, sdkPatchPath).replaceAll('\\', '/'),
    patch_sha256: upstreamPatchResult.patch_sha256,
    patch_status: upstreamPatchResult.status,
  } : reusedProvenance?.upstream_source || null,
  fork_products: forkProducts,
  source_inputs: await Promise.all(sourceFiles.map(async (file) => ({
    path: path.relative(repoRoot, file).replaceAll('\\', '/'),
    sha256: await fileSha256(file),
  }))),
  bundle_inputs: [...bundledInputs].map((file) => path.relative(repoRoot, file).replaceAll('\\', '/')).sort(),
  upstream_static_inputs: upstreamStaticInputs,
  outputs,
  artifacts: await Promise.all((await listFiles(outputRoot)).map(async (file) => ({
    path: path.relative(repoRoot, file).replaceAll('\\', '/'),
    bytes: (await stat(file)).size,
    sha256: await fileSha256(file),
  }))),
  license_inventory: [
    { component: 'CTOX Documents fork', license: 'AGPL-3.0-only', origin: 'CTOX' },
    { component: 'CTOX Spreadsheets fork', license: 'AGPL-3.0-only', origin: 'CTOX' },
    { component: 'Euro-Office upstream ancestry', license: pin.license, origin: pin.release_url },
  ],
  excluded_surfaces: excludedSegments.map((value) => value.slice(1, -1)),
};
await writeFile(path.join(outputRoot, 'provenance.json'), `${JSON.stringify(provenance, null, 2)}\n`);
if (reusedUpstreamRoot) await rm(reusedUpstreamRoot, { recursive: true, force: true });
console.log(`built CTOX Documents and CTOX Spreadsheets ESM bundles in ${path.relative(repoRoot, outputRoot)}`);

async function listFiles(root) {
  const result = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const target = path.join(root, entry.name);
    if (entry.isDirectory()) result.push(...await listFiles(target));
    else if (entry.isFile()) result.push(target);
  }
  return result.sort();
}

async function outputDescriptor(file, kind) {
  const details = await stat(file);
  return {
    kind,
    path: path.relative(repoRoot, file).replaceAll('\\', '/'),
    bytes: details.size,
    sha256: await fileSha256(file),
  };
}

async function fileSha256(file) {
  return createHash('sha256').update(await readFile(file)).digest('hex');
}

function pathToFileUrl(file) {
  const url = new URL('file:///');
  url.pathname = path.resolve(file).replaceAll('\\', '/');
  return url.href;
}

async function stageOfficeUpstream(buildRoot) {
  const required = [
    'web-apps/apps/documenteditor/main/index.html',
    'web-apps/apps/documenteditor/main/app.js',
    'web-apps/apps/documenteditor/main/code.js',
    'web-apps/apps/spreadsheeteditor/main/index.html',
    'web-apps/apps/spreadsheeteditor/main/app.js',
    'web-apps/apps/spreadsheeteditor/main/code.js',
    'sdkjs/word/sdk-all-min.js',
    'sdkjs/word/sdk-all.js',
    'sdkjs/cell/sdk-all-min.js',
    'sdkjs/cell/sdk-all.js',
    'sdkjs/common/AllFonts.js',
    'sdkjs/common/Images/fonts_thumbnail.png.bin',
    'sdkjs/common/Images/fonts_thumbnail@2x.png.bin',
  ];
  for (const relative of required) {
    const details = await stat(path.join(buildRoot, relative)).catch(() => null);
    if (!details?.isFile()) throw new Error(`Euro-Office build artifact is missing: ${relative}`);
  }
  const destination = path.join(outputRoot, 'upstream');
  const copies = [
    ['web-apps/apps/documenteditor/main/app.js'],
    ['web-apps/apps/documenteditor/main/code.js'],
    ['web-apps/apps/documenteditor/main/index.html'],
    ['web-apps/apps/documenteditor/main/index_loader.html'],
    ['web-apps/apps/documenteditor/main/locale/de.json'],
    ['web-apps/apps/documenteditor/main/locale/en.json'],
    ['web-apps/apps/documenteditor/main/resources/css'],
    ['web-apps/apps/documenteditor/main/resources/img'],
    ['web-apps/apps/documenteditor/main/resources/numbering'],
    ['web-apps/apps/documenteditor/main/resources/symboltable'],
    ['web-apps/apps/spreadsheeteditor/main/app.js'],
    ['web-apps/apps/spreadsheeteditor/main/code.js'],
    ['web-apps/apps/spreadsheeteditor/main/index.html'],
    ['web-apps/apps/spreadsheeteditor/main/index_loader.html'],
    ['web-apps/apps/spreadsheeteditor/main/index_internal.html'],
    ['web-apps/apps/spreadsheeteditor/main/locale/de.json'],
    ['web-apps/apps/spreadsheeteditor/main/locale/en.json'],
    ['web-apps/apps/spreadsheeteditor/main/resources/css'],
    ['web-apps/apps/spreadsheeteditor/main/resources/img'],
    ['web-apps/apps/spreadsheeteditor/main/resources/symboltable'],
    ['web-apps/apps/spreadsheeteditor/main/resources/formula-lang/de.json'],
    ['web-apps/apps/spreadsheeteditor/main/resources/formula-lang/de_desc.json'],
    ['web-apps/apps/spreadsheeteditor/main/resources/formula-lang/en.json'],
    ['web-apps/apps/spreadsheeteditor/main/resources/formula-lang/en_desc.json'],
    ['web-apps/apps/common/main'],
    ['web-apps/vendor/requirejs'],
    ['web-apps/vendor/xregexp'],
    ['sdkjs/word'],
    ['sdkjs/cell'],
    ['sdkjs/common'],
    ['fonts'],
    ['document_editor_service_worker.js'],
  ];
  for (const [relative] of copies) {
    const source = path.join(buildRoot, relative);
    const details = await stat(source).catch(() => null);
    if (!details) throw new Error(`Euro-Office closure input is missing: ${relative}`);
    await cp(source, path.join(destination, relative), { recursive: details.isDirectory() });
  }
  // The branded desktop header icons are upstream source assets which the
  // web-apps grunt deploy references but does not copy into the generic
  // common-image closure.
  await cp(
    path.join(upstreamSourceRoot, 'web-apps/theme/euro-office/assets/img/header/icon-spreadsheet.svg'),
    path.join(destination, 'web-apps/apps/common/main/resources/img/header/icon-spreadsheet.svg'),
  );
  await cp(
    path.join(upstreamSourceRoot, 'web-apps/theme/euro-office/assets/img/header/icon-document.svg'),
    path.join(destination, 'web-apps/apps/common/main/resources/img/header/icon-document.svg'),
  );
  await rm(path.join(destination, 'web-apps/apps/common/main/resources/help'), { recursive: true, force: true });
  await cp(
    path.join(destination, 'web-apps/apps/common/main/resources/themes/themes.json'),
    path.join(destination, 'themes.json'),
  );
  for (const editor of ['documenteditor', 'spreadsheeteditor']) {
    const localeRoot = path.join(destination, `web-apps/apps/${editor}/main/locale`);
    const englishLocale = JSON.parse(await readFile(path.join(localeRoot, 'en.json'), 'utf8'));
    const germanLocale = JSON.parse(await readFile(path.join(localeRoot, 'de.json'), 'utf8'));
    await writeFile(path.join(localeRoot, 'de.json'), `${JSON.stringify({ ...englishLocale, ...germanLocale }, null, 2)}\n`);
  }
  const socketTarget = path.join(destination, 'web-apps/vendor/socketio/socket.io.min.js');
  await mkdir(path.dirname(socketTarget), { recursive: true });
  await cp(path.join(sourceRoot, 'adapters/socketio-stub.js'), socketTarget);
  for (const editor of ['documenteditor', 'spreadsheeteditor']) {
    const entryNames = editor === 'documenteditor'
      ? ['index.html', 'index_loader.html']
      : ['index.html', 'index_loader.html', 'index_internal.html'];
    for (const name of entryNames) {
      const target = path.join(destination, `web-apps/apps/${editor}/main`, name);
      let html = await readFile(target, 'utf8');
      html = html.replaceAll('../../../../../../sdkjs/common/device_scale.js', '../../../../sdkjs/common/device_scale.js');
      html = html.replace('<head>', '<head>\n    <meta http-equiv="Content-Security-Policy" content="default-src \'self\' data: blob:; connect-src \'self\'; img-src \'self\' data: blob:; style-src \'self\' \'unsafe-inline\'; script-src \'self\' \'unsafe-inline\' \'unsafe-eval\'; worker-src \'self\' blob:">');
      await writeFile(target, html);
    }
  }
  const files = await listFiles(destination);
  return Promise.all(files.map(async (file) => ({
    path: path.relative(buildRoot, file.replace(destination, buildRoot)).replaceAll('\\', '/'),
    staged_path: path.relative(outputRoot, file).replaceAll('\\', '/'),
    sha256: await fileSha256(file),
  })));
}
