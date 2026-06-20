#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';

const moduleId = process.argv[2];
const modeArg = process.argv[3] || '';

if (!moduleId || moduleId.includes('/') || moduleId.includes('\\') || moduleId === '.' || moduleId === '..') {
  console.error('Usage: node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <module> [--installed]');
  process.exit(2);
}
if (modeArg && modeArg !== '--installed') {
  console.error(`Unknown option: ${modeArg}`);
  process.exit(2);
}

const root = process.cwd();
const sourceDir = join(root, 'src/apps/business-os/modules', moduleId);

function installedAppRootFor(workspace) {
  const runtimeRoot = join(workspace, 'runtime/business-os');
  if (existsSync(join(workspace, 'runtime')) || existsSync(runtimeRoot)) return runtimeRoot;
  return join(workspace, 'business-os');
}

const installedDir = join(installedAppRootFor(root), 'installed-modules', moduleId);
const installedMode = modeArg === '--installed' || (!existsSync(sourceDir) && existsSync(installedDir));
const moduleDir = installedMode ? installedDir : sourceDir;
const expectedEntry = installedMode
  ? `installed-modules/${moduleId}/index.html`
  : `modules/${moduleId}/index.html`;
const expectedInstallScope = installedMode ? 'installed' : 'store';
const registryPath = join(root, 'src/apps/business-os/modules/registry.json');
const failures = [];

const shellCollections = new Set([
  'business_module_catalog',
  'ctox_runtime_settings',
  'business_commands',
  'ctox_queue_tasks',
]);

const allowedInstalledRootFiles = new Set([
  'module.json',
  'collections.schema.json',
  'schema.js',
  'index.html',
  'index.css',
  'index.js',
  'icon.svg',
]);
const allowedInstalledRootDirs = new Set(['core', 'locales', 'tests']);
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function fail(message) {
  failures.push(message);
}

function rel(path) {
  return relative(root, path).split(sep).join('/');
}

function readText(path) {
  return readFileSync(path, 'utf8');
}

function readJson(path) {
  try {
    return JSON.parse(readText(path));
  } catch (error) {
    fail(`${rel(path)} is not valid JSON: ${error.message}`);
    return null;
  }
}

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    const stats = statSync(path);
    if (stats.isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}

function hasSegment(path, segment) {
  return path.split(sep).includes(segment);
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function stripJsComments(text) {
  return String(text || '')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/.*$/gm, '$1');
}

function parseNamedImportList(raw) {
  return stripJsComments(raw)
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => part.replace(/^type\s+/, '').split(/\s+as\s+/i)[0].trim())
    .filter(Boolean);
}

function parseNamedImports(text) {
  const imports = [];
  const source = stripJsComments(text);
  const pattern = /\bimport\s*\{([\s\S]*?)\}\s*from\s*['"]([^'"]+)['"]/g;
  for (const match of source.matchAll(pattern)) {
    imports.push({ specifier: match[2], names: parseNamedImportList(match[1]) });
  }
  return imports;
}

function parseExportedNames(text) {
  const names = new Set();
  const source = stripJsComments(text);
  for (const match of source.matchAll(/\bexport\s+(?:async\s+)?(?:function|class)\s+([A-Za-z_$][\w$]*)/g)) {
    names.add(match[1]);
  }
  for (const match of source.matchAll(/\bexport\s+(?:const|let|var)\s+([^;\n]+)/g)) {
    for (const part of match[1].split(',')) {
      const name = part.trim().match(/^([A-Za-z_$][\w$]*)/);
      if (name) names.add(name[1]);
    }
  }
  for (const match of source.matchAll(/\bexport\s*\{([\s\S]*?)\}/g)) {
    for (const part of stripJsComments(match[1]).split(',')) {
      const trimmed = part.trim();
      if (!trimmed) continue;
      const alias = trimmed.match(/\s+as\s+([A-Za-z_$][\w$]*)$/i);
      const direct = trimmed.match(/^([A-Za-z_$][\w$]*)/);
      if (alias) names.add(alias[1]);
      else if (direct) names.add(direct[1]);
    }
  }
  return names;
}

function extractStaticImportSpecs(text) {
  const specs = [];
  const importExportFromPattern = /\b(?:import|export)\s+(?:[\s\S]*?\s+from\s*)['"]([^'"]+)['"]/g;
  for (const match of text.matchAll(importExportFromPattern)) specs.push(match[1]);
  const sideEffectImportPattern = /\bimport\s*['"]([^'"]+)['"]/g;
  for (const match of text.matchAll(sideEffectImportPattern)) specs.push(match[1]);
  return specs;
}

function resolveRelativeJsImport(baseFile, specifier) {
  if (!specifier.startsWith('.')) return null;
  const target = join(dirname(baseFile), specifier);
  const candidates = /\.[cm]?js$/i.test(specifier)
    ? [target]
    : [target, `${target}.js`, `${target}.mjs`];
  return candidates.find((candidate) => existsSync(candidate) && statSync(candidate).isFile()) || null;
}

function relativeImportExists(baseFile, specifier) {
  if (!specifier.startsWith('.')) return true;
  const target = join(dirname(baseFile), specifier);
  if (/\.[cm]?js$/i.test(specifier) || specifier.endsWith('.css') || specifier.endsWith('.html')) {
    return existsSync(target);
  }
  return [target, `${target}.js`, `${target}.mjs`].some((candidate) => existsSync(candidate));
}

function collectEsmImportExportFailures(files) {
  const exportCache = new Map();
  const messages = [];
  for (const importer of files) {
    for (const imported of parseNamedImports(readText(importer))) {
      const target = resolveRelativeJsImport(importer, imported.specifier);
      if (!target) continue;
      if (!exportCache.has(target)) exportCache.set(target, parseExportedNames(readText(target)));
      const exportedNames = exportCache.get(target);
      for (const name of imported.names) {
        if (!exportedNames.has(name)) {
          messages.push(`${rel(importer)} imports \`${name}\` from ${imported.specifier}, but ${rel(target)} does not provide an export named \`${name}\``);
        }
      }
    }
  }
  return messages;
}

function collectStringLiterals(text) {
  const values = [];
  const pattern = /(['"`])((?:\\[\s\S]|(?!\1)[\s\S])*?)\1/g;
  for (const match of String(text || '').matchAll(pattern)) {
    if (match[1] === '`' && match[2].includes('${')) continue;
    values.push(match[2]);
  }
  return values;
}

function htmlDataActions(html) {
  const actions = new Set();
  for (const match of String(html || '').matchAll(/\bdata-action\s*=\s*(['"])([^'"]+)\1/g)) {
    const action = match[2].trim();
    if (action) actions.add(action);
  }
  return actions;
}

function indexJsHandlesDataAction(indexJs, action) {
  const escaped = escapeRegExp(action);
  return new RegExp(String.raw`\[data-action\s*=\s*["']${escaped}["']\]`).test(indexJs)
    || new RegExp(String.raw`(?:===|==|case)\s*['"\`]${escaped}['"\`]`).test(indexJs)
    || new RegExp(String.raw`['"\`]${escaped}['"\`]\s*:`).test(indexJs);
}

function hasBusinessOsChatTaskCommandType(text) {
  if (/(?:command_type\s*:\s*['"]business_os\.chat\.task['"]|["']command_type["']\s*:\s*["']business_os\.chat\.task["'])/.test(text)) {
    return true;
  }
  const constants = new Set();
  for (const match of text.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*['"]business_os\.chat\.task['"]/g)) {
    constants.add(match[1]);
  }
  return Array.from(constants).some((name) => {
    const escaped = escapeRegExp(name);
    return new RegExp(String.raw`(?:command_type\s*:\s*${escaped}\b|["']command_type["']\s*:\s*${escaped}\b)`).test(text);
  });
}

function hasCtoxTicketCommandType(text) {
  return /(?:command_type\s*:\s*['"]ctox\.ticket\.[^'"]+['"]|["']command_type["']\s*:\s*["']ctox\.ticket\.[^'"]+["'])/.test(text);
}

if (!existsSync(moduleDir)) {
  fail(`${rel(moduleDir)} does not exist`);
}

const requiredFiles = [
  'module.json',
  'collections.schema.json',
  'schema.js',
  'index.html',
  'index.css',
  'index.js',
  'icon.svg',
  'locales/de.json',
  'locales/en.json',
  ...(installedMode ? ['core/automation.mjs', 'core/records.mjs'] : []),
];

for (const file of requiredFiles) {
  if (!existsSync(join(moduleDir, file))) fail(`missing ${rel(join(moduleDir, file))}`);
}

if (installedMode && existsSync(moduleDir)) {
  for (const name of readdirSync(moduleDir)) {
    const path = join(moduleDir, name);
    const stats = statSync(path);
    const allowed = stats.isDirectory()
      ? allowedInstalledRootDirs.has(name)
      : allowedInstalledRootFiles.has(name);
    if (!allowed) fail(`unexpected installed-module root entry: ${rel(path)}`);
  }
}

const manifest = existsSync(join(moduleDir, 'module.json')) ? readJson(join(moduleDir, 'module.json')) : null;
const schemaDoc = existsSync(join(moduleDir, 'collections.schema.json'))
  ? readJson(join(moduleDir, 'collections.schema.json'))
  : null;

if (manifest) {
  if (manifest.id !== moduleId) fail(`module.json id must be ${moduleId}`);
  if (manifest.entry !== expectedEntry) fail(`module.json entry must be ${expectedEntry}`);
  if (manifest.install_scope !== expectedInstallScope) {
    fail(`module.json install_scope must be ${expectedInstallScope}`);
  }
  if (!Array.isArray(manifest.collections)) fail('module.json collections must be an array');
  if (installedMode) {
    const version = String(manifest.version || '');
    const parsed = semverPattern.exec(version);
    if (!parsed) fail('module.json version must be SemVer x.y.z without a v prefix');
    else if (parsed[1] === '0' && parsed[2] === '0' && parsed[3] === '0') {
      fail('module.json version 0.0.0 is not a valid Business OS app work version');
    }
    if (manifest.source === 'local') {
      fail('module.json source=local is a source/store module manifest field; omit it for runtime-installed modules');
    }
    if (manifest.store?.source_path && manifest.store.source_path !== `installed-modules/${moduleId}`) {
      fail(`module.json store.source_path must be installed-modules/${moduleId}`);
    }
    if (manifest.store?.distribution && manifest.store.distribution !== 'ctox-runtime-installed-module') {
      fail('module.json store.distribution must be ctox-runtime-installed-module');
    }
    if (manifest.store?.installable === true) {
      fail('module.json store.installable must not be true for runtime-installed modules');
    }
  }
  if (manifest.layout?.right && !manifest.layout?.third_pane_justification) {
    fail('module.json layout.right requires layout.third_pane_justification');
  }
  if (Object.prototype.hasOwnProperty.call(manifest.layout || {}, 'right_resizer')) {
    fail('module.json layout.right_resizer is forbidden');
  }
  if (manifest.layout?.icon_svg || manifest.icon_svg || manifest.iconSvg) {
    fail('module.json inline icon fields are forbidden; keep SVG markup in icon.svg');
  }
  const manifestText = JSON.stringify(manifest);
  if (/<\s*svg\b/i.test(manifestText)) {
    fail('module.json must not embed inline SVG markup');
  }
}

if (schemaDoc) {
  if (schemaDoc.schema_format !== 'ctox-business-os-module-collections-v1') {
    fail('collections.schema.json schema_format must be ctox-business-os-module-collections-v1');
  }
  if (!schemaDoc.collections || typeof schemaDoc.collections !== 'object' || Array.isArray(schemaDoc.collections)) {
    fail('collections.schema.json collections must be an object');
  }
}

if (manifest && schemaDoc?.collections) {
  for (const name of Object.keys(schemaDoc.collections)) {
    if (shellCollections.has(name)) fail(`collections.schema.json redeclares shell collection ${name}`);
  }
  for (const name of manifest.collections || []) {
    if (!shellCollections.has(name) && !schemaDoc.collections[name]) {
      fail(`collections.schema.json missing non-shell collection from module.json: ${name}`);
    }
  }
}

const schemaJsPath = join(moduleDir, 'schema.js');
if (existsSync(schemaJsPath)) {
  const schemaJs = readText(schemaJsPath);
  for (const collection of shellCollections) {
    const pattern = new RegExp(String.raw`(?:^|[,{]\s*)(?:['"]${collection}['"]|${collection})\s*:`, 'm');
    if (pattern.test(schemaJs)) fail(`schema.js exports shell-registered collection key ${collection}`);
  }
}

if (!installedMode && manifest && existsSync(registryPath)) {
  const registry = readJson(registryPath);
  const entry = (registry?.modules || []).find((item) => item.id === moduleId);
  if (!entry) fail(`registry.json missing module ${moduleId}`);
  else {
    if (entry.entry !== manifest.entry) fail(`registry entry mismatch for ${moduleId}: entry`);
    if (entry.install_scope !== manifest.install_scope) {
      fail(`registry entry mismatch for ${moduleId}: install_scope`);
    }
  }
}

const files = walk(moduleDir);
const jsFiles = files.filter((file) => /\.(?:js|mjs)$/.test(file));
const runtimeFiles = files.filter((file) =>
  /\.(?:js|mjs|html|css)$/.test(file) && !hasSegment(file, 'tests') && !file.endsWith('.test.mjs')
);

for (const path of files) {
  const name = path.split(sep).at(-1);
  if (
    name === 'package.json'
    || name === 'package-lock.json'
    || name === 'yarn.lock'
    || name === 'pnpm-lock.yaml'
    || name === 'bun.lockb'
    || name === 'vite.config.js'
    || name === 'webpack.config.js'
    || name === 'rollup.config.js'
    || name === '.DS_Store'
    || hasSegment(path, 'node_modules')
    || hasSegment(path, 'dist')
    || hasSegment(path, 'build')
    || name?.endsWith('.jsx')
    || name?.endsWith('.tsx')
    || name?.endsWith('.bundle.js')
    || name?.endsWith('.bundle.css')
  ) {
    fail(`forbidden module artifact ${rel(path)}`);
  }
}

for (const path of jsFiles) {
  const text = readText(path);
  for (const specifier of extractStaticImportSpecs(text)) {
    if (!relativeImportExists(path, specifier)) {
      fail(`${rel(path)} relative import ${specifier} does not exist`);
    }
    if (!specifier.startsWith('.') && !hasSegment(path, 'tests')) {
      fail(`${rel(path)} imports bare package ${specifier}; Business OS apps must use browser ESM local files only`);
    }
  }
}
for (const message of collectEsmImportExportFailures(jsFiles)) fail(`ESM import/export mismatch: ${message}`);

const indexJsPath = join(moduleDir, 'index.js');
const indexHtmlPath = join(moduleDir, 'index.html');
const indexJs = existsSync(indexJsPath) ? readText(indexJsPath) : '';
const indexHtml = existsSync(indexHtmlPath) ? readText(indexHtmlPath) : '';
const runtimeText = runtimeFiles.map((file) => readText(file)).join('\n');
const nonTestModuleText = files
  .filter((file) => /\.(?:js|mjs|html|css|json)$/.test(file))
  .filter((file) => !hasSegment(file, 'tests') && !file.endsWith('.test.mjs'))
  .map((file) => readText(file))
  .join('\n');

if (!/\bexport\s+(?:async\s+)?function\s+mount\s*\(|\bexport\s*\{[^}]*\bmount\b/.test(indexJs)) {
  fail('index.js must export mount(ctx)');
}
if (!/\bctx\.host\b|\bhost\.innerHTML\b|\bhost\.append/.test(indexJs)) {
  fail('index.js must render into ctx.host');
}

if (installedMode) {
  if (!/\bctx\??\.db\b|\bstate\.ctx\??\.db\b/.test(runtimeText)) {
    fail('installed module must persist records through the shell-provided ctx.db collection handle');
  }
  if (!/\b(?:ctx|state\.ctx)\??\.commandBus\??\.\s*dispatch\s*\(/.test(runtimeText)) {
    fail('installed module must dispatch at least one automation through ctx.commandBus.dispatch');
  }
  const hasChatTaskAutomation = /\bbusiness_os\.chat\.task\b/.test(nonTestModuleText)
    && hasBusinessOsChatTaskCommandType(nonTestModuleText);
  const hasTicketAutomation = /\bctox\.ticket\./.test(nonTestModuleText)
    && hasCtoxTicketCommandType(nonTestModuleText);
  if (!hasChatTaskAutomation && !hasTicketAutomation) {
    fail('installed module must include a supported automation command: business_os.chat.task or ctox.ticket.*');
  }
  if (hasChatTaskAutomation && !/\brecord_snapshot\b/.test(nonTestModuleText)) {
    fail('installed module automation must include payload.record_snapshot');
  }
}

const runtimeRules = [
  ['localStorage/sessionStorage persistence', /\b(?:localStorage|sessionStorage|indexedDB)\b/],
  ['Business OS HTTP data path', /fetch\s*\(\s*['"]\/(?:api|rxdb|business-os)/],
  ['direct business_commands write', /collection\s*\(\s*['"]business_commands['"]\s*\)|business_commands[\s\S]{0,120}\b(?:insert|upsert|bulk)\s*\(/],
  ['upstream rxdb import', /from\s+['"]rxdb['"]/],
  ['CommonJS require', /\brequire\s*\(/],
  ['Node runtime import', /from\s+['"]node:/],
  ['remote URL dependency', /https?:\/\/|cdn\./],
  ['React framework runtime', /\bReact(?:DOM)?\.|\bcreateRoot\s*\(|from\s+['"][^'"]*react(?:\/|['"])/i],
  ['Vue framework runtime', /\bVue\.|\bcreateApp\s*\(|from\s+['"][^'"]*vue(?:\/|['"])/i],
  ['Svelte framework runtime', /from\s+['"][^'"]*svelte(?:\/|['"])/i],
  ['Angular framework runtime', /from\s+['"][^'"]*@angular(?:\/|['"])/i],
  ['JSX runtime marker', /jsx-runtime|\/\*\s*@jsx/i],
  ['legacy shell event dispatch', /window\.dispatchEvent\s*\(|ctox-business-os-chat-submit/],
];
for (const path of runtimeFiles) {
  const text = readText(path);
  for (const [label, regex] of runtimeRules) {
    if (regex.test(text)) fail(`${rel(path)} contains forbidden runtime pattern: ${label}`);
  }
}

if (/<!doctype\b|<\s*html\b|<\s*head\b|<\s*body\b/i.test(indexHtml)) {
  fail('index.html must be a shell fragment, not a full HTML document');
}
if (/<\s*(?:link|script|meta|title|style)\b/i.test(indexHtml)) {
  fail('index.html must not include document/head resource tags such as <link>, <script>, <meta>, <title>, or <style>');
}
for (const action of htmlDataActions(indexHtml)) {
  if (!indexJsHandlesDataAction(indexJs, action)) {
    fail(`index.html declares data-action="${action}" but index.js has no visible handler for it`);
  }
}

if (manifest && installedMode) {
  const declaredCollections = new Set(Array.isArray(manifest.collections) ? manifest.collections : []);
  const schemaCollections = new Set(Object.keys(schemaDoc?.collections || {}));
  for (const path of runtimeFiles.filter((file) => /\.(?:js|mjs)$/.test(file))) {
    for (const literal of collectStringLiterals(readText(path))) {
      const value = String(literal || '').trim();
      const lower = value.toLowerCase();
      if (lower === 'business_commands') {
        fail(`${rel(path)} references shell collection business_commands; app code must use ctx.commandBus.dispatch`);
      }
      if (
        lower.startsWith(`${moduleId.toLowerCase()}_`)
        && !declaredCollections.has(value)
        && !declaredCollections.has(lower)
      ) {
        fail(`${rel(path)} references module collection ${value}, but module.json does not declare it`);
      }
      if (declaredCollections.has(value) && !shellCollections.has(value) && !schemaCollections.has(value)) {
        fail(`${rel(path)} references module collection ${value}, but collections.schema.json does not define it`);
      }
    }
  }
}

if (failures.length > 0) {
  console.error(`Business OS module static check failed for ${moduleId}:`);
  for (const message of failures) console.error(`- ${message}`);
  process.exit(1);
}

console.log(`Business OS module static check OK: ${moduleId} (${installedMode ? 'installed' : 'source'} mode)`);
