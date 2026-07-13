#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../../..');

const CRITICAL_ASSETS = Object.freeze([
  'index.html',
  'app.js',
  'modules/registry.json',
  'shared/window-manager.js',
  'shared/business-chat.js',
  'shared/shell-chat-composition.js',
  'modules/desktop/index.js',
  'modules/desktop/iconDrag.js',
  'modules/desktop/index.css',
  'modules/threads/index.css',
  'modules/matching/index.css',
  'modules/invoices/icon.svg',
  'modules/documents/index.js',
  'modules/spreadsheets/index.js',
  'office-engine/features.json',
  'vendor/ctox-office/ctox-office-document.mjs',
  'vendor/ctox-office/ctox-office-spreadsheet.mjs',
  'vendor/ctox-office/frame.html',
  'vendor/ctox-office/provenance.json',
  'vendor/ctox-office/runtime/ctox-documents.mjs',
  'vendor/ctox-office/runtime/ctox-spreadsheets.mjs',
  'vendor/ctox-office/forks/ctox-documents/manifest.json',
  'vendor/ctox-office/forks/ctox-documents/business-os.css',
  'vendor/ctox-office/forks/ctox-spreadsheets/manifest.json',
  'vendor/ctox-office/forks/ctox-spreadsheets/business-os.css',
  'vendor/ctox-office/forks/shared/business-os.css',
  'vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html',
  'vendor/ctox-office/upstream/web-apps/apps/spreadsheeteditor/main/index.html',
  'vendor/ctox-office/upstream/sdkjs/word/sdk-all-min.js',
  'vendor/ctox-office/upstream/sdkjs/cell/sdk-all-min.js',
]);

const options = parseArgs(process.argv.slice(2));
if (options.selfTest) {
  runSelfTest();
  process.exit(0);
}
const sourceRoot = path.resolve(options.sourceRoot || repoRoot);
const sourceBusinessOs = path.join(sourceRoot, 'src/apps/business-os');
const installRoot = path.resolve(options.installRoot || path.join(homedir(), '.local/lib/ctox'));
const currentRoot = options.currentRoot
  ? path.resolve(options.currentRoot)
  : resolveCurrentRoot(installRoot);
const stateRoot = path.resolve(options.stateRoot || inferStateRoot(currentRoot, sourceRoot));
const managedBusinessOs = path.join(stateRoot, 'business-os');
const currentBusinessOs = resolveCurrentBusinessOsRoot(currentRoot);

const failures = [];
const checks = [];

assertDir(sourceBusinessOs, 'source Business OS root');
assertDir(currentRoot, 'managed current release root');
assertDir(managedBusinessOs, 'managed state Business OS root');
assertPath(currentBusinessOs, 'current release Business OS app root');

if (existsSync(currentBusinessOs)) {
  const stat = lstatSync(currentBusinessOs);
  checks.push({
    name: 'current_business_os_path',
    path: currentBusinessOs,
    symlink: stat.isSymbolicLink(),
    realpath: realpathSafe(currentBusinessOs),
  });
}

for (const rel of CRITICAL_ASSETS) {
  compareAsset(rel);
}

checkInstalledModulesPreserved();

const httpResult = options.url ? await checkHttpBuild(options.url) : null;
if (httpResult) checks.push(httpResult);

const report = {
  ok: failures.length === 0,
  sourceRoot,
  installRoot,
  currentRoot,
  stateRoot,
  managedBusinessOs,
  currentBusinessOs,
  checkedAssets: CRITICAL_ASSETS,
  checks,
  failures,
};

if (options.json) {
  console.log(JSON.stringify(report, null, 2));
} else if (report.ok) {
  console.log(`Managed Business OS asset guard OK: ${CRITICAL_ASSETS.length} critical assets`);
} else {
  console.error(`Managed Business OS asset guard failed with ${failures.length} issue(s):`);
  for (const failure of failures) console.error(`- ${failure}`);
}

process.exit(report.ok ? 0 : 1);

function compareAsset(rel) {
  const source = path.join(sourceBusinessOs, rel);
  const managed = path.join(managedBusinessOs, rel);
  const current = path.join(currentBusinessOs, rel);

  if (!existsSync(source)) {
    failures.push(`source asset is missing: ${rel}`);
    return;
  }
  if (!existsSync(managed)) {
    failures.push(`managed state asset is missing: ${rel}`);
    return;
  }
  if (!existsSync(current)) {
    failures.push(`current release asset is missing: ${rel}`);
    return;
  }

  const sourceHash = sha256File(source);
  const managedHash = sha256File(managed);
  const currentHash = sha256File(current);
  checks.push({
    name: 'asset',
    rel,
    sourceHash,
    managedHash,
    currentHash,
    ok: sourceHash === managedHash && sourceHash === currentHash,
  });
  if (sourceHash !== managedHash) {
    failures.push(`managed state asset drift: ${rel} (${short(sourceHash)} != ${short(managedHash)})`);
  }
  if (sourceHash !== currentHash) {
    failures.push(`current release asset drift: ${rel} (${short(sourceHash)} != ${short(currentHash)})`);
  }
}

function checkInstalledModulesPreserved() {
  const installedModules = path.join(managedBusinessOs, 'installed-modules');
  if (!existsSync(installedModules)) {
    checks.push({ name: 'installed_modules_preserved', ok: true, present: false });
    return;
  }
  const sourceInstalledModules = path.join(sourceBusinessOs, 'installed-modules');
  const samePath = realpathSafe(installedModules) === realpathSafe(sourceInstalledModules);
  checks.push({
    name: 'installed_modules_preserved',
    ok: !samePath,
    present: true,
    path: installedModules,
    sourcePath: sourceInstalledModules,
  });
  if (samePath) {
    failures.push('managed installed-modules points at source installed-modules; runtime-installed apps must stay tenant state');
  }
}

async function checkHttpBuild(url) {
  const target = new URL('/business-os/app.js', url).toString();
  try {
    const response = await fetch(`${target}?asset-guard=${Date.now()}`, { cache: 'no-store' });
    const body = await response.text();
    const match = body.match(/APP_BUILD\s*=\s*['"]([^'"]+)['"]/);
    const sourceBody = readFileSync(path.join(sourceBusinessOs, 'app.js'), 'utf8');
    const sourceMatch = sourceBody.match(/APP_BUILD\s*=\s*['"]([^'"]+)['"]/);
    const ok = response.ok && match?.[1] && match[1] === sourceMatch?.[1];
    if (!ok) {
      failures.push(`HTTP app.js build drift at ${target}: source=${sourceMatch?.[1] || '<missing>'} served=${match?.[1] || '<missing>'} status=${response.status}`);
    }
    return {
      name: 'http_app_build',
      ok,
      url: target,
      status: response.status,
      sourceBuild: sourceMatch?.[1] || '',
      servedBuild: match?.[1] || '',
    };
  } catch (error) {
    failures.push(`HTTP app.js check failed at ${target}: ${error?.message || error}`);
    return {
      name: 'http_app_build',
      ok: false,
      url: target,
      error: String(error?.message || error),
    };
  }
}

function parseArgs(args) {
  const parsed = {
    sourceRoot: '',
    installRoot: '',
    stateRoot: '',
    currentRoot: '',
    url: '',
    json: false,
    selfTest: false,
  };
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === '--json') parsed.json = true;
    else if (arg === '--self-test') parsed.selfTest = true;
    else if (arg === '--source-root') parsed.sourceRoot = args[++i] || '';
    else if (arg === '--install-root') parsed.installRoot = args[++i] || '';
    else if (arg === '--state-root') parsed.stateRoot = args[++i] || '';
    else if (arg === '--current-root') parsed.currentRoot = args[++i] || '';
    else if (arg === '--url') parsed.url = args[++i] || '';
    else if (arg === '--help' || arg === '-h') {
      console.log([
        'Usage: node src/apps/business-os/scripts/assert-managed-business-os-assets.mjs [options]',
        '',
        'Options:',
        '  --source-root <path>   Repository/source root (default: cwd repo root)',
        '  --install-root <path>  Managed install root (default: ~/.local/lib/ctox)',
        '  --state-root <path>    Persistent state root (default: <current>/runtime realpath)',
        '  --current-root <path>  Active release root (default: <install-root>/current realpath)',
        '  --url <url>            Optional running Business OS URL to verify served APP_BUILD',
        '  --json                 Print machine-readable report',
        '  --self-test            Verify clean-copy acceptance and Office-asset drift rejection',
      ].join('\n'));
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function runSelfTest() {
  const root = mkdtempSync(path.join(process.env.TMPDIR || '/tmp', 'ctox-managed-assets-'));
  const sourceRoot = path.join(root, 'source');
  const currentRoot = path.join(root, 'current');
  const stateRoot = path.join(root, 'state');
  const roots = [
    path.join(sourceRoot, 'src/apps/business-os'),
    path.join(currentRoot, 'src/apps/business-os'),
    path.join(stateRoot, 'business-os'),
  ];
  try {
    for (const businessRoot of roots) {
      for (const rel of CRITICAL_ASSETS) {
        const target = path.join(businessRoot, rel);
        mkdirSync(path.dirname(target), { recursive: true });
        writeFileSync(target, `managed-asset:${rel}\n`);
      }
    }
    const args = [
      fileURLToPath(import.meta.url),
      '--source-root', sourceRoot,
      '--current-root', currentRoot,
      '--state-root', stateRoot,
      '--json',
    ];
    const clean = spawnSync(process.execPath, args, { encoding: 'utf8' });
    if (clean.status !== 0) throw new Error(`clean managed-asset fixture failed: ${clean.stderr || clean.stdout}`);
    const report = JSON.parse(clean.stdout);
    if (!report.ok || report.checkedAssets.length !== CRITICAL_ASSETS.length) {
      throw new Error('clean managed-asset fixture returned incomplete evidence');
    }

    const officeEntry = 'vendor/ctox-office/ctox-office-document.mjs';
    writeFileSync(path.join(stateRoot, 'business-os', officeEntry), 'tampered\n');
    const drift = spawnSync(process.execPath, args, { encoding: 'utf8' });
    if (drift.status === 0 || !`${drift.stdout}\n${drift.stderr}`.includes(`managed state asset drift: ${officeEntry}`)) {
      throw new Error('managed Office asset drift was not rejected');
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
  console.log(`Managed Business OS asset guard self-test OK: ${CRITICAL_ASSETS.length} critical assets`);
}

function resolveCurrentRoot(root) {
  const current = path.join(root, 'current');
  if (!existsSync(current)) return current;
  return realpathSafe(current);
}

function resolveCurrentBusinessOsRoot(root) {
  const candidates = [
    path.join(root, 'business-os'),
    path.join(root, 'src/apps/business-os'),
  ];
  return candidates.find((candidate) => existsSync(path.join(candidate, 'index.html')))
    || candidates[0];
}

function inferStateRoot(current, source) {
  const runtime = path.join(current, 'runtime');
  if (existsSync(runtime)) return realpathSafe(runtime);
  return path.join(source, 'runtime');
}

function assertDir(value, label) {
  if (!existsSync(value) || !lstatSync(value).isDirectory()) {
    failures.push(`${label} is missing or not a directory: ${value}`);
  }
}

function assertPath(value, label) {
  if (!existsSync(value)) failures.push(`${label} is missing: ${value}`);
}

function sha256File(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

function short(hash) {
  return String(hash || '').slice(0, 12);
}

function realpathSafe(value) {
  try {
    return realpathSync(value);
  } catch {
    return path.resolve(value);
  }
}
