#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));

function usage() {
  return [
    'Usage: node src/apps/business-os/scripts/validate-app-module.mjs <module> [--source|--installed] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]',
    '',
    'Validates a CTOX Business OS app module artifact in source or installed mode.',
  ].join('\n');
}

function parseArgs(argv) {
  const result = {
    moduleId: null,
    mode: null,
    workspace: process.cwd(),
    json: false,
    skipTests: false,
    skipNodeCheck: false,
  };
  for (let idx = 0; idx < argv.length; idx += 1) {
    const arg = argv[idx];
    if (arg === '--source') {
      result.mode = 'source';
    } else if (arg === '--installed') {
      result.mode = 'installed';
    } else if (arg === '--workspace') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--workspace requires a path');
      result.workspace = value;
      idx += 1;
    } else if (arg === '--json') {
      result.json = true;
    } else if (arg === '--skip-tests') {
      result.skipTests = true;
    } else if (arg === '--skip-node-check') {
      result.skipNodeCheck = true;
    } else if (arg.startsWith('-')) {
      throw new Error(`unknown option: ${arg}`);
    } else if (!result.moduleId) {
      result.moduleId = arg;
    } else {
      throw new Error(`unexpected argument: ${arg}`);
    }
  }
  if (!result.moduleId || /[\\/]/.test(result.moduleId) || result.moduleId === '.' || result.moduleId === '..') {
    throw new Error('module id is required and must be a single path segment');
  }
  result.workspace = resolve(result.workspace);
  return result;
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

function rel(workspace, path) {
  return relative(workspace, path).split(sep).join('/');
}

function resolveStaticChecker(workspace) {
  const candidates = [
    resolve(scriptDir, '../../../skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs'),
    join(workspace, 'src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs'),
  ];
  return Array.from(new Set(candidates)).find((candidate) => existsSync(candidate));
}

function installedAppRootFor(workspace) {
  const runtimeAppRoot = join(workspace, 'runtime/business-os');
  if (existsSync(join(workspace, 'runtime')) || existsSync(runtimeAppRoot)) {
    return runtimeAppRoot;
  }
  return join(workspace, 'business-os');
}

function moduleDirFor(workspace, moduleId, mode) {
  if (mode === 'installed') {
    return join(installedAppRootFor(workspace), 'installed-modules', moduleId);
  }
  return join(workspace, 'src/apps/business-os/modules', moduleId);
}

function runNode(args, cwd) {
  return spawnSync(process.execPath, args, {
    cwd,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
}

function runNodeWithInput(args, cwd, input) {
  return spawnSync(process.execPath, args, {
    cwd,
    input,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
}

function outputLines(text) {
  return String(text || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
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

function collectRootArtifactFailures(workspace) {
  if (!existsSync(workspace)) return [];
  return readdirSync(workspace)
    .filter((name) => forbiddenRootAppArtifactName(name))
    .map((name) => join(workspace, name))
    .filter((path) => statSync(path).isFile())
    .map((path) => `root-level app artifact is forbidden: ${rel(workspace, path)}`);
}

function collectStaticFailures(stderr) {
  const lines = outputLines(stderr);
  const bulletLines = lines
    .filter((line) => line.startsWith('- '))
    .map((line) => line.slice(2));
  if (bulletLines.length > 0) return bulletLines;
  return lines;
}

function validate(options) {
  const mode = options.mode || (
    existsSync(moduleDirFor(options.workspace, options.moduleId, 'source'))
      ? 'source'
      : 'installed'
  );
  const moduleDir = moduleDirFor(options.workspace, options.moduleId, mode);
  const failures = [];
  const checks = [];

  failures.push(...collectRootArtifactFailures(options.workspace));

  const staticChecker = resolveStaticChecker(options.workspace);
  if (!staticChecker) {
    failures.push('module static checker is not available in this workspace or release image');
    checks.push({ name: 'module_static_check', ok: false, detail: 'missing checker' });
  } else {
    const args = [staticChecker, options.moduleId];
    if (mode === 'installed') args.push('--installed');
    const run = runNode(args, options.workspace);
    const ok = run.status === 0;
    checks.push({
      name: 'module_static_check',
      ok,
      exit_code: run.status,
      stdout: outputLines(run.stdout),
      stderr: outputLines(run.stderr),
    });
    if (!ok) {
      failures.push(...collectStaticFailures(run.stderr));
    }
  }

  const indexJs = join(moduleDir, 'index.js');
  if (!options.skipNodeCheck) {
    if (!existsSync(indexJs)) {
      failures.push(`missing ${rel(options.workspace, indexJs)} for node --check`);
      checks.push({ name: 'node_check', ok: false, detail: 'missing index.js' });
    } else {
      const run = runNodeWithInput(
        ['--check', '--input-type=module', '-'],
        options.workspace,
        readFileSync(indexJs, 'utf8'),
      );
      const ok = run.status === 0;
      checks.push({
        name: 'node_check',
        ok,
        exit_code: run.status,
        stdout: outputLines(run.stdout),
        stderr: outputLines(run.stderr),
      });
      if (!ok) {
        failures.push(`node --check failed for ${rel(options.workspace, indexJs)}: ${outputLines(run.stderr).join(' ')}`);
      }
    }
  }

  if (!options.skipTests) {
    const testDir = join(moduleDir, 'tests');
    const testFiles = walk(testDir).filter((path) => path.endsWith('.test.mjs'));
    if (testFiles.length === 0) {
      failures.push(`missing ${rel(options.workspace, testDir)}/*.test.mjs`);
      checks.push({ name: 'module_tests', ok: false, detail: 'missing tests' });
    } else {
      for (const testFile of testFiles) {
        const run = runNode([testFile], options.workspace);
        const ok = run.status === 0;
        checks.push({
          name: `module_test:${rel(options.workspace, testFile)}`,
          ok,
          exit_code: run.status,
          stdout: outputLines(run.stdout),
          stderr: outputLines(run.stderr),
        });
        if (!ok) {
          failures.push(`module test failed: ${rel(options.workspace, testFile)}: ${outputLines(run.stderr).join(' ')}`);
        }
      }
    }
  }

  return {
    ok: failures.length === 0,
    module_id: options.moduleId,
    mode,
    module_dir: rel(options.workspace, moduleDir),
    failures: Array.from(new Set(failures)),
    checks,
  };
}

let options;
try {
  options = parseArgs(process.argv.slice(2));
} catch (error) {
  console.error(error.message);
  console.error(usage());
  process.exit(2);
}

const result = validate(options);
if (options.json) {
  console.log(JSON.stringify(result, null, 2));
} else if (result.ok) {
  console.log(`Business OS app artifact validation OK: ${result.module_id} (${result.mode} mode)`);
} else {
  console.error(`Business OS app artifact validation failed for ${result.module_id} (${result.mode} mode):`);
  for (const failure of result.failures) {
    console.error(`- ${failure}`);
  }
  console.error(`Repair these files under ${result.module_dir}/ and rerun validation.`);
}

process.exit(result.ok ? 0 : 1);
