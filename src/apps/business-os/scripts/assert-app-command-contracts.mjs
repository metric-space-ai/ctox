#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadBusinessOsAppInventory } from './business-os-app-inventory.mjs';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(scriptDir, '..');
const repoRoot = path.resolve(appRoot, '../../..');
const outputPath = process.env.BUSINESS_OS_APP_COMMAND_AUDIT
  || path.join(repoRoot, 'output/playwright/business-os-app-command-contracts.json');
const quality = JSON.parse(fs.readFileSync(path.join(appRoot, 'qa/app-quality-contracts.json'), 'utf8'));
const router = JSON.parse(fs.readFileSync(path.join(repoRoot, 'src/core/business_os/business_command_inventory.json'), 'utf8'));
const inventory = loadBusinessOsAppInventory();
const exactControlTypes = new Set(router.exact_control_types || []);
const predicateControlTypes = new Set(router.predicate_control_types || []);
const browserRuntimeTypes = new Set(router.browser_runtime_types || []);
const controlPrefixes = router.control_prefixes || [];
const forbiddenLegacyTypes = new Set(['business_os.command', 'business_os.test']);
const commandActions = new Set([
  'run', 'approve', 'delegate', 'handoff', 'release', 'install', 'rollback', 'reply',
  'erase', 'export', 'check', 'schedule', 'resume', 'cancel',
]);

const browserCommandTypes = router.browser_literal_types || [];
const knownCommandTypes = new Set(browserCommandTypes);
const apps = inventory.sourceApps.map((entry) => auditApp(entry));
const browserLegacySites = walk(appRoot)
  .filter((file) => /\.(?:js|mjs)$/.test(file))
  .filter((file) => !/\.(?:test|spec)\.(?:js|mjs)$/.test(file))
  .filter((file) => !file.includes(`${path.sep}tests${path.sep}`))
  .filter((file) => !file.includes(`${path.sep}vendor${path.sep}`))
  .filter((file) => !file.endsWith('/schema.js'))
  .flatMap((file) => findLegacyCommandProperties(fs.readFileSync(file, 'utf8')).map((site) => ({
    ...site,
    file: relative(appRoot, file),
  })));
const allowedCompatibilityAliases = browserLegacySites.filter((site) => (
  site.file === 'shared/command-bus.js' && site.value === 'ctox.command.cancel'
));
const forbiddenBrowserLegacySites = browserLegacySites.filter((site) => !allowedCompatibilityAliases.includes(site));
const routeInventory = {
  native_control: browserCommandTypes.filter((type) => nativeControlType(type)),
  browser_runtime: browserCommandTypes.filter((type) => browserRuntimeTypes.has(type)),
  harness_queue: browserCommandTypes.filter((type) => !nativeControlType(type) && !browserRuntimeTypes.has(type)),
};
const globalIssues = browserCommandTypes
  .filter((type) => forbiddenLegacyTypes.has(type))
  .map((type) => ({ app: '*', message: `forbidden legacy command type in browser source: ${type}` }))
  .concat(forbiddenBrowserLegacySites.map((site) => ({
    app: '*',
    message: `${site.file}:${site.line} registered command still uses legacy type property (${site.value})`,
  })));
const issues = [
  ...apps.flatMap((app) => app.issues.map((message) => ({ app: app.id, message }))),
  ...globalIssues,
];
const report = {
  schema: 'ctox.business_os.app_command_contract_audit.v1',
  generated_at: new Date().toISOString(),
  ok: issues.length === 0,
  authoritative_router: router.authoritative_router,
  route_inventory: routeInventory,
  compatibility_aliases: allowedCompatibilityAliases,
  apps,
  issues,
  totals: {
    apps: apps.length,
    direct_dispatch_apps: apps.filter((app) => app.direct_dispatch_sites > 0).length,
    context_action_apps: apps.filter((app) => app.context_action_sites > 0).length,
    business_chat_apps: apps.filter((app) => app.business_chat_sites > 0).length,
    apps_with_harness_routed_literals: apps.filter((app) => app.harness_routed_types.length > 0).length,
    dynamic_dispatch_sites: apps.reduce((sum, app) => sum + app.dynamic_dispatch_sites, 0),
    native_control_types: routeInventory.native_control.length,
    browser_runtime_types: routeInventory.browser_runtime.length,
    harness_queue_types: routeInventory.harness_queue.length,
    legacy_source_command_properties: apps.reduce((sum, app) => sum + app.legacy_source_command_properties, 0),
    forbidden_browser_legacy_properties: forbiddenBrowserLegacySites.length,
    issues: issues.length,
  },
};

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
console.log('Business OS per-app command contracts', report.totals);
console.log(`Report: ${outputPath}`);
if (!report.ok) {
  for (const issue of issues) console.error(`${issue.app}: ${issue.message}`);
  process.exitCode = 1;
}

function auditApp(entry) {
  const moduleDir = path.join(appRoot, 'modules', entry.id);
  const sources = walk(moduleDir)
    .filter((file) => /\.(?:js|mjs)$/.test(file))
    .filter((file) => !/\.(?:test|spec)\.(?:js|mjs)$/.test(file))
    .filter((file) => !file.endsWith('/schema.js'))
    .map((file) => ({ file, source: fs.readFileSync(file, 'utf8') }));
  const source = sources.map((item) => item.source).join('\n');
  const qualityContract = quality.apps.find((app) => app.id === entry.id);
  const inferredCommandActions = qualityContract?.required_actions?.filter((action) => commandActions.has(action)) || [];
  const requiredCommandActions = Array.isArray(qualityContract?.command_actions)
    ? qualityContract.command_actions
    : inferredCommandActions;
  const requiresCommand = requiredCommandActions.length > 0;
  const dispatchCalls = sources.flatMap(({ file, source: fileSource }) => (
    extractDispatchCalls(fileSource).map((call) => ({ file: relative(moduleDir, file), call }))
  ));
  const literalTypes = [...new Set(dispatchCalls.flatMap(({ call }) => call.types))].sort();
  const legacyTypes = literalTypes.filter((type) => forbiddenLegacyTypes.has(type));
  const conflicts = dispatchCalls.filter(({ call }) => call.conflict);
  const legacyOnlySites = dispatchCalls.filter(({ call }) => call.legacy.length > 0 && call.canonical.length === 0);
  const redundantAliasSites = dispatchCalls.filter(({ call }) => call.legacy.length > 0 && call.canonical.length > 0 && !call.conflict);
  const legacySourceCommandProperties = sources.reduce(
    (sum, item) => sum + countLegacyCommandProperties(item.source),
    0,
  );
  const directDispatchSites = dispatchCalls.length;
  const contextActionSites = countMatches(source, /contextActions\??\.dispatch\s*\(/g);
  const businessChatSites = countMatches(source, /businessChat\??\.(?:submitTask|open)\s*\(|ctox-business-os-chat-submit/g);
  const commandSignal = directDispatchSites > 0 || contextActionSites > 0 || businessChatSites > 0;
  const lifecycleTracking = /until\s*:\s*['"]terminal['"]|waitForTerminal\s*\(|resumeTracking\s*\(|commandBus\??\.subscribe\s*\(|commandBus\??\.getStatus\s*\(|ctox_queue_tasks|business_commands|businessChat\??\.submitTask\s*\(|ctox-business-os-chat-submit/.test(source);
  const runtimeTypes = literalTypes.filter((type) => browserRuntimeTypes.has(type));
  const nativeTypes = literalTypes.filter((type) => nativeControlType(type));
  const harnessRoutedTypes = literalTypes.filter((type) => !nativeControlType(type) && !browserRuntimeTypes.has(type));
  const dynamicDispatchSites = dispatchCalls.filter(({ call }) => call.types.length === 0).length;
  const issues = [];
  if (requiresCommand && !commandSignal) issues.push('required automation/action has no command-bus, context-action, or business-chat path');
  if (legacyTypes.length) issues.push(`forbidden legacy command type(s): ${legacyTypes.join(', ')}`);
  if (legacySourceCommandProperties > 0) {
    issues.push(`${legacySourceCommandProperties} registered command builder(s) still use the legacy type property`);
  }
  if (redundantAliasSites.length > 0) {
    issues.push(`${redundantAliasSites.length} dispatch site(s) redundantly send type and command_type`);
  }
  for (const conflict of conflicts) {
    issues.push(`${conflict.file}: dispatch contains conflicting type and command_type literals`);
  }
  if (harnessRoutedTypes.length > 0 && !lifecycleTracking) {
    issues.push(`harness-routed command has no visible lifecycle tracking signal: ${harnessRoutedTypes.join(', ')}`);
  }
  return {
    id: entry.id,
    required_command_actions: requiredCommandActions,
    direct_dispatch_sites: directDispatchSites,
    context_action_sites: contextActionSites,
    business_chat_sites: businessChatSites,
    dynamic_dispatch_sites: dynamicDispatchSites,
    literal_command_types: literalTypes,
    native_control_types: nativeTypes,
    browser_runtime_types: runtimeTypes,
    harness_routed_types: harnessRoutedTypes,
    legacy_type_only_sites: legacyOnlySites.length,
    redundant_type_alias_sites: redundantAliasSites.length,
    legacy_source_command_properties: legacySourceCommandProperties,
    lifecycle_tracking: lifecycleTracking,
    issues,
  };
}

function countLegacyCommandProperties(source) {
  return findLegacyCommandProperties(source).length;
}

function findLegacyCommandProperties(source) {
  const constants = new Map(
    [...source.matchAll(/\bconst\s+([A-Z][A-Z0-9_]*)\s*=\s*['"]([^'"]+)['"]/g)]
      .map((match) => [match[1], match[2]]),
  );
  const properties = [
    ...source.matchAll(/(?:^|[,\s{])type\s*:\s*(?:['"]([^'"]+)['"]|([A-Z][A-Z0-9_]*))/g),
  ];
  return properties.flatMap((match) => {
    const value = match[1] || constants.get(match[2]) || '';
    if (!knownCommandTypes.has(value)) return [];
    return [{
      value,
      line: source.slice(0, match.index).split('\n').length,
    }];
  });
}

function extractDispatchCalls(source) {
  const calls = [];
  const constants = new Map(
    [...source.matchAll(/\bconst\s+([A-Z][A-Z0-9_]*)\s*=\s*['"]([^'"]+)['"]/g)]
      .map((match) => [match[1], match[2]]),
  );
  const aliases = new Set(
    [...source.matchAll(/\b(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*[^;\n]*\bcommandBus(?:\?\.|\.)dispatch\b/g)]
      .map((match) => match[1]),
  );
  const starts = new Set();
  const directPattern = /(?:(?:\bcommandBus|\bbus)(?:\?\.|\.)dispatch|requireCommandBus\([^)]*\)\.dispatch)(?:\?\.)?\s*\(/g;
  for (const match of source.matchAll(directPattern)) {
    starts.add(match.index + match[0].lastIndexOf('('));
  }
  for (const alias of aliases) {
    const escaped = alias.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const aliasPattern = new RegExp(`\\b${escaped}(?:\\?\\.)?\\s*\\(`, 'g');
    const callPattern = new RegExp(`\\b${escaped}\\.call\\s*\\([^,]+,`, 'g');
    for (const match of source.matchAll(aliasPattern)) starts.add(source.indexOf('(', match.index));
    for (const match of source.matchAll(callPattern)) starts.add(source.indexOf(',', match.index) + 1);
  }
  for (const open of [...starts].sort((left, right) => left - right)) {
    const close = matchingParen(source, open);
    const body = source.slice(open + 1, close < 0 ? Math.min(source.length, open + 4000) : close);
    const canonical = extractPropertyTypes(body, 'command_type', constants);
    const legacy = extractPropertyTypes(body, 'type', constants);
    const types = [...new Set([...canonical, ...legacy])];
    calls.push({
      types,
      canonical,
      legacy,
      conflict: canonical.length > 0 && legacy.length > 0 && canonical.some((type) => !legacy.includes(type)),
    });
  }
  return calls;
}

function extractPropertyTypes(body, property, constants) {
  const pattern = new RegExp(`(?:^|[,\\s{])${property}\\s*:\\s*(?:['"]([^'"]+)['"]|([A-Z][A-Z0-9_]*))`, 'g');
  return [...body.matchAll(pattern)]
    .map((match) => match[1] || constants.get(match[2]) || '')
    .filter(Boolean);
}

function matchingParen(source, open) {
  let depth = 0;
  let quote = '';
  let escaped = false;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }
    if (char === '(') depth += 1;
    else if (char === ')') {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function nativeControlType(type) {
  return exactControlTypes.has(type)
    || predicateControlTypes.has(type)
    || controlPrefixes.some((prefix) => type.startsWith(prefix));
}

function countMatches(source, pattern) {
  return [...source.matchAll(pattern)].length;
}

function relative(root, file) {
  return path.relative(root, file).split(path.sep).join('/');
}

function walk(directory) {
  const files = [];
  if (!fs.existsSync(directory)) return files;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(target));
    else files.push(target);
  }
  return files;
}
