#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const moduleId = process.argv[2];
const modeArg = process.argv[3] || '';

if (!moduleId || moduleId.includes('/') || moduleId.includes('\\') || moduleId === '.' || moduleId === '..') {
  console.error('Usage: node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <module> [--installed|--catalog-installed|--local]');
  process.exit(2);
}
if (modeArg && modeArg !== '--installed' && modeArg !== '--catalog-installed' && modeArg !== '--local') {
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
const localDir = join(installedAppRootFor(root), 'local-modules', moduleId);
const catalogInstalledMode = modeArg === '--catalog-installed';
const installedMode = modeArg === '--installed'
  || catalogInstalledMode
  || (!modeArg && !existsSync(sourceDir) && existsSync(installedDir));
// Local mode: git-ignored, operator-placed dev/customer modules under
// runtime/business-os/local-modules/. Structural, data-boundary, and design
// rules apply like installed mode; the business-behavior positives
// (automation command, create affordance, mandatory persistence) do not.
const localMode = modeArg === '--local'
  || (!modeArg && !existsSync(sourceDir) && !existsSync(installedDir) && existsSync(localDir));
// Runtime module mode = the module lives outside src/ (installed or local).
const runtimeModuleMode = installedMode || localMode;
const moduleDir = localMode ? localDir : installedMode ? installedDir : sourceDir;
const expectedEntry = localMode
  ? `local-modules/${moduleId}/index.html`
  : installedMode
    ? `installed-modules/${moduleId}/index.html`
    : `modules/${moduleId}/index.html`;
const expectedInstallScope = localMode ? 'local' : installedMode ? 'installed' : 'store';
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
const allowedInstalledRootDirs = new Set(['core', 'lib', 'locales', 'tests', 'vendor']);
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function fail(message) {
  failures.push(message);
}

function rel(path) {
  return relative(root, path).split(sep).join('/');
}

function normalizedModuleCollectionPrefix(id) {
  return String(id || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '');
}

function isModuleScopedCollectionName(collection, id) {
  const name = String(collection || '').trim();
  const direct = String(id || '').trim();
  const normalized = normalizedModuleCollectionPrefix(id);
  return Boolean(name && (
    name === direct
    || name.startsWith(`${direct}_`)
    || (normalized && (name === normalized || name.startsWith(`${normalized}_`)))
  ));
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

function schemaTypes(schema) {
  const raw = schema && typeof schema === 'object' ? schema.type : undefined;
  if (Array.isArray(raw)) return raw.map((item) => String(item)).filter(Boolean);
  if (raw) return [String(raw)];
  return [];
}

function actualJsonType(value) {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

function normalizeRequired(value) {
  return Array.isArray(value) ? value.map(String).sort() : [];
}

function schemaPropertySummary(schema) {
  const properties = schema && typeof schema === 'object' && schema.properties && typeof schema.properties === 'object'
    ? schema.properties
    : {};
  const out = {};
  for (const [name, property] of Object.entries(properties)) {
    const summary = {};
    const types = schemaTypes(property);
    if (types.length > 0) summary.type = types.length === 1 ? types[0] : types;
    if (property && typeof property === 'object' && Object.prototype.hasOwnProperty.call(property, 'maxLength')) {
      summary.maxLength = property.maxLength;
    }
    out[name] = summary;
  }
  return out;
}

function sameJsonValue(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function collectSchemaJsParityFailures(schemaDoc, schemaJsCollections) {
  const messages = [];
  const nativeCollections = schemaDoc?.collections && typeof schemaDoc.collections === 'object'
    ? schemaDoc.collections
    : {};
  const browserCollections = schemaJsCollections && typeof schemaJsCollections === 'object'
    ? schemaJsCollections
    : {};
  for (const name of Object.keys(nativeCollections)) {
    const nativeSchema = nativeCollections[name];
    const browserSchema = browserCollections[name];
    if (!browserSchema) {
      messages.push(`schema.js missing collection ${name} from collections.schema.json`);
      continue;
    }
    for (const field of ['version', 'primaryKey', 'type', 'additionalProperties']) {
      if (
        Object.prototype.hasOwnProperty.call(nativeSchema || {}, field)
        || Object.prototype.hasOwnProperty.call(browserSchema || {}, field)
      ) {
        if (!sameJsonValue(nativeSchema?.[field], browserSchema?.[field])) {
          messages.push(`schema.js collection ${name} ${field} does not match collections.schema.json`);
        }
      }
    }
    if (!sameJsonValue(normalizeRequired(nativeSchema?.required), normalizeRequired(browserSchema?.required))) {
      messages.push(`schema.js collection ${name} required fields do not match collections.schema.json`);
    }
    const nativeProps = schemaPropertySummary(nativeSchema);
    const browserProps = schemaPropertySummary(browserSchema);
    for (const prop of Object.keys(nativeProps)) {
      if (!Object.prototype.hasOwnProperty.call(browserProps, prop)) {
        messages.push(`schema.js collection ${name} missing property ${prop} from collections.schema.json`);
      } else if (!sameJsonValue(nativeProps[prop], browserProps[prop])) {
        messages.push(`schema.js collection ${name} property ${prop} does not match collections.schema.json`);
      }
    }
    for (const prop of Object.keys(browserProps)) {
      if (!Object.prototype.hasOwnProperty.call(nativeProps, prop)) {
        messages.push(`schema.js collection ${name} property ${prop} is not declared in collections.schema.json`);
      }
    }
  }
  for (const name of Object.keys(browserCollections)) {
    if (!Object.prototype.hasOwnProperty.call(nativeCollections, name) && !shellCollections.has(name)) {
      messages.push(`schema.js exports collection ${name}, but collections.schema.json does not define it`);
    }
  }
  return messages;
}

// SYNC-32: per-collection sync profiles. `syncProfile` is a wrapper-level
// sibling of `schema` (like `conflictStrategy`), so declaring it never shifts
// the parsed schema or its advertised hash. `demand-chunks` collections must
// carry the fields the native demand-file source reads.
const allowedSyncProfiles = new Set(['eager', 'demand-only', 'demand-chunks']);

function collectSyncProfileFailures(schemaDoc) {
  const messages = [];
  const collections = schemaDoc?.collections && typeof schemaDoc.collections === 'object'
    ? schemaDoc.collections
    : {};
  for (const [name, definition] of Object.entries(collections)) {
    if (!definition || typeof definition !== 'object') continue;
    const isWrapper = Boolean(definition.schema && !definition.primaryKey);
    const schema = isWrapper ? definition.schema : definition;
    if (!isWrapper && Object.prototype.hasOwnProperty.call(definition, 'syncProfile')) {
      messages.push(`collections.schema.json collection ${name} declares syncProfile inside the schema object; declare it on the wrapper ({ "syncProfile": ..., "schema": {...} }) so it stays out of the schema hash`);
      continue;
    }
    if (!Object.prototype.hasOwnProperty.call(definition, 'syncProfile')) continue;
    const declared = definition.syncProfile;
    if (typeof declared !== 'string' || !allowedSyncProfiles.has(declared)) {
      messages.push(`collections.schema.json collection ${name} syncProfile must be one of ${Array.from(allowedSyncProfiles).map((value) => `"${value}"`).join(', ')}`);
      continue;
    }
    if (declared === 'demand-chunks') {
      const properties = schema?.properties && typeof schema.properties === 'object' ? schema.properties : {};
      for (const field of ['idx', 'data']) {
        if (!Object.prototype.hasOwnProperty.call(properties, field)) {
          messages.push(`collections.schema.json demand-chunks collection ${name} schema is missing required chunk field ${field}`);
        }
      }
      if (!Object.prototype.hasOwnProperty.call(properties, 'file_id')
        && !Object.prototype.hasOwnProperty.call(properties, 'blob_id')) {
        messages.push(`collections.schema.json demand-chunks collection ${name} schema is missing the owner key field (file_id or blob_id)`);
      }
    }
  }
  return messages;
}

function collectDeclarativeMigrationFailures(schemaDoc) {
  const messages = [];
  const collections = schemaDoc?.collections && typeof schemaDoc.collections === 'object'
    ? schemaDoc.collections
    : {};
  const strategies = schemaDoc?.migration_strategies && typeof schemaDoc.migration_strategies === 'object'
    ? schemaDoc.migration_strategies
    : {};
  for (const [name, definition] of Object.entries(collections)) {
    const schema = definition?.schema && !definition?.primaryKey ? definition.schema : definition;
    const version = Number(schema?.version || 0);
    if (!Number.isInteger(version) || version < 0) {
      messages.push(`collections.schema.json collection ${name} version must be a non-negative integer`);
      continue;
    }
    for (let target = 1; target <= version; target += 1) {
      const spec = strategies?.[name]?.[String(target)];
      if (!spec) {
        messages.push(`collections.schema.json collection ${name} version ${version} requires migration_strategies.${name}.${target}`);
        continue;
      }
      const operations = Array.isArray(spec) ? spec : spec?.operations;
      if (!Array.isArray(operations)) {
        messages.push(`migration_strategies.${name}.${target} must contain an operations array`);
        continue;
      }
      for (const operation of operations) {
        if (!operation || typeof operation !== 'object') {
          messages.push(`migration_strategies.${name}.${target} contains a non-object operation`);
        } else if (operation.op === 'set_from_first_truthy') {
          if (!operation.field || !Array.isArray(operation.paths)) {
            messages.push(`migration_strategies.${name}.${target} set_from_first_truthy requires field and paths`);
          }
        } else if (operation.op === 'set_boolean') {
          if (!operation.field) {
            messages.push(`migration_strategies.${name}.${target} set_boolean requires field`);
          }
        } else {
          messages.push(`migration_strategies.${name}.${target} uses unsupported operation ${operation.op || '<missing>'}`);
        }
      }
    }
  }
  return messages;
}

function sampleValueForSchema(schema) {
  const types = schemaTypes(schema);
  const type = types.find((item) => item !== 'null') || types[0] || 'string';
  if (type === 'number' || type === 'integer') return 1;
  if (type === 'boolean') return true;
  if (type === 'array') return [];
  if (type === 'object') return {};
  return '2026-01-02';
}

function sampleRecordForSchemas(schemas) {
  const sample = {};
  for (const schema of schemas) {
    const properties = schema?.properties && typeof schema.properties === 'object' ? schema.properties : {};
    for (const [name, property] of Object.entries(properties)) {
      if (!Object.prototype.hasOwnProperty.call(sample, name)) {
        sample[name] = sampleValueForSchema(property);
      }
    }
  }
  return sample;
}

function allowedTypesByProperty(schemas) {
  const out = new Map();
  for (const schema of schemas) {
    const properties = schema?.properties && typeof schema.properties === 'object' ? schema.properties : {};
    for (const [name, property] of Object.entries(properties)) {
      if (!out.has(name)) out.set(name, new Set());
      for (const type of schemaTypes(property)) out.get(name).add(type);
    }
  }
  return out;
}

async function importEsmModule(path) {
  const url = pathToFileURL(path);
  url.searchParams.set('ctox_static_check', `${Date.now()}_${Math.random().toString(36).slice(2)}`);
  return import(url.href);
}

async function loadSchemaJsCollections(path) {
  try {
    const module = await importEsmModule(path);
    if (!module.collections || typeof module.collections !== 'object' || Array.isArray(module.collections)) {
      fail('schema.js must export a collections object');
      return null;
    }
    return module.collections;
  } catch (error) {
    fail(`schema.js could not be imported as browser ESM: ${error.message}`);
    return null;
  }
}

async function collectRecordHelperSchemaFailures(moduleDir, schemaDoc) {
  const messages = [];
  const recordsPath = join(moduleDir, 'core/records.mjs');
  const schemas = Object.values(schemaDoc?.collections || {});
  if (!existsSync(recordsPath) || schemas.length === 0) return messages;
  let module;
  try {
    module = await importEsmModule(recordsPath);
  } catch (error) {
    messages.push(`core/records.mjs could not be imported as browser ESM: ${error.message}`);
    return messages;
  }
  const sample = sampleRecordForSchemas(schemas);
  const allowedByProperty = allowedTypesByProperty(schemas);
  for (const [name, value] of Object.entries(module)) {
    if (!/^normalize[A-Z]/.test(name) || typeof value !== 'function') continue;
    let record;
    try {
      record = value(sample, { nowMs: 1781990000000 });
    } catch (error) {
      messages.push(`core/records.mjs ${name} threw when called with schema-shaped sample input: ${error.message}`);
      continue;
    }
    if (!record || typeof record !== 'object' || Array.isArray(record)) continue;
    for (const [field, fieldValue] of Object.entries(record)) {
      const allowed = allowedByProperty.get(field);
      if (!allowed || allowed.size === 0) continue;
      const actual = actualJsonType(fieldValue);
      if (!allowed.has(actual)) {
        messages.push(`core/records.mjs ${name} returns ${field} as ${actual}, but collections.schema.json declares ${Array.from(allowed).sort().join('|')}`);
      }
    }
  }
  return messages;
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

function cssRules(css) {
  const rules = [];
  for (const match of String(css || '').matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    rules.push({ selector: match[1] || '', body: match[2] || '' });
  }
  return rules;
}

function cssSelectorContainsClass(selector, className) {
  const needle = String(className || '').trim().toLowerCase();
  if (!needle) return false;
  const source = String(selector || '').toLowerCase();
  let cursor = 0;
  while (cursor < source.length) {
    const index = source.indexOf(`.${needle}`, cursor);
    if (index === -1) return false;
    const after = source[index + needle.length + 1] || '';
    if (!/[A-Za-z0-9_-]/.test(after)) return true;
    cursor = index + 1;
  }
  return false;
}

function cssSelectorContainsHiddenClass(selector, className) {
  const compact = String(selector || '').replace(/\s+/g, '').toLowerCase();
  const needle = String(className || '').trim().toLowerCase();
  return Boolean(needle && compact.includes(`.${needle}[hidden]`));
}

function jsIdentifierAt(source, index) {
  const match = /^[A-Za-z_$][\w$]*/.exec(String(source || '').slice(index));
  return match?.[0] || '';
}

function isIdentifierBoundary(source, index) {
  return !/[A-Za-z0-9_$]/.test(String(source || '')[index] || '');
}

function containsIdentifierCall(source, name) {
  const text = String(source || '');
  const needle = String(name || '');
  if (!needle) return false;
  let cursor = 0;
  while (cursor < text.length) {
    const index = text.indexOf(needle, cursor);
    if (index === -1) return false;
    if (isIdentifierBoundary(text, index - 1) && isIdentifierBoundary(text, index + needle.length)) {
      const rest = text.slice(index + needle.length).trimStart();
      if (rest.startsWith('(') || rest.startsWith('?.(')) return true;
    }
    cursor = index + needle.length;
  }
  return false;
}

// Real scanner instead of a quote regex: comments, escapes and nested
// template-literal `${}` expressions all kept the naive matcher permanently
// out of sync (an apostrophe inside a comment opened a phantom string and
// every later literal paired off wrongly — dozens of false "no visible
// handler" failures on real modules).
function jsQuotedLiterals(source) {
  const text = String(source || '');
  const literals = [];
  // Stack of template nesting: each entry counts open braces inside a `${}`.
  const templateStack = [];
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    const next = text[i + 1];
    if (ch === '/' && next === '/') {
      const nl = text.indexOf('\n', i);
      i = nl === -1 ? text.length : nl + 1;
      continue;
    }
    if (ch === '/' && next === '*') {
      const close = text.indexOf('*/', i + 2);
      i = close === -1 ? text.length : close + 2;
      continue;
    }
    if (ch === "'" || ch === '"') {
      const start = i;
      i += 1;
      let value = '';
      while (i < text.length && text[i] !== ch && text[i] !== '\n') {
        if (text[i] === '\\') { value += text.slice(i, i + 2); i += 2; continue; }
        value += text[i];
        i += 1;
      }
      i += 1; // closing quote (or past the newline of an unterminated literal)
      literals.push({ value, start, end: i });
      continue;
    }
    if (ch === '`') {
      const start = i;
      i += 1;
      let value = '';
      while (i < text.length) {
        if (text[i] === '\\') { value += text.slice(i, i + 2); i += 2; continue; }
        if (text[i] === '`') { i += 1; break; }
        if (text[i] === '$' && text[i + 1] === '{') {
          // Scan the interpolation as regular code so literals inside it are
          // collected on their own; its raw text still joins the template value.
          value += '${';
          i += 2;
          let depth = 1;
          const exprStart = i;
          while (i < text.length && depth > 0) {
            const c = text[i];
            const n = text[i + 1];
            if (c === '/' && n === '/') { const nl = text.indexOf('\n', i); i = nl === -1 ? text.length : nl; continue; }
            if (c === '/' && n === '*') { const close = text.indexOf('*/', i + 2); i = close === -1 ? text.length : close + 2; continue; }
            if (c === "'" || c === '"' || c === '`') {
              const inner = jsQuotedLiteralAt(text, i);
              literals.push({ value: inner.value, start: inner.start, end: inner.end });
              i = inner.end;
              continue;
            }
            if (c === '{') depth += 1;
            if (c === '}') depth -= 1;
            i += 1;
          }
          value += text.slice(exprStart, i);
          continue;
        }
        value += text[i];
        i += 1;
      }
      literals.push({ value, start, end: i });
      continue;
    }
    i += 1;
  }
  return literals;
}

// Scan exactly one quoted/template literal starting at `start` and return its
// value and end offset (templates keep their raw inner text, escapes intact).
function jsQuotedLiteralAt(text, start) {
  const quote = text[start];
  let i = start + 1;
  let value = '';
  while (i < text.length) {
    if (text[i] === '\\') { value += text.slice(i, i + 2); i += 2; continue; }
    if (text[i] === quote) { i += 1; break; }
    if (quote !== '`' && text[i] === '\n') break;
    value += text[i];
    i += 1;
  }
  return { value, start, end: i };
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
  const source = stripJsComments(text);
  const importExportFromPattern = /(?:^|\n)\s*(?:import|export)\s+(?:[^;]*?\s+from\s*)['"]([^'"\n]+)['"]/g;
  for (const match of source.matchAll(importExportFromPattern)) specs.push(match[1]);
  const sideEffectImportPattern = /(?:^|\n)\s*import\s*['"]([^'"\n]+)['"]/g;
  for (const match of source.matchAll(sideEffectImportPattern)) specs.push(match[1]);
  return specs;
}

function resolveRelativeJsImport(baseFile, specifier) {
  if (!specifier.startsWith('.')) return null;
  const cleanSpecifier = specifier.split(/[?#]/, 1)[0];
  const target = join(dirname(baseFile), cleanSpecifier);
  const candidates = /\.[cm]?js$/i.test(cleanSpecifier)
    ? [target]
    : [target, `${target}.js`, `${target}.mjs`];
  return candidates.find((candidate) => existsSync(candidate) && statSync(candidate).isFile()) || null;
}

function relativeImportExists(baseFile, specifier) {
  if (!specifier.startsWith('.')) return true;
  const cleanSpecifier = specifier.split(/[?#]/, 1)[0];
  const target = join(dirname(baseFile), cleanSpecifier);
  if (/\.[cm]?js$/i.test(cleanSpecifier) || cleanSpecifier.endsWith('.css') || cleanSpecifier.endsWith('.html')) {
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
  for (const literal of jsQuotedLiterals(text)) {
    if (literal.value.includes('${')) continue;
    values.push(literal.value);
  }
  return values;
}

function maskAllowedFocusTaskSessionStorage(text) {
  return String(text || '')
    .replace(
      /\b(?:window\.|parent\.)?sessionStorage\.setItem\s*\(\s*(['"])ctox\.businessOs\.focusTask\1/g,
      'ctoxFocusTaskHandoff(',
    )
    .replace(
      /\b(?:window\.|parent\.)?sessionStorage\[\s*(['"])ctox\.businessOs\.focusTask\1\s*\]\s*=/g,
      'ctoxFocusTaskHandoff =',
    );
}

function containsForbiddenBrowserPersistence(text) {
  return /\b(?:localStorage|sessionStorage|indexedDB)\b/.test(maskAllowedFocusTaskSessionStorage(text));
}

function htmlDataActions(html) {
  const actions = new Set();
  for (const match of String(html || '').matchAll(/\bdata-[a-z0-9-]*action\s*=\s*(['"])([^'"]+)\1/gi)) {
    const action = match[2].trim();
    if (action) actions.add(action);
  }
  return actions;
}

function isPrimaryCreateActionValue(action) {
  const value = String(action || '').trim().toLowerCase();
  if (!value || /(?:^|[-_:])(?:follow-?up|create-?follow-?up)(?:$|[-_:])/.test(value)) {
    return false;
  }
  return /^(?:add|new|create|create-record|new-record|add-record)(?:$|[-_:])/.test(value);
}

function hasPrimaryCreateAffordance(html, indexJs) {
  const combined = `${html || ''}\n${indexJs || ''}`;
  if (/\bdata-[a-z0-9]+-add\b/i.test(combined)) return true;
  for (const action of htmlDataActions(html)) {
    if (isPrimaryCreateActionValue(action)) return true;
  }
  if (/\bdata-action\s*=\s*(['"])(?!(?:follow-?up|create-?follow-?up)\1)(?:add|new|create|create-record|new-record|add-record)[^'"]*\1/i.test(html)) {
    return true;
  }
  return /\b(?:add|new|create)(?:Primary)?Record\s*\(/i.test(indexJs)
    || /\bopen(?:Form|Modal)\s*\(\s*['"`](?:add|new|create|record|primary)/i.test(indexJs);
}

function nonEmptyHostHtmlAssignmentExists(indexJs) {
  const source = stripJsComments(indexJs);
  const pattern = /\b(?:ctx\.)?host\s*\.\s*innerHTML\s*=\s*([^;\n]+)/g;
  for (const match of source.matchAll(pattern)) {
    const rhs = String(match[1] || '').trim();
    if (!/^(['"`])\s*\1$/.test(rhs)) return true;
  }
  return false;
}

function writesToHostDom(indexJs) {
  const source = stripJsComments(indexJs);
  return nonEmptyHostHtmlAssignmentExists(indexJs)
    || /\b(?:ctx\.)?host\s*\.\s*insertAdjacentHTML\s*\(/.test(source)
    || /\b(?:ctx\.)?host\s*\.\s*replaceChildren\s*\(/.test(source)
    || /\b(?:ctx\.)?host\s*\.\s*append(?:Child)?\s*\(/.test(source);
}

function fetchesOwnIndexHtml(indexJs) {
  const source = stripJsComments(indexJs);
  return /\bfetch\s*\(\s*new\s+URL\s*\(\s*['"]\.\/index\.html['"]\s*,\s*import\.meta\.url\s*\)/.test(source)
    || /\bfetch\s*\(\s*['"]\.\/index\.html['"]/.test(source);
}

function rendersPrimaryCreateMarkupInHost(indexJs) {
  const source = stripJsComments(indexJs);
  const primaryCreateAction = String.raw`data-[a-z0-9-]*action\s*=\s*['"](?:add|new|create)(?:[-_:][^'"]*)?['"]`;
  const hostHtmlWrite = String.raw`\b(?:ctx\.)?host\s*\.\s*(?:innerHTML|insertAdjacentHTML)\b`;
  return new RegExp(String.raw`${hostHtmlWrite}[\s\S]{0,3000}${primaryCreateAction}`, 'i').test(source);
}

function collectInstalledMountMarkupFailures(indexHtml, indexJs) {
  if (!hasPrimaryCreateAffordance(indexHtml, indexJs)) return [];
  if ((fetchesOwnIndexHtml(indexJs) && writesToHostDom(indexJs))
    || rendersPrimaryCreateMarkupInHost(indexJs)) {
    return [];
  }
  return [
    'installed module mount(ctx) must load index.html into ctx.host or render an equivalent primary create UI; the Business OS shell does not preload runtime module index.html',
  ];
}

function collectHiddenModalClasses(html) {
  const classes = new Set();
  const tagPattern = /<[^>]*\bhidden\b[^>]*\bclass\s*=\s*(['"])([^'"]+)\1[^>]*>|<[^>]*\bclass\s*=\s*(['"])([^'"]+)\3[^>]*\bhidden\b[^>]*>/gi;
  for (const match of String(html || '').matchAll(tagPattern)) {
    const raw = match[2] || match[4] || '';
    for (const cls of raw.split(/\s+/).map((item) => item.trim()).filter(Boolean)) {
      if (/modal/i.test(cls)) classes.add(cls);
    }
  }
  return Array.from(classes);
}

function cssHasDisplayRuleForClass(css, className) {
  return cssRules(css).some((rule) => cssSelectorContainsClass(rule.selector, className) && /\bdisplay\s*:/.test(rule.body));
}

function cssHidesHiddenClass(css, className) {
  const globalHidden = /\[\s*hidden\s*\][^{]*\{[^}]*\bdisplay\s*:\s*none\b/i;
  return globalHidden.test(css) || cssRules(css).some((rule) => (
    cssSelectorContainsHiddenClass(rule.selector, className)
    && /\bdisplay\s*:\s*none\b/i.test(rule.body)
  ));
}

function collectHiddenModalFailures(indexHtml, indexCss) {
  const messages = [];
  for (const className of collectHiddenModalClasses(indexHtml)) {
    if (!cssHasDisplayRuleForClass(indexCss, className)) continue;
    if (!cssHidesHiddenClass(indexCss, className)) {
      messages.push(`hidden modal .${className} has a display rule but no CSS rule that hides .${className}[hidden]`);
    }
  }
  return messages;
}

function lineNumberForIndex(text, index) {
  return String(text || '').slice(0, Math.max(0, index)).split(/\r?\n/).length;
}

function collectDuplicateFunctionDeclarationFailures(file, text) {
  const declarations = new Map();
  const source = stripJsComments(text);
  const pattern = /\b(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)\s*\(/g;
  for (const match of source.matchAll(pattern)) {
    const name = match[1];
    if (!declarations.has(name)) declarations.set(name, []);
    declarations.get(name).push(lineNumberForIndex(source, match.index || 0));
  }
  const messages = [];
  for (const [name, lines] of declarations.entries()) {
    if (lines.length <= 1) continue;
    messages.push(`${rel(file)} declares function ${name} more than once (lines ${lines.join(', ')}); duplicate function names shadow helpers and can break browser mount`);
  }
  return messages;
}

function hasFormSubmitHandler(text) {
  return /\.addEventListener\s*\(\s*['"]submit['"]/.test(text)
    || /\bonSubmitForm\b/.test(text);
}

function hasVisibleSubmitOrSaveControl(text) {
  return /\btype\s*=\s*['"`]submit['"`]/.test(text)
    || /\bdata-[a-z0-9-]*action\s*=\s*['"`][^'"`]*\bsave\b[^'"`]*['"`]/i.test(text)
    || />\s*(?:Save|Speichern)\s*</i.test(text);
}

function hasCommandBusDispatchInvocation(text) {
  const source = stripJsComments(text);
  const commandBusAccess = String.raw`(?:ctx|state\s*\.\s*ctx)\??\.\s*commandBus`;
  const dispatchAccess = String.raw`${commandBusAccess}\??\.\s*dispatch`;
  if (new RegExp(String.raw`\b${dispatchAccess}\s*(?:\(|\?\.\s*\()`).test(source)) return true;

  const dispatchAliases = new Set();
  for (const match of source.matchAll(new RegExp(String.raw`\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*${dispatchAccess}\b`, 'g'))) {
    dispatchAliases.add(match[1]);
  }
  for (const alias of dispatchAliases) {
    if (containsIdentifierCall(source, alias)) return true;
  }

  const busAliases = new Set();
  for (const match of source.matchAll(new RegExp(String.raw`\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*${commandBusAccess}\b`, 'g'))) {
    busAliases.add(match[1]);
  }
  for (const alias of busAliases) {
    if (new RegExp(String.raw`\b${escapeRegExp(alias)}\??\.\s*dispatch\s*(?:\(|\?\.\s*\()`).test(source)) return true;
  }

  return false;
}

function collectLegacyDbFacadeFailures(file, text) {
  const source = stripJsComments(text);
  const messages = [];
  const seen = new Set();
  const dbAccess = String.raw`(?:ctx|state\s*(?:\?\.|\.)\s*ctx)\s*(?:\?\.|\.)\s*db`;

  function add(message) {
    if (!seen.has(message)) {
      seen.add(message);
      messages.push(message);
    }
  }

  const directPropertyPattern = new RegExp(String.raw`${dbAccess}\s*(?:\?\.|\.)\s*([A-Za-z_$][\w$]*)`, 'g');
  for (const match of source.matchAll(directPropertyPattern)) {
    const property = match[1];
    if (property === 'collection') continue;
    if (property === 'collections') {
      add(`${rel(file)} uses legacy ctx.db.collections fallback; use ctx.db.collection('<collection>')`);
    } else if (property === 'registerSchemas') {
      add(`${rel(file)} calls ctx.db.registerSchemas from app code; declare schemas in collections.schema.json and schema.js`);
    } else if (property === 'raw') {
      add(`${rel(file)} uses raw ctx.db access; use ctx.db.collection('<collection>')`);
    } else {
      add(`${rel(file)} uses direct ctx.db.${property} access; use ctx.db.collection('<collection>')`);
    }
  }

  const bracketPattern = new RegExp(String.raw`${dbAccess}\s*(?:\?\.)?\s*\[`, 'g');
  if (bracketPattern.test(source)) {
    add(`${rel(file)} uses bracket ctx.db[...] collection access; use ctx.db.collection('<collection>')`);
  }

  const aliasPattern = new RegExp(String.raw`\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*${dbAccess}\b`, 'g');
  const aliases = new Set();
  for (const match of source.matchAll(aliasPattern)) {
    const next = source.slice((match.index || 0) + match[0].length).trimStart();
    if (next.startsWith('.') || next.startsWith('?.')) continue;
    aliases.add(match[1]);
    add(`${rel(file)} caches the ctx.db facade in ${match[1]}; keep collection handles from ctx.db.collection('<collection>')`);
  }

  for (const alias of aliases) {
    const escaped = escapeRegExp(alias);
    const aliasPropertyPattern = new RegExp(String.raw`\b${escaped}\s*(?:\?\.|\.)\s*([A-Za-z_$][\w$]*)`, 'g');
    for (const match of source.matchAll(aliasPropertyPattern)) {
      const property = match[1];
      if (property === 'collection') continue;
      if (property === 'collections') {
        add(`${rel(file)} uses legacy ${alias}.collections fallback; use ctx.db.collection('<collection>')`);
      } else if (property === 'registerSchemas') {
        add(`${rel(file)} calls ${alias}.registerSchemas from app code; declare schemas in collections.schema.json and schema.js`);
      } else if (property === 'raw') {
        add(`${rel(file)} uses raw ${alias}.raw access; use ctx.db.collection('<collection>')`);
      } else {
        add(`${rel(file)} uses direct ${alias}.${property} access; use ctx.db.collection('<collection>')`);
      }
    }
    const aliasBracketPattern = new RegExp(String.raw`\b${escaped}\s*(?:\?\.)?\s*\[`, 'g');
    if (aliasBracketPattern.test(source)) {
      add(`${rel(file)} uses bracket ${alias}[...] collection access; use ctx.db.collection('<collection>')`);
    }
  }

  return messages;
}

function indexJsHandlesDataAction(indexJs, action) {
  return jsContainsDataActionSelector(indexJs, action)
    || jsComparesAgainstLiteral(indexJs, action)
    || jsObjectKeyLiteralExists(indexJs, action);
}

function htmlDataActionIsSubmitControl(indexHtml, action) {
  for (const tag of String(indexHtml || '').matchAll(/<\s*(?:button|input)\b[^>]*>/gi)) {
    const attrs = tag[0] || '';
    if (htmlAttributeValue(attrs, 'type') !== 'submit') continue;
    const actionValue = htmlDataActionValue(attrs);
    if (actionValue === action) return true;
  }
  return false;
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
    return jsCommandTypeReferencesIdentifier(text, name);
  });
}

function jsContainsDataActionSelector(source, action) {
  const expected = String(action || '');
  for (const literal of jsQuotedLiterals(source)) {
    if (!literal.value.includes('[data-') || !literal.value.includes('action')) continue;
    if (htmlDataActionValue(literal.value) === expected) return true;
  }
  return false;
}

function jsComparesAgainstLiteral(source, action) {
  const text = String(source || '');
  const expected = String(action || '');
  for (const literal of jsQuotedLiterals(text)) {
    if (literal.value !== expected) continue;
    const before = text.slice(Math.max(0, literal.start - 12), literal.start).trimEnd();
    if (before.endsWith('===') || before.endsWith('==') || /\bcase$/.test(before)) return true;
  }
  return false;
}

function jsObjectKeyLiteralExists(source, action) {
  const text = String(source || '');
  const expected = String(action || '');
  for (const literal of jsQuotedLiterals(text)) {
    if (literal.value !== expected) continue;
    if (text.slice(literal.end).trimStart().startsWith(':')) return true;
  }
  return false;
}

function htmlAttributeValue(tag, name) {
  const lowerName = String(name || '').toLowerCase();
  for (const match of String(tag || '').matchAll(/\b([A-Za-z0-9_:-]+)\s*=\s*(['"])(.*?)\2/g)) {
    if ((match[1] || '').toLowerCase() === lowerName) return match[3] || '';
  }
  return '';
}

function htmlDataActionValue(tag) {
  for (const match of String(tag || '').matchAll(/\b(data-[A-Za-z0-9_-]*action)\s*=\s*(['"])(.*?)\2/g)) {
    if ((match[1] || '').toLowerCase().endsWith('action')) return match[3] || '';
  }
  return '';
}

function jsCommandTypeReferencesIdentifier(source, name) {
  const text = String(source || '');
  const expected = String(name || '');
  if (!expected) return false;
  for (const match of text.matchAll(/(?:command_type|["']command_type["'])\s*:/g)) {
    const valueStart = (match.index || 0) + match[0].length;
    const value = jsIdentifierAt(text, valueStart + text.slice(valueStart).search(/\S|$/));
    if (value === expected) return true;
  }
  return false;
}

function hasCtoxTicketCommandType(text) {
  return /(?:command_type\s*:\s*['"]ctox\.ticket\.[^'"]+['"]|["']command_type["']\s*:\s*["']ctox\.ticket\.[^'"]+["'])/.test(text);
}

const COLOR_BEARING_PROPERTY = /^(?:background|background-color|color|fill|stroke|border(?:-(?:top|right|bottom|left))?(?:-color)?|outline(?:-color)?)$/;
const COLOR_LITERAL = /#[0-9a-f]{3,8}\b|\brgba?\s*\(|\bhsla?\s*\(|\boklch\s*\(|\boklab\s*\(/i;
// Neutral alpha overlays (pure black/white shadows and scrims) are
// theme-independent and allowed.
const NEUTRAL_ALPHA = /^rgba?\(\s*(?:0|255)\s*,\s*(?:0|255)\s*,\s*(?:0|255)\s*[,)]/i;

function collectHardcodedColorFailures(indexCss) {
  const css = stripJsComments(indexCss);
  const offenders = [];
  for (const match of css.matchAll(/([a-z-]+)\s*:\s*([^;{}]+)/gi)) {
    const property = match[1].trim().toLowerCase();
    const value = match[2].trim();
    if (!COLOR_BEARING_PROPERTY.test(property)) continue;
    if (!COLOR_LITERAL.test(value)) continue;
    if (/var\s*\(/.test(value)) continue;
    if (NEUTRAL_ALPHA.test(value)) continue;
    offenders.push(`${property}: ${value.length > 48 ? `${value.slice(0, 45)}...` : value}`);
    if (offenders.length >= 8) break;
  }
  if (offenders.length === 0) return [];
  return [
    `index.css hard-codes theme colors (${offenders.join('; ')}); every surface/text/border/accent color must come from Business OS tokens via var(--bg), var(--surface), var(--text), var(--muted), var(--line), var(--accent), status tokens, or color-mix over them — see design-guide Token Contract`,
  ];
}

function collectKitUsageFailures(runtimeText) {
  if (/\bctox-[a-z]/.test(String(runtimeText || ''))) return [];
  return [
    'app renders no shared kit classes (ctox-*); build the frame and controls from shared/base.css — .ctox-pane/.ctox-pane-header, .ctox-button/.ctox-pane-icon, .ctox-input/.ctox-select, .ctox-table, .ctox-fields, .ctox-badge, .ctox-modal, .ctox-empty — instead of app-local rebuilds; see design-guide Component Kit',
  ];
}

function collectThemeTokenFailures(indexCss) {
  const css = stripJsComments(indexCss);
  const messages = [];
  if (/\bcolor-scheme\s*:/.test(css)) {
    messages.push('index.css must not force color-scheme; inherit the Business OS light/dark theme from the shell');
  }
  if (/(?:^|})\s*(?::root|html)\s*(?:\[[^\]]+\])?\s*\{[^}]*--(?:bg|surface|surface-2|line|text|text-strong|muted|accent|accent-soft|accent-foreground|danger|warning|success|focus-ring)\s*:/is.test(css)) {
    messages.push('index.css must not redefine root Business OS design tokens; consume shell-provided tokens so workspace branding can override them');
  }
  if (/(?:^|})\s*(?:html|body)\s*(?:\[[^\]]+\])?\s*\{[^}]*\bbackground(?:-color)?\s*:\s*(?:#[0-9a-f]{3,8}|rgb[a]?\(|hsl[a]?\(|oklch\(|oklab\()/is.test(css)) {
    messages.push('index.css must not hard-code root page surfaces on html/body; use module containers with var(--bg), var(--surface), or var(--surface-2)');
  }
  if (!/\bvar\s*\(\s*--(?:bg|surface|surface-2)\b/.test(css)) {
    messages.push('index.css must use Business OS surface tokens such as var(--bg), var(--surface), or var(--surface-2) so light and dark themes render correctly');
  }
  if (!/\bvar\s*\(\s*--(?:text|text-strong|muted)\b/.test(css)) {
    messages.push('index.css must use Business OS text tokens such as var(--text), var(--text-strong), or var(--muted) so light and dark themes render correctly');
  }
  return messages;
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
  ...(installedMode && !catalogInstalledMode ? ['core/automation.mjs', 'core/records.mjs'] : []),
];

for (const file of requiredFiles) {
  if (!existsSync(join(moduleDir, file))) fail(`missing ${rel(join(moduleDir, file))}`);
}

if (runtimeModuleMode && !catalogInstalledMode && existsSync(moduleDir)) {
  for (const name of readdirSync(moduleDir)) {
    const path = join(moduleDir, name);
    const stats = statSync(path);
    const allowed = stats.isDirectory()
      ? allowedInstalledRootDirs.has(name)
      : allowedInstalledRootFiles.has(name);
    if (!allowed) fail(`unexpected runtime-module root entry: ${rel(path)}`);
  }
}

const manifest = existsSync(join(moduleDir, 'module.json')) ? readJson(join(moduleDir, 'module.json')) : null;
const schemaDoc = existsSync(join(moduleDir, 'collections.schema.json'))
  ? readJson(join(moduleDir, 'collections.schema.json'))
  : null;
const sourceInstallScope = String(manifest?.install_scope || '').trim().toLowerCase();
const sourceShellModuleMode = catalogInstalledMode || (!runtimeModuleMode
  && ['core', 'internal', 'local', 'starter'].includes(sourceInstallScope));

if (manifest) {
  if (manifest.id !== moduleId) fail(`module.json id must be ${moduleId}`);
  if (manifest.entry !== expectedEntry) fail(`module.json entry must be ${expectedEntry}`);
  if (runtimeModuleMode && manifest.install_scope !== expectedInstallScope) {
    fail(`module.json install_scope must be ${expectedInstallScope}`);
  }
  if (!Array.isArray(manifest.collections)) fail('module.json collections must be an array');
  if (runtimeModuleMode && !catalogInstalledMode && Array.isArray(manifest.collections)) {
    for (const name of manifest.collections) {
      if (shellCollections.has(name)) continue;
      if (!isModuleScopedCollectionName(name, moduleId)) {
        fail(`module.json collection ${name} must be scoped to module id ${moduleId}; use ${normalizedModuleCollectionPrefix(moduleId)}_<name>`);
      }
    }
  }
  if (installedMode) {
    const presentationShell = String(manifest.layout?.shell || '').trim();
    if (!['windowed', 'desktop-window'].includes(presentationShell)) {
      fail('module.json layout.shell must be windowed for runtime-installed apps; full-workspace is a legacy source-only compatibility mode');
    }
    const launchKind = String(manifest.launch_kind || '').trim();
    if (launchKind !== 'desktop-app') {
      fail('runtime apps must set root module.json launch_kind=desktop-app');
    }
    if (manifest.presentation == null) {
      fail('runtime apps must declare the canonical module.json presentation contract');
    } else {
      const presentation = manifest.presentation;
      if (!presentation || typeof presentation !== 'object' || Array.isArray(presentation)) {
        fail('module.json presentation must be an object');
      } else {
        const modes = new Set(['window', 'maximized', 'focus']);
        if (!modes.has(presentation.default_mode)) {
          fail('module.json presentation.default_mode must be window, maximized, or focus');
        }
        if (!Array.isArray(presentation.supported_modes)
          || !presentation.supported_modes.includes('window')
          || presentation.supported_modes.some((mode) => !modes.has(mode))) {
          fail('module.json presentation.supported_modes must contain window and only supported modes');
        }
        for (const [key, minimum] of [['initial_size', 1], ['minimum_size', 1]]) {
          if (!Number.isInteger(presentation[key]?.width) || presentation[key].width < minimum
            || !Number.isInteger(presentation[key]?.height) || presentation[key].height < minimum) {
            fail(`module.json presentation.${key} must contain positive integer width and height`);
          }
        }
        if (presentation.minimum_size?.width !== 640 || presentation.minimum_size?.height !== 480) {
          fail('module.json presentation.minimum_size must be exactly 640x480; the shell switches to mobile-sheet presentation below the floating-window contract');
        }
      }
    }
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
    if (manifest.icon !== 'icon.svg') {
      fail('module.json icon must be icon.svg for runtime-installed modules');
    }
    if (Object.prototype.hasOwnProperty.call(manifest, 'icon_path') || Object.prototype.hasOwnProperty.call(manifest, 'iconPath')) {
      fail('module.json icon_path is forbidden for runtime-installed modules; use icon: "icon.svg"');
    }
    if (Object.prototype.hasOwnProperty.call(manifest, 'icon_url') || Object.prototype.hasOwnProperty.call(manifest, 'iconUrl')) {
      fail('module.json icon_url is forbidden for runtime-installed modules; use local icon.svg');
    }
  }
  if (!sourceShellModuleMode && manifest.layout?.right && !manifest.layout?.third_pane_justification) {
    fail('module.json layout.right requires layout.third_pane_justification');
  }
  if (!sourceShellModuleMode && Object.prototype.hasOwnProperty.call(manifest.layout || {}, 'right_resizer')) {
    fail('module.json layout.right_resizer is forbidden');
  }
  if (installedMode && (manifest.layout?.icon_svg || manifest.icon_svg || manifest.iconSvg)) {
    fail('module.json inline icon fields are forbidden; keep SVG markup in icon.svg');
  }
  const manifestText = JSON.stringify(manifest);
  if (installedMode && /<\s*svg\b/i.test(manifestText)) {
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
  if (runtimeModuleMode && !catalogInstalledMode && schemaDoc.collections && typeof schemaDoc.collections === 'object' && !Array.isArray(schemaDoc.collections)) {
    for (const name of Object.keys(schemaDoc.collections)) {
      if (!isModuleScopedCollectionName(name, moduleId)) {
        fail(`collections.schema.json collection ${name} must be scoped to module id ${moduleId}; use ${normalizedModuleCollectionPrefix(moduleId)}_<name>`);
      }
    }
    for (const message of collectDeclarativeMigrationFailures(schemaDoc)) fail(message);
    for (const message of collectSyncProfileFailures(schemaDoc)) fail(message);
  }
}

if (manifest && schemaDoc?.collections) {
  for (const name of Object.keys(schemaDoc.collections)) {
    if (!sourceShellModuleMode && shellCollections.has(name)) fail(`collections.schema.json redeclares shell collection ${name}`);
  }
  for (const name of manifest.collections || []) {
    if (!shellCollections.has(name) && !schemaDoc.collections[name]) {
      fail(`collections.schema.json missing non-shell collection from module.json: ${name}`);
    }
  }
}

const schemaJsPath = join(moduleDir, 'schema.js');
let schemaJsCollections = null;
if (existsSync(schemaJsPath)) {
  const schemaJs = readText(schemaJsPath);
  for (const collection of shellCollections) {
    const pattern = new RegExp(String.raw`(?:^|[,{]\s*)(?:['"]${collection}['"]|${collection})\s*:`, 'm');
    if (!sourceShellModuleMode && pattern.test(schemaJs)) fail(`schema.js exports shell-registered collection key ${collection}`);
  }
  if (runtimeModuleMode && !catalogInstalledMode && schemaDoc?.collections) {
    schemaJsCollections = await loadSchemaJsCollections(schemaJsPath);
    if (schemaJsCollections) {
      for (const message of collectSchemaJsParityFailures(schemaDoc, schemaJsCollections)) fail(message);
    }
  }
}

if (runtimeModuleMode && !catalogInstalledMode && schemaDoc?.collections) {
  for (const message of await collectRecordHelperSchemaFailures(moduleDir, schemaDoc)) fail(message);
}

// System modules the server registers directly (src/core/service/business_os.rs
// serves them without a registry.json entry).
const SERVER_REGISTERED_MODULES = new Set([
  'app-store', 'browser', 'coding-agents', 'creator', 'credentials', 'ctox',
]);

if (!runtimeModuleMode && manifest && existsSync(registryPath)) {
  const registry = readJson(registryPath);
  const entry = (registry?.modules || []).find((item) => item.id === moduleId);
  if (!entry && !SERVER_REGISTERED_MODULES.has(moduleId)) fail(`registry.json missing module ${moduleId}`);
  else if (entry) {
    if (entry.entry !== manifest.entry) fail(`registry entry mismatch for ${moduleId}: entry`);
    if (entry.install_scope !== manifest.install_scope) {
      fail(`registry entry mismatch for ${moduleId}: install_scope`);
    }
  }
}

const files = walk(moduleDir);
const jsFiles = files.filter((file) => /\.(?:js|mjs)$/.test(file));
const isTestFile = (file) => hasSegment(file, 'tests')
  || /(?:^|[/\\])test\.[cm]?js$|\.test\.[cm]?js$|-smoke\.[cm]?js$/i.test(file);
const runtimeFiles = files.filter((file) =>
  /\.(?:js|mjs|html|css)$/.test(file) && !isTestFile(file)
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
    if (!relativeImportExists(path, specifier) && !catalogInstalledMode) {
      fail(`${rel(path)} relative import ${specifier} does not exist`);
    }
    if (!specifier.startsWith('.') && !isTestFile(path)) {
      fail(`${rel(path)} imports bare package ${specifier}; Business OS apps must use browser ESM local files only`);
    }
  }
}
for (const message of collectEsmImportExportFailures(jsFiles)) fail(`ESM import/export mismatch: ${message}`);

const indexJsPath = join(moduleDir, 'index.js');
const indexHtmlPath = join(moduleDir, 'index.html');
const indexCssPath = join(moduleDir, 'index.css');
const indexJs = existsSync(indexJsPath) ? readText(indexJsPath) : '';
const indexHtml = existsSync(indexHtmlPath) ? readText(indexHtmlPath) : '';
const indexCss = existsSync(indexCssPath) ? readText(indexCssPath) : '';
const runtimeText = runtimeFiles.map((file) => readText(file)).join('\n');
const nonTestModuleText = files
  .filter((file) => /\.(?:js|mjs|html|css|json)$/.test(file))
  .filter((file) => !isTestFile(file))
  .map((file) => readText(file))
  .join('\n');

if (!/\bexport\s+(?:async\s+)?function\s+mount\s*\(|\bexport\s*\{[^}]*\bmount\b/.test(indexJs)) {
  fail('index.js must export mount(ctx)');
}
if (!/\bctx\.host\b|\bhost\.innerHTML\b|\bhost\.append/.test(indexJs)) {
  fail('index.js must render into ctx.host');
}

if (runtimeModuleMode && !catalogInstalledMode) {
  for (const message of collectThemeTokenFailures(indexCss)) fail(message);
  for (const message of collectHardcodedColorFailures(indexCss)) fail(message);
  for (const message of collectKitUsageFailures(runtimeText)) fail(message);
}

if (installedMode && !catalogInstalledMode) {
  if (!/\bctx\??\.db\b|\bstate\.ctx\??\.db\b/.test(runtimeText)) {
    fail('installed module must persist records through the shell-provided ctx.db collection handle');
  }
  if (!hasCommandBusDispatchInvocation(runtimeText)) {
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
  if (!hasPrimaryCreateAffordance(indexHtml, indexJs)) {
    fail('installed module must expose a primary create action for its main business record');
  }
  for (const message of collectInstalledMountMarkupFailures(indexHtml, indexJs)) fail(message);
  if (hasFormSubmitHandler(indexJs) && !hasVisibleSubmitOrSaveControl(runtimeText)) {
    fail('installed module wires a form submit handler but renders no visible submit/save control for the form');
  }
}

const runtimeRules = [
  ['browser storage data path outside CTOX focus handoff', containsForbiddenBrowserPersistence],
  ['Business OS HTTP data path', /fetch\s*\(\s*['"]\/(?:api|rxdb|business-os)/],
  ['direct business_commands write', /collection\s*\(\s*['"]business_commands['"]\s*\)|business_commands[\s\S]{0,120}\b(?:insert|upsert|bulk)\s*\(/],
  ['legacy command type property', /\btype\s*:\s*['"](?:ats|browser|business_os|ctox|customers|document|invoices|knowledge|matching|outbound|research|spreadsheet|support|threads|web_stack)\./],
  ['upstream rxdb import', /from\s+['"]rxdb['"]/],
  ['CommonJS require', /\brequire\s*\(/],
  ['Node runtime import', /from\s+['"]node:/],
  ['remote URL dependency', /(?:from\s+|import\s*\(|src\s*=\s*)['"]https?:\/\//i],
  ['React framework runtime', /\bReact(?:DOM)?\.|\bcreateRoot\s*\(|from\s+['"][^'"]*react(?:\/|['"])/i],
  ['Vue framework runtime', /\bVue\.|\bcreateApp\s*\(|from\s+['"][^'"]*vue(?:\/|['"])/i],
  ['Svelte framework runtime', /from\s+['"][^'"]*svelte(?:\/|['"])/i],
  ['Angular framework runtime', /from\s+['"][^'"]*@angular(?:\/|['"])/i],
  ['JSX runtime marker', /jsx-runtime|\/\*\s*@jsx/i],
  ['legacy shell event dispatch', /window\.dispatchEvent\s*\(|ctox-business-os-chat-submit/],
];
for (const path of runtimeFiles) {
  const text = readText(path);
  const executableText = stripJsComments(text);
  if (!sourceShellModuleMode) {
    for (const [label, check] of runtimeRules) {
      const matched = typeof check === 'function' ? check(executableText) : check.test(executableText);
      if (matched) fail(`${rel(path)} contains forbidden runtime pattern: ${label}`);
    }
  }
  if (runtimeModuleMode && !catalogInstalledMode && /\.(?:js|mjs)$/.test(path)) {
    for (const message of collectLegacyDbFacadeFailures(path, text)) fail(message);
  }
  if (/\.(?:js|mjs)$/.test(path)) {
    for (const message of collectDuplicateFunctionDeclarationFailures(path, text)) fail(message);
  }
}

if (/<!doctype\b|<\s*html\b|<\s*head\b|<\s*body\b/i.test(indexHtml)) {
  fail('index.html must be a shell fragment, not a full HTML document');
}
if (/<\s*(?:link|script|meta|title|style)\b/i.test(indexHtml)) {
  fail('index.html must not include document/head resource tags such as <link>, <script>, <meta>, <title>, or <style>');
}
for (const message of collectHiddenModalFailures(indexHtml, indexCss)) fail(message);
for (const action of htmlDataActions(indexHtml)) {
  if (htmlDataActionIsSubmitControl(indexHtml, action) && hasFormSubmitHandler(indexJs)) {
    continue;
  }
  if (!indexJsHandlesDataAction(runtimeText, action)) {
    fail(`index.html declares data-action="${action}" but index.js has no visible handler for it`);
  }
}

// Column grammar: a view band exists only with >= 2 real views. A band with a
// single counted tab reads as a stray filter chip (an illegal standing badge);
// a single view's count belongs in the pane footer.
for (const band of String(indexHtml || '').matchAll(/<(?:div|nav)[^>]*class="[^"]*ctox-pane-tabs[^"]*"[^>]*>([\s\S]*?)<\/(?:div|nav)>/g)) {
  const tabCount = (band[1].match(/class="[^"]*ctox-pane-tab[^"]*"/g) || []).length;
  if (tabCount === 1) {
    fail('a .ctox-pane-tabs view band with a single tab is a stray chip — bands need >= 2 real views; put a lone count into the pane footer');
  }
}

if (manifest && runtimeModuleMode && !catalogInstalledMode) {
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

console.log(`Business OS module static check OK: ${moduleId} (${localMode ? 'local' : catalogInstalledMode ? 'catalog-installed' : installedMode ? 'installed' : 'source'} mode)`);
