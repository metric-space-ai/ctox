// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const CUSTOMER_RUNTIME_PATH = /(?:^|\/)(?:installed-modules|local-modules)(?:\/|$)/;
const CUSTOMER_WORK_PATH = /^runtime\/(?:rem|customer)(?:-|\/)/i;
const PUBLIC_SCOPE_VALUES = new Set(['public', 'global', 'system', 'store', 'internal', 'shared']);

export function customerReleaseViolations(paths, readText) {
  const violations = [];
  for (const rawPath of paths) {
    const path = String(rawPath || '').replaceAll('\\', '/');
    if (!path) continue;
    if (CUSTOMER_RUNTIME_PATH.test(path) || CUSTOMER_WORK_PATH.test(path)) {
      violations.push(`${path}: customer/runtime application state is tracked`);
      continue;
    }
    const match = /^src\/apps\/business-os\/modules\/([^/]+)\/module\.json$/.exec(path);
    if (!match) continue;
    let manifest;
    try {
      manifest = JSON.parse(readText(path));
    } catch {
      violations.push(`${path}: manifest is not valid JSON`);
      continue;
    }
    const moduleId = String(manifest.id || match[1]).toLowerCase();
    const declarations = [manifest.distribution, manifest.audience, manifest.visibility]
      .filter((value) => value !== undefined && value !== null);
    const invalidOrPrivateScope = declarations.some((value) => (
      typeof value !== 'string'
      || value.trim().length === 0
      || !PUBLIC_SCOPE_VALUES.has(value.trim().toLowerCase())
    ));
    const customerId = manifest.customer_id ?? manifest.customerId;
    const hasCustomerId = customerId !== undefined && customerId !== null;
    if (
      moduleId.startsWith('rem-')
      || moduleId.startsWith('customer-')
      || invalidOrPrivateScope
      || hasCustomerId
    ) {
      violations.push(`${path}: customer module is placed in the global module tree`);
    }
  }
  return violations;
}

export function assertCustomerAppIsolation(repoRoot) {
  const gitPaths = (args) => {
    try {
      return execFileSync('git', args, { cwd: repoRoot })
        .toString('utf8')
        .split('\0')
        .filter(Boolean);
    } catch {
      return [];
    }
  };
  const tracked = gitPaths(['ls-files', '-z']);
  const untracked = gitPaths(['ls-files', '--others', '--exclude-standard', '-z']);
  const paths = [...new Set([...tracked, ...untracked])];
  const trackedSet = new Set(tracked);
  const readText = (path) => {
    if (trackedSet.has(path)) {
      try {
        // Review the exact staged blob, not a potentially different worktree
        // file. This closes the stage-safe/worktree-private mismatch.
        return execFileSync('git', ['show', `:${path}`], { cwd: repoRoot }).toString('utf8');
      } catch {
        // No index (release archive) or an unstaged-only path: fall through.
      }
    }
    return readFileSync(resolve(repoRoot, path), 'utf8');
  };
  const globalModulesRoot = resolve(repoRoot, 'src/apps/business-os/modules');
  if (existsSync(globalModulesRoot)) {
    for (const entry of readdirSync(globalModulesRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const relative = `src/apps/business-os/modules/${entry.name}/module.json`;
      if (existsSync(resolve(repoRoot, relative)) && !paths.includes(relative)) paths.push(relative);
    }
  }
  const violations = customerReleaseViolations(paths, readText);
  if (violations.length > 0) {
    throw new Error(`Customer app isolation failed:\n- ${violations.join('\n- ')}`);
  }
}

const isMain = process.argv[1]
  && realpathSync(resolve(process.argv[1])) === realpathSync(fileURLToPath(import.meta.url));
if (isMain) {
  const repoRoot = resolve(process.argv[2] || '.');
  assertCustomerAppIsolation(repoRoot);
  console.log('customer app isolation: ok');
}
