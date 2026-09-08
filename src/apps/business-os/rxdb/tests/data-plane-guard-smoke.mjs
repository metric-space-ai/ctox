// GUARD: the ctox-rxdb data plane is WebRTC-only, package-manager-free, and
// env-toggle-free. This is the architecture's hardest rule (root README.md
// "Data Boundary"): Business OS collections, commands, files, manifests and
// runtime state replicate ONLY over RxDB/WebRTC. Several past agents tried to
// "help" by adding HTTP fallbacks, npm imports, or env toggles — every one of
// those was a regression and had to be reverted.
//
// This is a RATCHET guard: the allowlists below describe the exact legitimate
// occurrences that exist today. Any NEW occurrence fails the suite. If you
// (the agent reading this) hit a failure here, the answer is to remove your
// forbidden pattern — not to extend the allowlist. Allowlist changes require
// an explicit architecture decision recorded in docs/ctox-rxdb.md.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(testDir, '../../../../..');
const jsSrcDir = resolve(testDir, '../src');
const rustPluginDir = resolve(repoRoot, 'src/core/rxdb/src/plugins/replication_webrtc');
const rustPeerFile = resolve(repoRoot, 'src/core/business_os/rxdb_peer.rs');

const offenders = [];

// ---------------------------------------------------------------------------
// Browser runtime (src/*.mjs): no HTTP data paths, no non-relative imports.
// ---------------------------------------------------------------------------
const JS_RULES = [
  // fetch()/XHR/beacon would be an HTTP data path: collections never travel
  // over HTTP. (String form keeps this file from flagging itself.)
  { name: 'js-fetch', pattern: new RegExp('\\bfetch\\s*\\(') },
  { name: 'js-xhr', pattern: new RegExp('XMLHttpRequest') },
  { name: 'js-beacon', pattern: new RegExp('sendBeacon') },
  // http(s) URLs inside the runtime are a smell for an HTTP bridge.
  { name: 'js-http-url', pattern: new RegExp('https?://'), allow: { 'src/index.mjs': 0 } },
  // Exactly ONE WebSocket exists: the signaling socket in webrtc-native.mjs.
  { name: 'js-websocket', pattern: new RegExp('new WebSocket\\('), allow: { 'src/webrtc-native.mjs': 1 } },
];

for (const file of readdirSync(jsSrcDir).filter((name) => name.endsWith('.mjs'))) {
  const path = join(jsSrcDir, file);
  const rel = `src/${file}`;
  const content = stripComments(readFileSync(path, 'utf8'));
  for (const rule of JS_RULES) {
    const count = countMatches(content, rule.pattern);
    const allowed = rule.allow?.[rel] ?? 0;
    if (count > allowed) {
      offenders.push(`${rel}: ${rule.name} (${count} found, ${allowed} allowed)`);
    }
  }
  // Package-manager-free: every import must be relative. A bare specifier
  // means an npm dependency (forbidden — manifest.json: package_manager none);
  // a node: builtin would break the browser bundle outright.
  for (const match of readFileSync(path, 'utf8').matchAll(/^\s*(?:import|export)[^'"\n]*from\s+['"]([^'"]+)['"]/gm)) {
    const specifier = match[1];
    if (!specifier.startsWith('./') && !specifier.startsWith('../')) {
      offenders.push(`${rel}: non-relative specifier '${specifier}' (npm/node builtins are forbidden in the browser runtime)`);
    }
  }
}

// ---------------------------------------------------------------------------
// Rust side: no HTTP/TCP transports, no NEW process-env toggles.
// ---------------------------------------------------------------------------
const RUST_RULES = [
  { name: 'rust-http-client', pattern: new RegExp('\\b(?:reqwest|tiny_http|hyper)\\b') },
  // TcpListener is allowed only inside signaling_client.rs tests (the chaos
  // tests run local WebSocket servers). Ratchet raised 2 -> 7 for the
  // SYNC-31 signaling-failover chaos tests (initial-connect failover +
  // supervisor rotation each bind local listeners); recorded in
  // docs/ctox-rxdb.md §8. Still test-only: the production client never
  // listens.
  { name: 'rust-tcp-listener', pattern: new RegExp('TcpListener'), allow: { 'signaling_client.rs': 7 } },
  // Runtime config flows through the SQLite runtime store (AGENTS.md rule),
  // not process env. One legacy escape hatch exists (UDP bind addr).
  { name: 'rust-env-read', pattern: new RegExp('std::env::var'), allow: { 'connection_handler_rs.rs': 1 } },
];

for (const file of readdirSync(rustPluginDir).filter((name) => name.endsWith('.rs'))) {
  const content = stripComments(readFileSync(join(rustPluginDir, file), 'utf8'));
  for (const rule of RUST_RULES) {
    const count = countMatches(content, rule.pattern);
    const allowed = rule.allow?.[file] ?? 0;
    if (count > allowed) {
      offenders.push(`replication_webrtc/${file}: ${rule.name} (${count} found, ${allowed} allowed)`);
    }
  }
}

// rxdb_peer.rs: HTTP transports forbidden outright; env reads ratcheted at
// the current legacy count (2 toggles + 2 HOME-style path lookups).
{
  const content = stripComments(readFileSync(rustPeerFile, 'utf8'));
  const httpCount = countMatches(content, new RegExp('\\b(?:reqwest|tiny_http|hyper)\\b'));
  if (httpCount > 0) offenders.push(`rxdb_peer.rs: rust-http-client (${httpCount} found, 0 allowed)`);
  const envCount = countMatches(content, new RegExp('std::env::var'));
  if (envCount > 4) {
    offenders.push(`rxdb_peer.rs: rust-env-read ratchet exceeded (${envCount} found, 4 allowed) — new runtime toggles belong in the SQLite runtime store, not process env`);
  }
}

// ---------------------------------------------------------------------------
// One canonical bundle importer owns module identity. DB and sync consumers
// must use it, including timeout/recovery paths; URL parity is no longer a
// manually maintained promise across several independent imports.
{
  const sharedDir = resolve(repoRoot, 'src/apps/business-os/shared');
  const runtimePath = resolve(sharedDir, 'rxdb-runtime.js');
  const runtime = readFileSync(runtimePath, 'utf8');
  const urls = runtime.match(/ctox-rxdb-js\.mjs\?v=([^'"`]+)/g) || [];
  if (urls.length !== 1 || /[&?]retry=/.test(runtime)) {
    offenders.push('rxdb-runtime.js must define exactly one versioned bundle URL without retry variants');
  }
  for (const name of readdirSync(sharedDir).filter((name) => name.endsWith('.js') && name !== 'rxdb-runtime.js')) {
    const source = readFileSync(resolve(sharedDir, name), 'utf8');
    if (/ctox-rxdb-js\.mjs\?/.test(source)) {
      offenders.push(`${name}: direct versioned RxDB bundle reference bypasses the canonical loader`);
    }
  }
  for (const name of ['db.js', 'sync.js']) {
    const source = readFileSync(resolve(sharedDir, name), 'utf8');
    if (!/import\s*\{\s*loadRxdbRuntime\s*\}\s*from\s*['"]\.\/rxdb-runtime\.js['"]/.test(source)) {
      offenders.push(`${name}: must import the canonical RxDB runtime loader`);
    }
  }
}

if (offenders.length) {
  console.error('ctox-rxdb data-plane guard FAILED:');
  for (const line of offenders) console.error(`  - ${line}`);
  console.error('The data plane is WebRTC-only. Do not add HTTP paths, npm imports,');
  console.error('or env toggles. See docs/ctox-rxdb.md and the file headers.');
  process.exit(1);
}

console.log('ctox-rxdb data-plane guard OK', {
  jsFiles: readdirSync(jsSrcDir).filter((name) => name.endsWith('.mjs')).length,
  rustFiles: readdirSync(rustPluginDir).filter((name) => name.endsWith('.rs')).length + 1,
});

function countMatches(content, pattern) {
  return (content.match(new RegExp(pattern.source, 'g')) || []).length;
}

// Strip // line comments and /* */ blocks so documentation may mention the
// forbidden patterns without tripping the guard. Naive but sufficient here:
// string literals in these sources do not contain comment markers.
function stripComments(content) {
  return content
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/[^\n]*/g, '$1');
}
