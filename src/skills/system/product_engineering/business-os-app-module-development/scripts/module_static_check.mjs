#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

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
  const escaped = escapeRegExp(className);
  return new RegExp(String.raw`\.[\w-]*\s*\.?${escaped}\b[^{]*\{[^}]*\bdisplay\s*:`, 'i').test(css)
    || new RegExp(String.raw`\.${escaped}\b[^{]*\{[^}]*\bdisplay\s*:`, 'i').test(css);
}

function cssHidesHiddenClass(css, className) {
  const escaped = escapeRegExp(className);
  const classHidden = new RegExp(String.raw`\.${escaped}\s*\[\s*hidden\s*\][^{]*\{[^}]*\bdisplay\s*:\s*none\b`, 'i');
  const scopedClassHidden = new RegExp(String.raw`\.[\w-]+\s+\.${escaped}\s*\[\s*hidden\s*\][^{]*\{[^}]*\bdisplay\s*:\s*none\b`, 'i');
  const globalHidden = /\[\s*hidden\s*\][^{]*\{[^}]*\bdisplay\s*:\s*none\b/i;
  return classHidden.test(css) || scopedClassHidden.test(css) || globalHidden.test(css);
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
    if (new RegExp(String.raw`\b${escapeRegExp(alias)}\s*(?:\(|\?\.\s*\()`).test(source)) return true;
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
  const escaped = escapeRegExp(action);
  return new RegExp(String.raw`\[data-[a-z0-9-]*action\s*=\s*["']${escaped}["']\]`, 'i').test(indexJs)
    || new RegExp(String.raw`(?:===|==|case)\s*['"\`]${escaped}['"\`]`).test(indexJs)
    || new RegExp(String.raw`['"\`]${escaped}['"\`]\s*:`).test(indexJs);
}

function htmlDataActionIsSubmitControl(indexHtml, action) {
  const escaped = escapeRegExp(action);
  const tagWithAction = new RegExp(
    String.raw`<\s*(?:button|input)\b(?=[^>]*\bdata-[a-z0-9-]*action\s*=\s*["']${escaped}["'])(?=[^>]*\btype\s*=\s*["']submit["'])[^>]*>`,
    'i',
  );
  return tagWithAction.test(indexHtml);
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
const sourceInstallScope = String(manifest?.install_scope || '').trim().toLowerCase();
const sourceShellModuleMode = !installedMode && ['core', 'internal', 'local', 'starter'].includes(sourceInstallScope);

if (manifest) {
  if (manifest.id !== moduleId) fail(`module.json id must be ${moduleId}`);
  if (manifest.entry !== expectedEntry) fail(`module.json entry must be ${expectedEntry}`);
  if (installedMode && manifest.install_scope !== expectedInstallScope) {
    fail(`module.json install_scope must be ${expectedInstallScope}`);
  }
  if (!Array.isArray(manifest.collections)) fail('module.json collections must be an array');
  if (installedMode && Array.isArray(manifest.collections)) {
    for (const name of manifest.collections) {
      if (shellCollections.has(name)) continue;
      if (!isModuleScopedCollectionName(name, moduleId)) {
        fail(`module.json collection ${name} must be scoped to module id ${moduleId}; use ${normalizedModuleCollectionPrefix(moduleId)}_<name>`);
      }
    }
  }
  if (installedMode) {
    if (manifest.layout?.shell !== 'full-workspace') {
      fail('module.json layout.shell must be full-workspace for runtime-installed apps; do not leave users in generic Kontext/Themen shell side panes');
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
  if (installedMode && schemaDoc.collections && typeof schemaDoc.collections === 'object' && !Array.isArray(schemaDoc.collections)) {
    for (const name of Object.keys(schemaDoc.collections)) {
      if (!isModuleScopedCollectionName(name, moduleId)) {
        fail(`collections.schema.json collection ${name} must be scoped to module id ${moduleId}; use ${normalizedModuleCollectionPrefix(moduleId)}_<name>`);
      }
    }
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
  if (installedMode && schemaDoc?.collections) {
    schemaJsCollections = await loadSchemaJsCollections(schemaJsPath);
    if (schemaJsCollections) {
      for (const message of collectSchemaJsParityFailures(schemaDoc, schemaJsCollections)) fail(message);
    }
  }
}

if (installedMode && schemaDoc?.collections) {
  for (const message of await collectRecordHelperSchemaFailures(moduleDir, schemaDoc)) fail(message);
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
    if (!specifier.startsWith('.') && !hasSegment(path, 'tests') && !path.endsWith('.test.mjs')) {
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
  for (const message of collectThemeTokenFailures(indexCss)) fail(message);
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
  if (!sourceShellModuleMode) {
    for (const [label, check] of runtimeRules) {
      const matched = typeof check === 'function' ? check(text) : check.test(text);
      if (matched) fail(`${rel(path)} contains forbidden runtime pattern: ${label}`);
    }
  }
  if (installedMode && /\.(?:js|mjs)$/.test(path)) {
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
