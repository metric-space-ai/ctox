// v4 counterpart of scripts/assert-module-conformance.mjs (CI line selection E37, see scripts/CI-V4-LINE.md).
// The V2 script stays byte-identical and runs on release/v2; this file runs on every other line. It is a copy
// of the V2 guard with these deliberate differences (everything else is checked exactly as in V2):
//
//   schema-covers-collections  the canonical schema source is modules/<id>/collections.schema.json (§16.13,
//                              App-API 4.0); the port deleted schema.js. Every collection listed in module.json
//                              must be declared there (same SHELL_REGISTERED exemptions as V2).
//   schema-import              collections.schema.json must exist and parse as {collections: {…}}.
//   schema-parity              fingerprints taken from collections.schema.json.
//   mount-signature            the v4 contract is mount(root, api, { restored } = {}) → {unmount, snapshot}
//                              (kernel/index.d.ts AppMount, v4/tools/port/PORTING.md R1-mount): exactly the two
//                              positionals <element>, api plus at most one destructured options object.
//                              mount(ctx) is now a violation (V2 contract), as is any other shape.
//   db-isolation-inventory     the V2 inventory guard (assert-db-isolation-inventory.mjs) still runs unchanged. Its
//                              only accepted finding is "desktop_apps: stale inventory entry for <id>" when <id> was
//                              ported to modules/<id> (desktop-apps/<id>/app.js gone, modules/<id>/module.json and
//                              v4/manifests/<id>.manifest.json present) – the inventory documents the V2 location of
//                              an app that moved; every other finding stays red. In addition the v4 form of the same
//                              invariant runs: v4/tools/isolation/check-isolation.mjs --rule R2 (every foreign
//                              collection access declared in foreign_collections, exported by its owner, write
//                              evidence requires 'write').
//   NO_V4 – "schema.js must import in Node and its collections count": the leftover V2 schema.js files
//          (ctox, conversations, outbound) are not loaded on the v4 line (the kernel registers collections from
//          collections.schema.json, §16.13) and are deleted with S9 (debt L-14). Line release/v2 checks them in the
//          unchanged V2 script.
//
// Business OS module conformance guard.
//
// Static checks over modules/<id>/ that enforce the module contract from
// README.md / ARCHITECTURE.md / shared/base.css:
//
//   schema-covers-collections  every collection listed in module.json is
//                              declared in the module's schema.js (the shell
//                              only pre-registers the critical collections);
//                              a missing declaration fails sync silently.
//                              Checked against the REAL schema export
//                              (dynamic import), so re-export patterns like
//                              modules/reports -> ../ctox/schema.js count.
//   schema-import              schema.js must be importable in Node (the
//                              module tests and this guard rely on it).
//   schema-parity              when two modules declare the same collection
//                              (cross-module reads via import/re-export),
//                              their schema definitions must be identical —
//                              whichever module registers first wins at
//                              runtime, so divergence would be a silent
//                              version/shape conflict.
//   mount-export               index.js exports mount().
//   mount-signature            mount takes the shell context as its single
//                              parameter: mount(ctx).
//   no-db-raw                  modules must not unwrap the shell DB facade
//                              (ctx.db.raw): raw handles go stale when the
//                              data plane recovers from schema drift.
//   no-indexeddb               IndexedDB is owned by shared/db.js.
//   css-no-root-tokens         module CSS must not define custom properties
//                              on :root — they leak into the shell and every
//                              other app once the stylesheet loads.
//   css-no-shell-token-redefinition
//                              module CSS must not redefine shell/base design
//                              tokens (--bg, --surface, --accent, ...); derive
//                              module-local names from them instead (see
//                              modules/customers/index.css).
//   css-no-self-referential-token
//                              module CSS custom properties must not assign
//                              themselves through var(--same-token).
//   css-no-cdn-import          no @import of remote stylesheets/fonts in the
//                              no-build runtime.
//   locales                    module ships locales/de.json + locales/en.json.
//
// The HTTP-path and upstream-rxdb-import rules live in assert-rxdb-only.mjs;
// this guard does not duplicate them.
// The DB-isolation inventory/drift rules live in
// assert-db-isolation-inventory.mjs and are invoked here so the standard module
// conformance gate fails on new unscoped raw/property/proxy/cache DB access.
//
// ALLOWLIST POLICY: entries below freeze violations that existed when the
// guard was introduced. Do not add new entries — remove them as modules are
// migrated to the contract.

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');
const repoRoot = resolve(appRoot, '../../..');
const modulesRoot = join(appRoot, 'modules');
const dbIsolationInventoryGuard = join(scriptDir, 'assert-db-isolation-inventory.mjs');
const v4IsolationGuard = join(appRoot, 'v4/tools/isolation/check-isolation.mjs');
const v4ManifestsRoot = join(appRoot, 'v4/manifests');

// Registered by the shell before any module mounts (app.js
// CRITICAL_SYNC_COLLECTIONS); modules may list them without re-declaring.
const SHELL_REGISTERED_COLLECTIONS = new Set([
  'business_module_catalog',
  'ctox_runtime_settings',
  'business_commands',
  'ctox_queue_tasks',
]);

const SHELL_TOKEN_NAMES = [
  'bg', 'surface', 'surface-2', 'line', 'text', 'text-strong', 'muted',
  'accent', 'accent-soft', 'danger', 'hairline', 'panel-radius',
  'control-radius', 'panel-shadow', 'glass-bg', 'glass-blur',
  'font-sans', 'font-mono',
  // shared/base.css derived semantic tokens
  'line-strong', 'success', 'success-soft', 'warning', 'warning-soft',
  'danger-soft', 'focus-ring',
];
const shellTokenPattern = new RegExp(
  `--(?:${SHELL_TOKEN_NAMES.join('|')})(?![\\w-])\\s*:`,
);

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Frozen pre-existing violations. Remove entries as modules are fixed.
// Currently EMPTY — keep it that way: fix the module instead of adding here.
const ALLOWLIST = new Map();

const offenders = [];
// collection name -> [{ module, fingerprint }] across all modules' schema.js
const collectionDeclarations = new Map();
const moduleDirs = readdirSync(modulesRoot).filter((name) => {
  const dir = join(modulesRoot, name);
  return statSync(dir).isDirectory() && existsSync(join(dir, 'module.json'));
});

if (moduleDirs.length === 0) {
  console.error('module conformance guard found no modules — wrong root?');
  process.exit(1);
}

for (const id of moduleDirs) {
  const dir = join(modulesRoot, id);
  const allow = new Set(ALLOWLIST.get(id) || []);
  const offend = (rule, detail) => {
    if (allow.has(rule)) return;
    offenders.push(`${relative(repoRoot, dir)}: ${rule}${detail ? ` — ${detail}` : ''}`);
  };

  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(dir, 'module.json'), 'utf8'));
  } catch (error) {
    offend('manifest-parse', String(error?.message || error));
    continue;
  }

  // schema-covers-collections — v4: collections.schema.json is canonical (§16.13).
  let schemaCollections = {};
  const jsonSchemaPath = join(dir, 'collections.schema.json');
  if (!existsSync(jsonSchemaPath)) {
    offend('schema-import', 'missing collections.schema.json (v4 canonical schema source, §16.13)');
  } else {
    try {
      const parsed = JSON.parse(readFileSync(jsonSchemaPath, 'utf8'));
      if (!parsed || typeof parsed.collections !== 'object' || Array.isArray(parsed.collections)) {
        offend('schema-import', 'collections.schema.json has no {collections: {…}} object');
      } else {
        schemaCollections = { ...parsed.collections };
      }
    } catch (error) {
      offend('schema-import', `collections.schema.json does not parse: ${String(error?.message || error).slice(0, 160)}`);
    }
  }
  const declared = Array.isArray(manifest.collections) ? manifest.collections : [];
  for (const collection of declared) {
    if (SHELL_REGISTERED_COLLECTIONS.has(collection)) continue;
    if (!Object.hasOwn(schemaCollections, collection)) {
      offend('schema-covers-collections', `${collection} listed in module.json but not declared in collections.schema.json`);
    }
  }
  for (const [collection, definition] of Object.entries(schemaCollections)) {
    if (SHELL_REGISTERED_COLLECTIONS.has(collection)) continue;
    if (!collectionDeclarations.has(collection)) collectionDeclarations.set(collection, []);
    collectionDeclarations.get(collection).push({
      module: id,
      fingerprint: stableStringify(definition?.schema || definition),
    });
  }

  // mount-export / mount-signature
  const indexJsPath = join(dir, 'index.js');
  const indexJs = existsSync(indexJsPath) ? readFileSync(indexJsPath, 'utf8') : '';
  if (!indexJs) {
    offend('mount-export', 'missing index.js');
  } else {
    const mountMatch = indexJs.match(/export\s+(?:async\s+)?function\s+mount\s*\(([^)]*)\)/)
      || indexJs.match(/export\s+const\s+mount\s*=\s*(?:async\s*)?\(([^)]*)\)/);
    if (!mountMatch) {
      offend('mount-export', 'index.js does not export mount()');
    } else {
      if (!isV4MountSignature(mountMatch[1])) {
        offend('mount-signature', `mount(${mountMatch[1].trim()}) — v4 contract is mount(root, api, { restored } = {})`);
      }
    }
    if (/\b(?:ctx\.)?db\.raw\b/.test(indexJs)) {
      offend('no-db-raw', 'unwraps the shell DB facade');
    }
    if (/\bindexedDB\s*[.[]/.test(indexJs)) {
      offend('no-indexeddb', 'modules must not touch IndexedDB directly');
    }
  }

  // CSS rules
  const cssPath = join(dir, 'index.css');
  const cssText = existsSync(cssPath) ? readFileSync(cssPath, 'utf8') : '';
  if (cssText) {
    const cssNoComments = cssText.replace(/\/\*[\s\S]*?\*\//g, '');
    for (const match of cssNoComments.matchAll(/(?:^|[};])\s*([^{};]+)\{([^{}]*)\}/g)) {
      const selector = match[1].trim();
      const body = match[2];
      const isPureRoot = /^:root(?:\[[^\]]*\])?(?:\s*,\s*(?:html|body|:root)(?:\[[^\]]*\])?)*$/.test(selector);
      if (isPureRoot && /--[\w-]+\s*:/.test(body)) {
        offend('css-no-root-tokens', `"${selector}" defines custom properties globally`);
      }
      if (shellTokenPattern.test(body)) {
        const token = body.match(shellTokenPattern)?.[0]?.replace(/\s*:$/, '');
        offend('css-no-shell-token-redefinition', `"${selector}" redefines ${token}`);
      }
      for (const declaration of body.matchAll(/(--[\w-]+)\s*:\s*([^;{}]+);?/g)) {
        const token = declaration[1];
        const value = declaration[2];
        const selfReferencePattern = new RegExp(String.raw`\bvar\(\s*${escapeRegExp(token)}(?:\s*[,)]|\s*$)`);
        if (selfReferencePattern.test(value)) {
          offend('css-no-self-referential-token', `"${selector}" defines ${token} using itself`);
        }
      }
    }
    if (/@import\s+url\(\s*['"]?https?:/.test(cssNoComments)) {
      offend('css-no-cdn-import', 'remote @import in module CSS');
    }
  }

  // locales
  const localesDir = join(dir, 'locales');
  if (!existsSync(join(localesDir, 'de.json')) || !existsSync(join(localesDir, 'en.json'))) {
    offend('locales', 'missing locales/de.json + locales/en.json');
  }
}

// schema-parity: collections declared by more than one module must carry the
// exact same definition — at runtime, whichever module registers first wins,
// so any divergence is a silent version/shape conflict on some clients.
for (const [collection, declarations] of collectionDeclarations) {
  if (declarations.length < 2) continue;
  const reference = declarations[0];
  for (const other of declarations.slice(1)) {
    if (other.fingerprint !== reference.fingerprint) {
      offenders.push(
        `src/apps/business-os/modules: schema-parity — ${collection} declared with diverging schemas by `
        + `${reference.module} and ${other.module}; share one definition via import/re-export`,
      );
    }
  }
}

runDbIsolationInventoryGuard();

if (offenders.length) {
  console.error(`Business OS module conformance failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log(`Business OS module conformance OK (${moduleDirs.length} modules)`);

// Key-order-independent serialization so independently-typed but identical
// schemas compare equal, while any real divergence (version, fields) differs.
function stableStringify(value) {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`).join(',')}}`;
  }
  if (typeof value === 'function') return JSON.stringify(String(value));
  return JSON.stringify(value) ?? 'undefined';
}

function runDbIsolationInventoryGuard() {
  const result = spawnSync(process.execPath, [dbIsolationInventoryGuard], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    const output = `${result.stderr || ''}${result.stdout || ''}`.trim();
    const findings = output.split('\n').map((line) => line.trim()).filter((line) => line.startsWith('- '));
    const remaining = findings.filter((line) => !isPortedDesktopAppStaleEntry(line));
    if (!findings.length || remaining.length) {
      offenders.push(`db-isolation-inventory — ${remaining.length ? remaining.join(' | ') : output || 'guard failed without output'}`);
    }
  }
  const isolation = spawnSync(process.execPath, [v4IsolationGuard, '--rule', 'R2', '--summary'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  if (isolation.status !== 0) {
    const output = `${isolation.stderr || ''}${isolation.stdout || ''}`.trim();
    offenders.push(`v4-isolation-R2 — ${output || 'check-isolation failed without output'}`);
  }
}

// "desktop_apps: stale inventory entry for <id>" is accepted only for an app that moved to modules/<id> (v4 port).
function isPortedDesktopAppStaleEntry(line) {
  const id = line.match(/^- desktop_apps: stale inventory entry for ([a-z0-9-]+)$/)?.[1];
  if (!id) return false;
  return !existsSync(join(appRoot, 'desktop-apps', id, 'app.js'))
    && existsSync(join(modulesRoot, id, 'module.json'))
    && existsSync(join(v4ManifestsRoot, `${id}.manifest.json`));
}

// v4 App-API 4.0 mount: (<element>, api) plus at most one destructured options object with a default.
function isV4MountSignature(rawParams) {
  const params = String(rawParams || '').replace(/\s+/g, ' ').trim();
  return /^[A-Za-z_$][\w$]*\s*,\s*api(?:\s*,\s*\{[^{}]*\}\s*(?:=\s*\{\s*\})?)?$/.test(params);
}
