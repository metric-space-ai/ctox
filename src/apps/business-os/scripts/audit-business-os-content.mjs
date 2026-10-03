#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR AGPL-3.0-only

/**
 * Business OS user-facing content guard.
 *
 * This is intentionally a bounded content audit, not a source-wide word ban:
 * it follows the Workjet UI contract vocabulary, audits rendered shell/app
 * boundaries and relevant catalog metadata, and leaves protocol symbols,
 * data-plane implementation, tests, vendor code and diagnostic consoles out
 * through explicit, reviewable scope rules.
 */
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
export const BUSINESS_OS_ROOT = resolve(SCRIPT_DIR, '..');
export const REPO_ROOT = resolve(BUSINESS_OS_ROOT, '../../..');
export const CONTRACT_RELATIVE_PATH = 'src/apps/business-os/ui-contract/v1/workjet-ui-contract.json';

const CONTRACT_PATH = join(REPO_ROOT, CONTRACT_RELATIVE_PATH);
const contract = loadUiContract(CONTRACT_PATH);

export const FORBIDDEN_TERMS = Object.freeze(
  contract.vocabulary.forbiddenTerms.map((term) => String(term)),
);
export const USER_TERMS = Object.freeze(
  contract.vocabulary.userTerms.map((term) => String(term)),
);

const SOURCE_EXTENSIONS = new Set(['.css', '.html', '.js', '.mjs']);
const LOCALE_EXTENSIONS = new Set(['.json']);
const METADATA_EXTENSIONS = new Set(['.json']);
const TECHNICAL_DIAGNOSTIC_MODULES = new Map([
  [
    'appsec-pentest',
    'Security assessment/active-check console: its operator-facing diagnostic vocabulary is intentionally outside the product-copy audit.',
  ],
  [
    'ctox',
    'CTOX runtime/control diagnostics console: protocol and projection terminology is intentionally retained for advanced operators.',
  ],
]);

const EXCLUDED_UNMANIFESTED_MODULES = new Map([
  [
    'creator',
    'Legacy App Creator/template surface is not in the registry-backed Core-/Store-App manifest inventory; its desktop wrapper remains in the audited desktop-app boundary.',
  ],
]);

/**
 * These paths are not product-copy surfaces. Every exclusion has a concrete
 * boundary so a future scope expansion cannot silently swallow new files.
 */
export const EXCLUSION_RULES = Object.freeze([
  {
    prefix: 'src/apps/business-os/vendor/',
    reason: 'third-party/vendor source is not owned product copy',
  },
  {
    prefix: 'src/apps/business-os/rxdb/',
    reason: 'sync-engine/data-plane source and generated bundle are technical implementation',
  },
  {
    prefix: 'src/apps/business-os/node_modules/',
    reason: 'dependency tree is not owned product copy',
  },
  {
    prefix: 'src/apps/business-os/installed-modules/',
    reason: 'runtime-installed/customer app state is tenant-scoped and not release-owned copy',
  },
  {
    prefix: 'src/apps/business-os/local-modules/',
    reason: 'operator/customer local app state is not release-owned copy',
  },
  {
    prefix: 'src/apps/business-os/ui-contract/',
    reason: 'canonical contract vocabulary intentionally contains the forbidden-term list',
  },
]);

/**
 * The shell's settings helper renders product UI, while the other shared
 * files are data-plane/transport helpers. Auditing only this explicit helper
 * keeps the guard about visible shell copy rather than implementation logs.
 */
export const SHELL_SURFACE_FILES = Object.freeze([
  'src/apps/business-os/index.html',
  'src/apps/business-os/app.js',
  'src/apps/business-os/shared/react-settings.js',
  'src/apps/business-os/shared/bugReporter.js',
]);

const EXCLUDED_MODULE_UI_DIRS = new Set([
  'commands',
  'core',
  'document-format',
  'parsers',
  'reports',
  'templates',
]);

const USER_FACING_KEY_PATTERN =
  /(?:aria-(?:label|description|roledescription|placeholder|valuetext|errormessage)|title|alt|placeholder|label|description|tooltip(?:Text)?|message|error|warning|success|notice|heading|caption|text|displayName|productName|appName|reason|summary|details)\s*["']?\s*(?:=|:)/iu;
const USER_FACING_RENDER_PATTERN =
  /(?:innerHTML|textContent|setStatus|showBusiness(?:Alert|Confirm)|toast(?:Manager)?\.|notify\b|set(?:Error|Warning|Notice|Message)\b)/iu;
const ERROR_COPY_PATTERN = /(?:throw\s+new\s+Error|new\s+Error|new\s+[A-Za-z_$][\w$]*Error)\s*\(/u;
const CONSOLE_PATTERN = /(?:^|\b)(?:console|logger)\.(?:debug|info|log|warn|error)\s*\(/u;
const JSX_TEXT_PATTERN = />\s*([^<>{}]*\p{L}[^<>{}]*)\s*</gu;

/**
 * Technical diagnostic UI is a deliberate exception, scoped to exact shell
 * fragments. This does not create a global ignore word.
 */
export const TECHNICAL_DIAGNOSTIC_CONTEXTS = Object.freeze([
  {
    path: 'src/apps/business-os/app.js',
    context: /WebRTC Sync|Sync Room|<span>Signaling<\/span>|Signaling-URL|WebRTC-Signalisierung|Native control surface for queues, runs, sync state, and agent context|Native CTOX control surface for queue tasks, runs, module reports, releases, and source evidence/iu,
    reason: 'the shell exposes an explicit advanced connection-diagnostics panel',
  },
  {
    path: 'src/apps/business-os/shared/react-settings.js',
    context: /data-sync-room|data-sync-signaling|WebRTC-Signalisierung|WebRTC-Raum|WebRTC-Signalisierungs-URLs/iu,
    reason: 'the settings connection panel is an explicit advanced diagnostics surface',
  },
]);

/** Internal execution prompts are not rendered product copy and stay exact-path scoped. */
export const INTERNAL_COPY_CONTEXTS = Object.freeze([
  {
    path: 'src/apps/business-os/modules/cv-print-builder/index.js',
    context: /Ausfuehrung nur ueber CTOX desktop_files\/desktop_file_chunks/iu,
    reason: 'the parser task prompt is an internal execution instruction, not product copy',
  },
]);

const SCOPED_COPY_EXCEPTIONS = Object.freeze([
  ...TECHNICAL_DIAGNOSTIC_CONTEXTS,
  ...INTERNAL_COPY_CONTEXTS,
]);

function loadUiContract(filePath) {
  if (!existsSync(filePath)) throw new Error(`UI contract is missing: ${filePath}`);
  let value;
  try {
    value = JSON.parse(readFileSync(filePath, 'utf8'));
  } catch (error) {
    throw new Error(`UI contract is not valid JSON: ${error.message}`);
  }
  if (value?.schema !== 'workjet.ui.contract.v1' || value?.version !== 1) {
    throw new Error('Business OS content guard requires workjet.ui.contract.v1 version 1');
  }
  if (!Array.isArray(value?.vocabulary?.forbiddenTerms) || value.vocabulary.forbiddenTerms.length === 0) {
    throw new Error('UI contract forbiddenTerms must be a non-empty array');
  }
  if (!Array.isArray(value?.vocabulary?.userTerms) || value.vocabulary.userTerms.length === 0) {
    throw new Error('UI contract userTerms must be a non-empty array');
  }
  return value;
}

function normalizePath(value) {
  return String(value).replaceAll('\\', '/');
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function termPattern(term) {
  return new RegExp(`\\b${escapeRegExp(term)}\\b`, 'giu');
}

function findForbiddenTerms(value) {
  const findings = [];
  for (const term of FORBIDDEN_TERMS) {
    const pattern = termPattern(term);
    for (const match of String(value).matchAll(pattern)) {
      findings.push({ term, offset: match.index ?? 0 });
    }
  }
  return findings;
}

function lineNumberAt(source, offset) {
  let line = 1;
  for (let index = 0; index < offset; index += 1) {
    if (source.charCodeAt(index) === 10) line += 1;
  }
  return line;
}

function lineTextAt(source, offset) {
  const start = source.lastIndexOf('\n', offset - 1) + 1;
  const end = source.indexOf('\n', offset);
  return source.slice(start, end === -1 ? source.length : end).trim();
}

function previousSignificantCharacter(source, offset) {
  for (let index = offset - 1; index >= 0; index -= 1) {
    if (!/\s/u.test(source[index])) return source[index];
  }
  return '';
}

function shouldStartRegex(previous) {
  return previous === '' || /[=(:,;!?&|{[\]]/u.test(previous);
}

/** Parse only quoted literals while skipping comments and regex bodies. */
export function scanStringLiterals(source) {
  const literals = [];
  let index = 0;
  let blockComment = false;

  const skipRegex = () => {
    let inClass = false;
    index += 1;
    while (index < source.length) {
      const current = source[index];
      if (current === '\\') {
        index += 2;
        continue;
      }
      if (current === '[') inClass = true;
      if (current === ']') inClass = false;
      if (current === '/' && !inClass) {
        index += 1;
        while (/[A-Za-z]/u.test(source[index] ?? '')) index += 1;
        return;
      }
      if (current === '\n') return;
      index += 1;
    }
  };

  const skipTemplateExpression = () => {
    let depth = 1;
    let quote = null;
    while (index < source.length && depth > 0) {
      const current = source[index];
      if (quote !== null) {
        if (current === '\\') {
          index += 2;
          continue;
        }
        if (current === quote) quote = null;
        index += 1;
        continue;
      }
      if (current === "'" || current === '"' || current === '`') {
        quote = current;
        index += 1;
        continue;
      }
      if (current === '{') depth += 1;
      if (current === '}') depth -= 1;
      index += 1;
    }
  };

  while (index < source.length) {
    if (blockComment) {
      const end = source.indexOf('*/', index);
      if (end === -1) break;
      index = end + 2;
      blockComment = false;
      continue;
    }
    const current = source[index];
    const next = source[index + 1];
    if (current === '/' && next === '*') {
      blockComment = true;
      index += 2;
      continue;
    }
    if (current === '/' && next === '/') {
      const end = source.indexOf('\n', index + 2);
      index = end === -1 ? source.length : end + 1;
      continue;
    }
    if (current === '/' && shouldStartRegex(previousSignificantCharacter(source, index))) {
      skipRegex();
      continue;
    }
    if (current !== "'" && current !== '"' && current !== '`') {
      index += 1;
      continue;
    }

    const quote = current;
    const start = index;
    index += 1;
    let value = '';
    while (index < source.length) {
      const character = source[index];
      if (character === '\\') {
        value += source.slice(index, index + 2);
        index += 2;
        continue;
      }
      if (quote === '`' && character === '$' && source[index + 1] === '{') {
        index += 2;
        skipTemplateExpression();
        continue;
      }
      if (character === quote) {
        index += 1;
        break;
      }
      value += character;
      index += 1;
    }
    literals.push({
      value: value
        .replaceAll(/\\([\\'"`])/g, '$1')
        .replaceAll(/\\n/g, '\n')
        .replaceAll(/\\r/g, '\r')
        .replaceAll(/\\t/g, '\t'),
      start,
      end: index,
      before: source.slice(Math.max(0, start - 300), start),
      after: source.slice(index, Math.min(source.length, index + 160)),
    });
  }
  return literals;
}

function looksLikeHtml(value) {
  return /<[A-Za-z][^>]*>|<\/[A-Za-z][^>]*>/u.test(value);
}

/**
 * HTML strings contain implementation values (`value`, `class`, `data-*`)
 * alongside actual copy. Only retain visible text and accessibility/content
 * attributes so internal enum values such as `native` cannot become a false
 * product-copy finding.
 */
function extractHtmlCopy(value) {
  const attributes = [];
  const attributePattern = /\b(?:aria-(?:label|description|roledescription|placeholder|valuetext|errormessage)|title|alt|placeholder)\s*=\s*(["'])(.*?)\1/giu;
  for (const match of String(value).matchAll(attributePattern)) attributes.push(match[2]);
  const visibleText = String(value).replace(/<[^>]*>/gu, ' ');
  return `${visibleText} ${attributes.join(' ')}`;
}

function isNaturalCopy(value) {
  return /\s/u.test(value) && /\p{L}/u.test(value) && !/^\s*(?:https?:|[A-Za-z_$][\w$.-]*\s*=)/u.test(value);
}

function isTechnicalToken(value) {
  return /^\s*[a-z0-9]+(?:-[a-z0-9]+)+\s*$/u.test(value);
}

function isUserFacingLiteral(literal, relativePath, metadata) {
  if (metadata || looksLikeHtml(literal.value)) return true;
  const nearby = literal.before.split('\n').slice(-3).join('\n');
  const lineBefore = literal.before.slice(literal.before.lastIndexOf('\n') + 1);
  if (relativePath.endsWith('.html') && /(?:^|\s)(?:src|srcset|href)\s*=\s*$/iu.test(lineBefore)) return false;
  if (isTechnicalToken(literal.value)) return false;
  if (CONSOLE_PATTERN.test(lineBefore)) return false;
  if (/\.(?:includes|test)\s*\(/u.test(lineBefore)) return false;
  if (USER_FACING_KEY_PATTERN.test(nearby)) {
    const keyMatch = lineBefore.match(/\b([A-Za-z][\w-]*)\s*(?:=|:)\s*$/u);
    if (keyMatch && /^(?:reason|message|error|warning|success|notice|details)$/iu.test(keyMatch[1])) {
      return isNaturalCopy(literal.value);
    }
    return true;
  }
  if (USER_FACING_RENDER_PATTERN.test(lineBefore)) return true;
  if (ERROR_COPY_PATTERN.test(lineBefore) && isNaturalCopy(literal.value)) return true;
  // Labels/messages in the module's UI dictionaries are object values. Keep
  // this restricted to rendered app boundaries; runtime helpers are excluded.
  if (isNaturalCopy(literal.value) && /(?:index|desktop-apps|react-settings|bugReporter)\.(?:js|mjs|html)$/u.test(relativePath)) {
    return true;
  }
  return false;
}

function matchingScopedCopyException(relativePath, context) {
  return SCOPED_COPY_EXCEPTIONS.find(
    (entry) => entry.path === relativePath && entry.context.test(context),
  );
}

function makeFinding({ source, relativePath, start, term, kind }) {
  const line = lineNumberAt(source, start);
  return {
    path: relativePath,
    line,
    term,
    kind,
    text: lineTextAt(source, start),
  };
}

/** Audit one source/metadata document without touching its contents. */
export function auditSourceText(source, relativePath, { metadata = false } = {}) {
  const normalizedPath = normalizePath(relativePath);
  const findings = [];
  const seen = new Set();
  const add = (start, value, context, kind, sourceValue = value) => {
    const diagnostic = matchingScopedCopyException(normalizedPath, context);
    for (const match of findForbiddenTerms(value)) {
      if (diagnostic) continue;
      const sourceOffset = value === sourceValue
        ? match.offset
        : sourceValue.search(termPattern(match.term));
      const absoluteStart = start + (sourceOffset < 0 ? 0 : sourceOffset);
      const finding = makeFinding({
        source,
        relativePath: normalizedPath,
        start: absoluteStart,
        term: match.term,
        kind,
      });
      const key = `${finding.path}:${finding.line}:${finding.term}:${finding.kind}`;
      if (!seen.has(key)) {
        seen.add(key);
        findings.push(finding);
      }
    }
  };

  for (const literal of scanStringLiterals(source)) {
    const copyValue = looksLikeHtml(literal.value) ? extractHtmlCopy(literal.value) : literal.value;
    const forbidden = findForbiddenTerms(copyValue);
    if (forbidden.length === 0) continue;
    if (!isUserFacingLiteral(literal, normalizedPath, metadata)) continue;
    const context = `${literal.before.slice(-220)}${literal.value}${literal.after.slice(0, 100)}`;
    add(literal.start, copyValue, context, metadata ? 'metadata' : 'user-facing-literal', literal.value);
  }

  for (const match of source.matchAll(JSX_TEXT_PATTERN)) {
    const value = match[1] ?? '';
    const start = (match.index ?? 0) + Math.max(0, match[0].indexOf(value));
    const context = `${source.slice(Math.max(0, start - 220), start)}${value}`;
    add(start, value, context, 'rendered-text');
  }
  return findings;
}

function exclusionFor(relativePath) {
  const path = normalizePath(relativePath);
  return EXCLUSION_RULES.find((entry) => path === entry.prefix.slice(0, -1) || path.startsWith(entry.prefix));
}

export function exclusionReason(relativePath) {
  return exclusionFor(relativePath)?.reason || null;
}

function shouldSkipFile(relativePath) {
  const normalized = normalizePath(relativePath);
  if (exclusionFor(normalized)) return true;
  if (/(?:^|\/)(?:tests?|qa|fixtures)(?:\/|$)/iu.test(normalized)) return true;
  if (/(?:^|\/)[^/]*\.test\.(?:js|mjs)$/iu.test(normalized)) return true;
  if (/(?:^|\/)(?:registry-launch-smoke)\.(?:js|mjs)$/iu.test(normalized)) return true;
  return false;
}

function shouldAuditFile(relativePath) {
  if (shouldSkipFile(relativePath)) return false;
  const extension = extname(relativePath).toLowerCase();
  if (SOURCE_EXTENSIONS.has(extension)) return true;
  return LOCALE_EXTENSIONS.has(extension) && /(?:^|\/)locales\/[^/]+\.json$/u.test(relativePath);
}

function walkFiles(root, relativeDirectory, { skipDirectories = new Set() } = {}) {
  const absoluteDirectory = join(root, relativeDirectory);
  if (!existsSync(absoluteDirectory)) throw new Error(`Audit scope is missing: ${relativeDirectory}`);
  const files = [];
  const visit = (absolutePath, relativePath) => {
    const stat = statSync(absolutePath);
    if (stat.isDirectory()) {
      if (skipDirectories.has(relativePath.split('/').at(-1))) return;
      for (const name of readdirSync(absolutePath).sort()) {
        visit(join(absolutePath, name), `${relativePath}/${name}`);
      }
      return;
    }
    if (stat.isFile() && shouldAuditFile(relativePath)) files.push(relativePath);
  };
  const normalizedDirectory = normalizePath(relativeDirectory).replace(/\/+$/u, '');
  visit(absoluteDirectory, normalizedDirectory);
  return files;
}

function readJson(root, relativePath) {
  const absolutePath = join(root, relativePath);
  if (!existsSync(absolutePath)) throw new Error(`Required metadata is missing: ${relativePath}`);
  try {
    return JSON.parse(readFileSync(absolutePath, 'utf8'));
  } catch (error) {
    throw new Error(`${relativePath} is not valid JSON: ${error.message}`);
  }
}

function loadModuleScope(root) {
  const modulesRelative = 'src/apps/business-os/modules';
  const modulesRoot = join(root, modulesRelative);
  if (!existsSync(modulesRoot)) throw new Error(`Module scope is missing: ${modulesRelative}`);
  const approved = [];
  const excluded = [];
  const entries = readdirSync(modulesRoot, { withFileTypes: true }).filter((entry) => entry.isDirectory()).sort((a, b) => a.name.localeCompare(b.name));
  if (entries.length === 0) throw new Error('Business OS content guard found no module directories');

  for (const entry of entries) {
    const id = entry.name;
    const relativeManifest = `${modulesRelative}/${id}/module.json`;
    const manifestPath = join(root, relativeManifest);
    if (!existsSync(manifestPath)) {
      const reason = EXCLUDED_UNMANIFESTED_MODULES.get(id);
      if (reason) {
        excluded.push({ id, path: `${modulesRelative}/${id}/`, reason });
        continue;
      }
      throw new Error(`Module ${id} has no manifest: ${relativeManifest}`);
    }
    const manifest = readJson(root, relativeManifest);
    if (String(manifest.id || '') !== id) throw new Error(`${relativeManifest}: manifest id does not match directory`);

    const diagnosticReason = TECHNICAL_DIAGNOSTIC_MODULES.get(id);
    if (diagnosticReason) {
      excluded.push({ id, path: `${modulesRelative}/${id}/`, reason: diagnosticReason });
      continue;
    }
    if (manifest.source === 'internal' || manifest.install_scope === 'internal') {
      excluded.push({
        id,
        path: `${modulesRelative}/${id}/`,
        reason: 'internal module/research surface is not part of the freigegebene Core-/Store-App release scope',
      });
      continue;
    }
    const approvedShape = manifest.source === 'core' || manifest.source === 'catalog';
    const approvedScope = manifest.install_scope === 'core' || manifest.install_scope === 'store';
    const distribution = manifest.store?.distribution;
    const approvedDistribution = distribution === 'system-module' || distribution === 'catalog-module';
    if (!approvedShape || !approvedScope || !approvedDistribution) {
      throw new Error(
        `${relativeManifest}: unclassified module scope (source=${JSON.stringify(manifest.source)}, install_scope=${JSON.stringify(manifest.install_scope)}, distribution=${JSON.stringify(distribution)})`,
      );
    }
    approved.push({ id, path: `${modulesRelative}/${id}/`, manifest });
  }
  if (approved.length === 0) throw new Error('Business OS content guard found no approved Core-/Store-App modules');
  return { approved, excluded };
}

function relevantManifestMetadata(manifest) {
  return {
    id: manifest.id,
    title: manifest.title,
    description: manifest.description,
    category: manifest.category,
    store: {
      summary: manifest.store?.summary,
    },
    layout: {
      left: manifest.layout?.left,
      center: manifest.layout?.center,
      right: manifest.layout?.right,
      third_pane_justification: manifest.layout?.third_pane_justification,
      drawers: manifest.layout?.drawers,
    },
  };
}

function registryMetadata(root, approvedIds) {
  const registryRelative = 'src/apps/business-os/modules/registry.json';
  const registry = readJson(root, registryRelative);
  if (!Array.isArray(registry.modules) || registry.modules.length === 0) {
    throw new Error(`${registryRelative}: modules must be a non-empty array`);
  }
  const byId = new Map(registry.modules.map((module) => [String(module?.id || ''), module]));
  const copies = [];
  for (const id of approvedIds) {
    const module = byId.get(id);
    if (!module) throw new Error(`${registryRelative}: approved module ${id} is missing from registry`);
    copies.push({ id, metadata: relevantManifestMetadata(module) });
  }
  return { path: registryRelative, copies };
}

/** Resolve and audit the bounded Business OS product surfaces. */
export async function auditBusinessOsContent(root = REPO_ROOT) {
  const resolvedRoot = resolve(root);
  // The contract and module inventory are prerequisites, not optional inputs.
  const scoped = loadModuleScope(resolvedRoot);
  const files = [];
  for (const relativePath of SHELL_SURFACE_FILES) {
    if (!existsSync(join(resolvedRoot, relativePath))) throw new Error(`Required shell surface is missing: ${relativePath}`);
    files.push(relativePath);
  }
  const desktopAppsRoot = 'src/apps/business-os/desktop-apps';
  files.push(...walkFiles(resolvedRoot, desktopAppsRoot));
  for (const module of scoped.approved) {
    const required = `${module.path}module.json`;
    if (!existsSync(join(resolvedRoot, required))) throw new Error(`Required approved manifest is missing: ${required}`);
    files.push(required);
    files.push(...walkFiles(resolvedRoot, module.path, { skipDirectories: EXCLUDED_MODULE_UI_DIRS }));
  }

  const findings = [];
  for (const relativePath of [...new Set(files)].sort()) {
    const source = readFileSync(join(resolvedRoot, relativePath), 'utf8');
    findings.push(...auditSourceText(source, relativePath, {
      metadata: METADATA_EXTENSIONS.has(extname(relativePath).toLowerCase()),
    }));
  }

  const registry = registryMetadata(resolvedRoot, scoped.approved.map(({ id }) => id));
  for (const { id, metadata } of registry.copies) {
    findings.push(...auditSourceText(JSON.stringify(metadata), `${registry.path}#${id}`, { metadata: true }));
  }

  return {
    filesAudited: files.length + registry.copies.length,
    findings,
    approvedModules: scoped.approved.map(({ id }) => id),
    excludedModules: scoped.excluded,
    forbiddenTerms: FORBIDDEN_TERMS,
    userTerms: USER_TERMS,
  };
}

export function formatFindings(findings) {
  return findings
    .map((finding) => `${finding.path}:${finding.line} [${finding.term}] ${finding.text}`)
    .join('\n');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const result = await auditBusinessOsContent(process.argv[2] || REPO_ROOT);
    if (result.findings.length > 0) {
      console.error(`Business OS content guard found ${result.findings.length} forbidden UI/metadata term(s):`);
      console.error(formatFindings(result.findings));
      process.exitCode = 1;
    } else {
      console.log(`Business OS content guard passed (${result.filesAudited} files; ${result.approvedModules.length} approved apps).`);
    }
  } catch (error) {
    console.error(`Business OS content guard failed closed: ${error.message}`);
    process.exitCode = 1;
  }
}
