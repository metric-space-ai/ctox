#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

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

const repoRoot = process.cwd();
const sourceModuleDir = join(repoRoot, 'src/apps/business-os/modules', moduleId);
function installedAppRootFor(root) {
  const runtimeAppRoot = join(root, 'runtime/business-os');
  if (existsSync(join(root, 'runtime')) || existsSync(runtimeAppRoot)) {
    return runtimeAppRoot;
  }
  return join(root, 'business-os');
}
const installedModuleDir = join(installedAppRootFor(repoRoot), 'installed-modules', moduleId);
const installedMode = modeArg === '--installed' || (!existsSync(sourceModuleDir) && existsSync(installedModuleDir));
const moduleDir = installedMode ? installedModuleDir : sourceModuleDir;
const expectedEntry = installedMode
  ? `installed-modules/${moduleId}/index.html`
  : `modules/${moduleId}/index.html`;
const expectedInstallScope = installedMode ? 'installed' : 'store';
const planPath = join(repoRoot, 'docs', `business-os-${moduleId}-implementation-plan.md`);
const registryPath = join(repoRoot, 'src/apps/business-os/modules/registry.json');
const failures = [];

const shellCollections = new Set([
  'business_module_catalog',
  'ctox_runtime_settings',
  'business_commands',
  'ctox_queue_tasks',
]);

const shellTokenNames = [
  'bg', 'surface', 'surface-2', 'line', 'text', 'text-strong', 'muted',
  'accent', 'accent-soft', 'danger', 'hairline', 'panel-radius',
  'control-radius', 'panel-shadow', 'glass-bg', 'glass-blur',
  'font-sans', 'font-mono',
  'line-strong', 'success', 'success-soft', 'warning', 'warning-soft',
  'danger-soft', 'focus-ring',
];
const shellTokenPattern = new RegExp(
  `--(?:${shellTokenNames.join('|')})(?![\\w-])\\s*:`,
);
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function fail(message) {
  failures.push(message);
}

function rel(path) {
  return relative(repoRoot, path).split(sep).join('/');
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`${rel(path)} is not valid JSON: ${error.message}`);
    return null;
  }
}

function parseSemver(value) {
  if (typeof value !== 'string') return null;
  const match = semverPattern.exec(value.trim());
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    const stats = statSync(path);
    if (stats.isDirectory()) {
      walk(path, out);
    } else {
      out.push(path);
    }
  }
  return out;
}

function walkEntries(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    out.push(path);
    if (statSync(path).isDirectory()) {
      walkEntries(path, out);
    }
  }
  return out;
}

function hasPathSegment(path, segment) {
  return path.split(sep).includes(segment);
}

function fetchCallSnippet(text, index) {
  return text.slice(index, index + 180).replace(/\s+/g, ' ').trim();
}

function isAllowedInstalledModuleFetch(snippet) {
  return /^fetch\s*\(\s*new\s+URL\s*\(\s*['"]\.\/index\.html['"]\s*,\s*import\.meta\.url\s*\)/.test(snippet);
}

function rootModuleDirLooksLikeOnlyCtoxLedger(path) {
  if (moduleId !== 'contracts') return false;
  const ledgerPath = join(path, 'history', 'creation-ledger.md');
  if (!existsSync(ledgerPath)) return false;
  const appArtifactNames = new Set([
    'module.json',
    'collections.schema.json',
    'schema.js',
    'index.html',
    'index.css',
    'index.js',
    'icon.svg',
  ]);
  return !walk(path).some((file) => {
    const relativePath = rel(file);
    const basename = relativePath.split('/').pop();
    return appArtifactNames.has(basename)
      || relativePath.includes('/locales/')
      || relativePath.includes('/tests/');
  });
}

function forbiddenRootAppArtifactName(name) {
  const lower = String(name || '').toLowerCase();
  return lower === 'module.json'
    || lower === 'collections.schema.json'
    || lower.startsWith('_test_')
    || lower.startsWith('_probe_')
    || lower.startsWith('test-')
    || lower.startsWith('probe-')
    || lower.includes('-test.')
    || lower.includes('_test.')
    || lower.includes('-probe.')
    || lower.includes('_probe.')
    || lower.endsWith('-module.json')
    || lower.endsWith('_module.json')
    || lower.endsWith('.module.json')
    || lower.endsWith('-collections.schema.json')
    || lower.endsWith('_collections.schema.json')
    || lower.endsWith('.collections.schema.json')
    || lower === 'artifact-status.md'
    || lower.endsWith('-artifact-status.md')
    || lower.endsWith('_artifact_status.md')
    || lower.endsWith('-blocker.md')
    || lower.endsWith('_blocker.md');
}

function forbiddenModuleArtifactName(name) {
  const lower = String(name || '').toLowerCase();
  return (lower.endsWith('.md') && lower !== 'readme.md')
    || lower.startsWith('harness_')
    || lower.startsWith('harness-')
    || lower.includes('_harness_')
    || lower.includes('-harness-')
    || lower.includes('artifact_conflict')
    || lower.includes('artifact-conflict')
    || lower.includes('artifact_status')
    || lower.includes('artifact-status')
    || lower.includes('blocker')
    || lower.includes('probe');
}

if (!existsSync(moduleDir)) {
  fail(`${rel(moduleDir)} does not exist`);
}

for (const name of readdirSync(repoRoot)) {
  const path = join(repoRoot, name);
  if (statSync(path).isFile() && forbiddenRootAppArtifactName(name)) {
    fail(`root-level app artifact is forbidden: ${rel(path)}`);
  }
}
const rootModuleDir = join(repoRoot, moduleId);
if (existsSync(rootModuleDir) && !rootModuleDirLooksLikeOnlyCtoxLedger(rootModuleDir)) {
  fail(`root-level module directory is forbidden: ${rel(rootModuleDir)}`);
}

for (const file of [
  'module.json',
  'collections.schema.json',
  'schema.js',
  'index.html',
  'index.css',
  'index.js',
  'icon.svg',
  'locales/de.json',
  'locales/en.json',
]) {
  if (!existsSync(join(moduleDir, file))) {
    fail(`missing ${rel(join(moduleDir, file))}`);
  }
}

if (!installedMode && !existsSync(join(moduleDir, 'README.md'))) {
  fail(`missing ${rel(join(moduleDir, 'README.md'))}`);
}

if (!installedMode && !existsSync(planPath)) {
  fail(`missing ${rel(planPath)}`);
}

const manifest = existsSync(join(moduleDir, 'module.json'))
  ? readJson(join(moduleDir, 'module.json'))
  : null;
const schemaDoc = existsSync(join(moduleDir, 'collections.schema.json'))
  ? readJson(join(moduleDir, 'collections.schema.json'))
  : null;
const registry = existsSync(registryPath) ? readJson(registryPath) : null;

if (manifest) {
  if (manifest.id !== moduleId) fail(`module.json id must be ${moduleId}`);
  if (manifest.entry !== expectedEntry) {
    fail(`module.json entry must be ${expectedEntry}`);
  }
  if (manifest.install_scope !== expectedInstallScope) {
    fail(`module.json install_scope must be ${expectedInstallScope}`);
  }
  if (installedMode) {
    const parsedVersion = parseSemver(manifest.version);
    if (!parsedVersion) {
      fail('module.json version must be SemVer x.y.z without a v prefix for installed App Creator modules');
    } else if (parsedVersion.major === 0 && parsedVersion.minor === 0 && parsedVersion.patch === 0) {
      fail('module.json version 0.0.0 is not a valid Business OS app work version');
    }
  }
  if (!Array.isArray(manifest.collections)) {
    fail('module.json collections must be an array');
  }
  if (manifest.layout?.right && !manifest.layout?.third_pane_justification) {
    fail('module.json layout.right requires layout.third_pane_justification; use two panes plus modals/drawers by default');
  }
  if (manifest.layout?.icon_svg) {
    fail('module.json layout.icon_svg is forbidden; keep icons in icon.svg instead of embedding SVG in the manifest');
  }
  if (manifest.icon_svg || manifest.iconSvg) {
    fail('module.json inline icon fields are forbidden; keep icons in icon.svg instead of embedding SVG in the manifest');
  }
  const inlineManifestIcons = [
    manifest.layout?.icon,
    manifest.layout?.icon_svg,
    manifest.icon,
    manifest.icon_svg,
    manifest.iconSvg,
  ].filter((value) => typeof value === 'string');
  if (inlineManifestIcons.some((value) => /<\s*svg\b/i.test(value))) {
    fail('module.json must not embed inline SVG markup; keep SVG markup only in icon.svg');
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
    if (shellCollections.has(name)) {
      fail(`collections.schema.json redeclares shell collection ${name}`);
    }
  }
  for (const name of manifest.collections || []) {
    if (!shellCollections.has(name) && !schemaDoc.collections[name]) {
      fail(`collections.schema.json missing non-shell collection from module.json: ${name}`);
    }
  }
}

const schemaJsPath = join(moduleDir, 'schema.js');
if (existsSync(schemaJsPath)) {
  const schemaJsText = readFileSync(schemaJsPath, 'utf8');
  const shellCollectionKey = Array.from(shellCollections).find((name) => {
    const keyPattern = new RegExp(
      String.raw`(?:^|[,{]\s*)(?:['"]${name}['"]|${name})\s*:`,
      'm',
    );
    return keyPattern.test(schemaJsText);
  });
  if (shellCollectionKey) {
    fail(`schema.js exports shell-registered collection key ${shellCollectionKey}`);
  }
}

if (!installedMode && manifest && registry) {
  const entry = (registry.modules || []).find((item) => item.id === moduleId);
  if (!entry) {
    fail(`registry.json missing module ${moduleId}`);
  } else {
    if (entry.entry !== manifest.entry) fail(`registry entry mismatch for ${moduleId}: entry`);
    if (entry.install_scope !== manifest.install_scope) fail(`registry entry mismatch for ${moduleId}: install_scope`);
    for (const name of manifest.collections || []) {
      if (!(entry.collections || []).includes(name)) {
        fail(`registry entry missing collection ${name}`);
      }
    }
  }
}

const entries = walkEntries(moduleDir);
const files = entries.filter((path) => !statSync(path).isDirectory());
const testFiles = files.filter((path) =>
  hasPathSegment(path, 'tests') && path.endsWith('.test.mjs')
);

if (testFiles.length === 0) {
  fail(`missing ${rel(join(moduleDir, 'tests'))}/*.test.mjs`);
}

for (const path of entries) {
  const name = path.split(sep).at(-1);
  if (!statSync(path).isDirectory()) continue;
  if (
    name === 'node_modules' ||
    name === '.opencode' ||
    name === '.vite' ||
    name === '.parcel-cache' ||
    name === 'dist' ||
    name === 'build'
  ) {
    fail(`forbidden module artifact ${rel(path)}`);
  }
}

for (const path of files) {
  const name = path.split(sep).at(-1);
  if (
    name === '.DS_Store' ||
    name === 'Thumbs.db' ||
    name === 'package.json' ||
    name === 'package-lock.json' ||
    name === 'yarn.lock' ||
    name === 'pnpm-lock.yaml' ||
    name === 'bun.lockb' ||
    name === 'vite.config.js' ||
    name === 'vite.config.mjs' ||
    name === 'webpack.config.js' ||
    name === 'rollup.config.js' ||
    forbiddenModuleArtifactName(name) ||
    name?.startsWith('_probe_') ||
    name?.endsWith('.jsx') ||
    name?.endsWith('.tsx') ||
    name?.endsWith('.bundle.js') ||
    name?.endsWith('.bundle.mjs') ||
    name?.endsWith('.bundle.css') ||
    name?.endsWith('.bak') ||
    name?.endsWith('.orig') ||
    name?.endsWith('.rej') ||
    name?.endsWith('.tmp') ||
    hasPathSegment(path, 'node_modules')
  ) {
    fail(`forbidden module artifact ${rel(path)}`);
  }
}

for (const path of files.filter((file) => /\.(html|css|js|mjs)$/.test(file))) {
  const text = readFileSync(path, 'utf8');
  const isTestFile = hasPathSegment(path, 'tests') || path.endsWith('.test.mjs');
  if (isTestFile) {
    if (/data:text\/javascript/i.test(text)) {
      fail(`${rel(path)} imports local app source through a data: URL; test shared .mjs helpers and JSON/text parity instead`);
    }
    if (/(?:from\s+['"]\.\.\/(?:index|schema)\.js['"]|import\s*\(\s*(?:new\s+URL\s*\(\s*)?['"]\.\.\/(?:index|schema)\.js['"])/.test(text)) {
      fail(`${rel(path)} imports browser .js entrypoints directly; put testable logic/schemas in local .mjs helpers and import those helpers from tests`);
    }
    const testEvasionRules = [
      ['validator scanner-evasion String.fromCharCode', /\bString\.fromCharCode\s*\(/],
      ['validator scanner-evasion forbidden token list', /\b(?:bundlerTokens|thirdPaneTokens|forbiddenTokens?|legacyTokens?)\b/],
      ['validator scanner-evasion source absence assertion', /\bassert\.doesNotMatch\s*\(\s*\w*Source\b/],
      ['validator scanner-evasion anti-pattern source scan', /\bdoes\s+not\s+use\s+forbidden\b|\b(?:forbidden|legacy|anti[-_\s]?pattern)[\s\S]{0,160}\b(?:tokens?|literals?|patterns?|source|scan)\b/i],
      ['validator scanner-evasion workaround language', /\b(?:validator|checker|static checker)[\s\S]{0,160}\b(?:does not flag|bypass|workaround|evad|scan)\b/i],
    ];
    for (const [label, regex] of testEvasionRules) {
      if (regex.test(text)) {
        fail(`${rel(path)} contains forbidden ${label}; generated tests must assert positive Business OS behavior instead of scanning for anti-pattern absence`);
      }
    }
  }
  const thirdPanePatterns = [
    /\blayout\.right\b/,
    /\bdata-[\w-]*right\b/i,
    /\bclass=["'][^"']*\b[\w-]+-right\b/i,
    /\bright-column\b/i,
    /\bright\s+pane\b/i,
    /\bright[-_\s]?resizer\b/i,
    /grid-template-columns\s*:[^;]*(?:\b1fr\b[^;]*){1}[^;]*(?:\bvar\([^)]*right|right-width|minmax\([^)]*right)/i,
  ];
  if (
    !isTestFile &&
    thirdPanePatterns.some((regex) => regex.test(text)) &&
    !/third[-_\s]?pane[-_\s]?justification|persistent third pane/i.test(text)
  ) {
    fail(`${rel(path)} appears to define a third/right pane without an explicit workflow justification`);
  }
}

const runtimeRules = [
  ['ctx.db.raw/db.raw access', /ctx\??\.db\??\.raw|\bdb\??\.raw\b/],
  ['ctx.collections contract', /ctx\.collections/],
  ['ctox.db global handle', /ctox\.db/],
  ['localStorage/sessionStorage persistence', /\b(?:localStorage|sessionStorage)\b/],
  ['Business OS HTTP record API', /fetch\(\s*['"]\/api\/business-os/],
  ['legacy shell event dispatch', /window\.dispatchEvent\s*\(|ctox-business-os-chat-submit/],
  ['direct business_commands write fallback', /collection\s*\(\s*['"]business_commands['"]\s*\)|business_commands[\s\S]{0,120}\b(?:insert|upsert)\s*\(/],
  ['JSON module import', /\bimport\s+(?:[^'"]+\s+from\s+)?['"][^'"]+\.json['"]/],
  ['upstream rxdb import', /from\s+['"]rxdb['"]/],
  ['Node runtime import', /from\s+['"]node:/],
  ['CommonJS require', /\brequire\s*\(/],
  ['bare package import', /\bimport\s+(?:[^'"]+\s+from\s+)?['"](?![./])[^'"]+['"]/],
  ['bare dynamic import', /\bimport\s*\(\s*['"](?![./])[^'"]+['"]\s*\)/],
  ['HTML import map', /\bimportmap\b|type\s*=\s*['"]importmap['"]/i],
  ['remote URL import/reference', /https?:\/\/|cdn\./],
];

for (const path of files) {
  if (!/\.(js|mjs|html|css)$/.test(path)) continue;
  if (hasPathSegment(path, 'tests') || path.endsWith('.test.mjs')) continue;
  const text = readFileSync(path, 'utf8');
  for (const [label, regex] of runtimeRules) {
    if (regex.test(text)) {
      fail(`${rel(path)} contains forbidden runtime pattern: ${label}`);
    }
  }
  if (path.endsWith('index.css')) {
    const cssNoComments = text.replace(/\/\*[\s\S]*?\*\//g, '');
    for (const match of cssNoComments.matchAll(/(?:^|[};])\s*([^{};]+)\{([^{}]*)\}/g)) {
      const selector = match[1].trim();
      const body = match[2];
      const isPureRoot = /^:root(?:\[[^\]]*\])?(?:\s*,\s*(?:html|body|:root)(?:\[[^\]]*\])?)*$/.test(selector);
      if (isPureRoot && /--[\w-]+\s*:/.test(body)) {
        fail(`${rel(path)} defines custom properties on :root; scope module tokens under the module root class`);
      }
      if (shellTokenPattern.test(body)) {
        const token = body.match(shellTokenPattern)?.[0]?.replace(/\s*:$/, '');
        fail(`${rel(path)} redefines shell/base design token ${token}; use a module-local token name`);
      }
      for (const declaration of body.matchAll(/(--[\w-]+)\s*:\s*([^;{}]+);?/g)) {
        const token = declaration[1];
        const value = declaration[2];
        const selfReferencePattern = new RegExp(String.raw`\bvar\(\s*${escapeRegExp(token)}(?:\s*[,)]|\s*$)`);
        if (selfReferencePattern.test(value)) {
          fail(`${rel(path)} defines self-referential CSS custom property ${token}; use a shell-token fallback or literal module value`);
        }
      }
    }
  }
}

const broadScanFiles = files
  .filter((path) => /\.(js|mjs|html|css|json|md)$/.test(path))
  .concat(!installedMode && existsSync(planPath) ? [planPath] : []);

const broadRules = [
  ['forbidden data-plane literal /api/business-os', /\/api\/business-os/],
  ['forbidden data-plane literal /rxdb/pull', /\/rxdb\/pull/],
  ['forbidden data-plane literal /commands', /\/commands/],
  ['forbidden browser storage literal localStorage', /\blocalStorage\b/],
  ['forbidden browser storage literal sessionStorage', /\bsessionStorage\b/],
  ['forbidden data-plane literal local-only', /\blocal-only\b/],
  ['forbidden data-plane literal FallbackDatabase', /\bFallbackDatabase\b/],
  ['forbidden upstream rxdb literal', /from\s+['"]rxdb['"]|\bupstream rxdb\b/i],
  ['forbidden dependency literal esbuild', /\besbuild\b/i],
  ['forbidden dependency literal webpack', /\bwebpack\b/i],
  [
    'forbidden Rollup bundler dependency/config literal',
    /\b(?:rollup\.config|rollup-plugin|@rollup|rollupjs|rollup\s+(?:build|bundle|config|plugin)|from\s+['"][^'"]*rollup|import\s*\(\s*['"][^'"]*rollup)/i,
  ],
  ['forbidden dependency literal vite', /\bvite\b/i],
  ['forbidden dependency literal npm install', /\bnpm install\b/i],
  ['forbidden dependency literal npx', /\bnpx\b/i],
  ['forbidden dependency literal node_modules', /\bnode_modules\b/],
  ['forbidden dependency literal package-lock', /\bpackage-lock\b/],
  ['forbidden dependency literal package.json', /\bpackage\.json\b/],
  ['forbidden dependency literal importmap', /\bimportmap\b|\bimport map\b/i],
  ['forbidden schema test transform node:vm', /from\s+['"]node:vm['"]|from\s+['"]vm['"]/],
  ['forbidden schema test dynamic evaluator', /\bnew\s+Function\s*\(/],
  ['forbidden raw DB negative-proof literal', /ctx\.db\.raw|\bdb\.raw\b/],
  ['forbidden legacy shell chat event literal', /ctox-business-os-chat-submit|window\.dispatchEvent\s*\(/],
  ['forbidden command state literal pending_sync', /\bpending_sync\b/],
  ['forbidden direct command fallback literal', /business_commands\s+fallback|fallback\s+to\s+business_commands|falls?\s+back\s+to\s+business_commands|with\s+business_commands\s+fallback/i],
  ['forbidden alternate App Creator automation command ctox.business_os.ticket.followup.create', /\bctox\.business_os\.ticket\.followup\.create\b/],
  ['forbidden third-pane literal layout.right', /\blayout\.right\b/],
  ['forbidden third-pane literal right-resizer', /\bright[-_]?resizer\b/i],
  ['forbidden third-pane literal right-column', /\bright-column\b/i],
  ['forbidden third-pane literal data-*-right', /\bdata-[\w-]*right\b/i],
];

for (const path of broadScanFiles) {
  const text = readFileSync(path, 'utf8');
  for (const [label, regex] of broadRules) {
    if (regex.test(text)) {
      fail(`${rel(path)} contains ${label}`);
    }
  }
}

if (installedMode) {
  const runtimeTextFiles = files.filter((path) =>
    /\.(js|mjs|html|css)$/.test(path) && !hasPathSegment(path, 'tests')
  );
  const runtimeText = runtimeTextFiles
    .map((path) => readFileSync(path, 'utf8'))
    .join('\n');
  const allModuleText = files
    .filter((path) => /\.(js|mjs|html|css|json)$/.test(path))
    .map((path) => readFileSync(path, 'utf8'))
    .join('\n');
  const frameworkRules = [
    ['React framework runtime', /\bReact(?:DOM)?\.|\bcreateRoot\s*\(|from\s+['"][^'"]*react(?:\/|['"])/i],
    ['Vue framework runtime', /\bVue\.|\bcreateApp\s*\(|from\s+['"][^'"]*vue(?:\/|['"])/i],
    ['Svelte framework runtime', /from\s+['"][^'"]*svelte(?:\/|['"])/i],
    ['Angular framework runtime', /from\s+['"][^'"]*@angular(?:\/|['"])/i],
    ['Solid framework runtime', /from\s+['"][^'"]*solid-js(?:\/|['"])/i],
    ['Preact framework runtime', /from\s+['"][^'"]*preact(?:\/|['"])/i],
    ['Lit framework runtime', /from\s+['"][^'"]*lit(?:\/|['"])/i],
    ['JSX runtime marker', /jsx-runtime|\/\*\s*@jsx/i],
  ];
  for (const [label, regex] of frameworkRules) {
    if (regex.test(runtimeText)) {
      fail(`installed App Creator module must be vanilla HTML/CSS/browser ESM; found ${label}`);
    }
  }
  const installedRuntimeRules = [
    ['dynamic import', /\bimport\s*\(/],
    ['cached ctx.db facade handle', /\b(?:const|let|var)\s+[A-Za-z_$][\w$]*\s*=\s*(?:ctx|state\.ctx)\.db\b|\b(?:window|globalThis|this)\.[A-Za-z_$][\w$]*\s*=\s*(?:ctx|state\.ctx)\.db\b/],
    ['Business OS shell global state access', /\b(?:window|globalThis)\.(?:CTOX_BUSINESS_OS_APP|CTOX_BUSINESS_OS_STATUS|ctoxBusinessOsSmoke|openModuleSourceEditor|setStartupProgress|showStartupError|toggleStartMenu)\b/],
    ['direct CTOX control command', /\bctox\.(?:module|business_os|task|ticket|approval|runtime|outbound|agent)\b/],
    ['Worker runtime', /\b(?:new\s+Worker|new\s+SharedWorker|navigator\.serviceWorker)\b/],
    ['direct browser navigation', /\b(?:window\.open|location\.(?:assign|replace|href))\b/],
    ['dynamic evaluator', /\b(?:eval\s*\(|new\s+Function\s*\()/],
  ];
  for (const path of runtimeTextFiles) {
    const text = readFileSync(path, 'utf8');
    for (const match of text.matchAll(/\bfetch\s*\(/g)) {
      const snippet = fetchCallSnippet(text, match.index);
      if (!isAllowedInstalledModuleFetch(snippet)) {
        fail(`${rel(path)} contains forbidden installed-app network fetch; only fetch(new URL('./index.html', import.meta.url)) is allowed for the local template`);
      }
    }
    for (const [label, regex] of installedRuntimeRules) {
      if (regex.test(text)) {
        fail(`${rel(path)} contains forbidden installed-app runtime capability: ${label}`);
      }
    }
  }
  if (!/\b(?:ctx|state\.ctx)\.commandBus\.dispatch\s*\(/.test(runtimeText)) {
    fail('installed App Creator module must dispatch at least one real automation through ctx.commandBus.dispatch');
  }
  if (!/\bbusiness_os\.chat\.task\b/.test(allModuleText)) {
    fail('installed App Creator module must include a business_os.chat.task automation command');
  }
  if (!/(?:command_type\s*:\s*['"]business_os\.chat\.task['"]|["']command_type["']\s*:\s*["']business_os\.chat\.task["'])/.test(allModuleText)) {
    fail('installed App Creator module must preserve command_type: business_os.chat.task in its automation command');
  }
  if (!/\brecord_snapshot\b/.test(allModuleText)) {
    fail('installed App Creator module automation must include a source record_snapshot');
  }
  const indexJsPath = join(moduleDir, 'index.js');
  const indexJsText = existsSync(indexJsPath) ? readFileSync(indexJsPath, 'utf8') : '';
  if (!/fetch\s*\(\s*new\s+URL\s*\(\s*['"]\.\/index\.html['"]\s*,\s*import\.meta\.url\s*\)/.test(indexJsText)) {
    fail('installed App Creator module index.js must load ./index.html with fetch(new URL(\'./index.html\', import.meta.url)) before DOM wiring');
  }
  if (!/(?:ctx|state\.ctx)\.host\.innerHTML\s*=/.test(indexJsText)) {
    fail('installed App Creator module mount(ctx) must render index.html into ctx.host.innerHTML');
  }
  if (!/new\s+URL\s*\(\s*['"]\.\/index\.css['"]\s*,\s*import\.meta\.url\s*\)/.test(indexJsText)) {
    fail('installed App Creator module index.js must attach ./index.css through a local new URL(\'./index.css\', import.meta.url) stylesheet');
  }
}

if (failures.length > 0) {
  console.error(`Business OS module static check failed for ${moduleId}:`);
  for (const message of failures) {
    console.error(`- ${message}`);
  }
  process.exit(1);
}

console.log(`Business OS module static check OK: ${moduleId} (${installedMode ? 'installed' : 'source'} mode)`);
