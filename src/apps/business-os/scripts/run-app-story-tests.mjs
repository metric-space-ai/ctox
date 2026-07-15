#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadBusinessOsAppInventory } from './business-os-app-inventory.mjs';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '../../..');
const outputPath = process.env.BUSINESS_OS_APP_STORY_TEST_REPORT
  || join(repoRoot, 'output/playwright/business-os-app-story-tests.json');
const inventory = loadBusinessOsAppInventory();
const apps = inventory.sourceApps.map((app) => {
  const moduleRoot = join(appRoot, 'modules', app.id);
  const testFiles = walk(moduleRoot)
    .filter((path) => path.endsWith('.test.mjs') || path.endsWith('/test.mjs'))
    .sort();
  return {
    id: app.id,
    title: app.title,
    testFiles,
  };
});

const missing = apps.filter((app) => app.testFiles.length === 0).map((app) => app.id);
if (missing.length) {
  throw new Error(`Source apps without an executable story test: ${missing.join(', ')}`);
}

const testFiles = [...new Set(apps.flatMap((app) => app.testFiles))].sort();
const startedAt = new Date().toISOString();
const run = spawnSync(process.execPath, ['--test', ...testFiles], {
  cwd: repoRoot,
  encoding: 'utf8',
  stdio: ['ignore', 'pipe', 'pipe'],
  maxBuffer: 32 * 1024 * 1024,
});
const endedAt = new Date().toISOString();
const stdout = String(run.stdout || '');
const stderr = String(run.stderr || '');
const summary = parseNodeTestSummary(`${stdout}\n${stderr}`);
const report = {
  schema: 'ctox.business_os.app_story_tests.v1',
  started_at: startedAt,
  ended_at: endedAt,
  ok: run.status === 0 && apps.length === 34 && missing.length === 0,
  source_app_count: apps.length,
  system_app_count: inventory.coreApps.length,
  test_file_count: testFiles.length,
  node_test_summary: summary,
  apps: apps.map((app) => ({
    id: app.id,
    title: app.title,
    test_files: app.testFiles.map((path) => relative(repoRoot, path)),
  })),
};

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
process.stdout.write(stdout);
process.stderr.write(stderr);
console.log(`Business OS app story tests: ${apps.length}/34 apps, ${testFiles.length} test files, ${summary.pass ?? '?'} passed.`);
console.log(`Report: ${outputPath}`);
if (!report.ok) process.exit(run.status || 1);

function walk(root, out = []) {
  if (!existsSync(root)) return out;
  for (const name of readdirSync(root)) {
    const path = join(root, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}

function parseNodeTestSummary(output) {
  const read = (label) => {
    const match = output.match(new RegExp(`(?:^|\\n)[\\s#ℹ]*${label}\\s+(\\d+)`, 'i'));
    return match ? Number(match[1]) : null;
  };
  return {
    tests: read('tests'),
    pass: read('pass'),
    fail: read('fail'),
    skipped: read('skipped'),
    duration_ms: Number(output.match(/duration_ms\s+([0-9.]+)/i)?.[1] || 0),
  };
}
