#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const rxdbRoot = path.resolve(__dirname, '..');
const root = path.resolve(rxdbRoot, '..', '..', 'apps', 'business-os', 'rxdb');
const offenders = [];

const forbiddenBasenames = new Set([
  'package.json',
  'package-lock.json',
  'npm-shrinkwrap.json',
  'pnpm-lock.yaml',
  'yarn.lock',
]);
const forbiddenCodeTokens = [
  'simple-peer',
  'rxjs',
  'dexie',
  'ajv',
  'z-schema',
  'mingo',
  'broadcast-channel',
  'node_modules',
  'pluginMissing',
  'NON_PREMIUM',
  'rxdb-premium',
  'premium access',
  'trial version',
];

if (!fs.existsSync(root)) {
  offenders.push('src/apps/business-os/rxdb: missing');
} else {
  for (const file of walk(root)) {
    const basename = path.basename(file);
    const rel = relative(file);
    if (forbiddenBasenames.has(basename)) {
      offenders.push(`${rel}: package-manager files are not allowed`);
    }
    if (file.split(path.sep).includes('node_modules')) {
      offenders.push(`${rel}: dependency trees are not allowed`);
    }
    if (/\.(mjs|js|json)$/.test(file)) {
      const content = fs.readFileSync(file, 'utf8');
      assertNoBareImports(rel, content);
      for (const token of forbiddenCodeTokens) {
        if (content.includes(token)) {
          offenders.push(`${rel}: forbidden dependency token ${token}`);
        }
      }
      if (/\.(mjs|js)$/.test(file)) {
        const check = spawnSync(process.execPath, ['--check', file], { encoding: 'utf8' });
        if (check.status !== 0) {
          offenders.push(`${rel}: syntax check failed: ${(check.stderr || check.stdout).trim()}`);
        }
      }
    }
  }
  assertRequiredMarkers();
  assertDynamicSmokeTests();
}

if (offenders.length) {
  console.error(`ctox-rxdb-js guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-rxdb-js guard OK');

function assertNoBareImports(rel, content) {
  const staticImport = /(?:import|export)\s+(?:[^'"]+\s+from\s+)?['"]([^.'"/][^'"]*)['"]/g;
  const dynamicImport = /import\(\s*['"]([^.'"/][^'"]*)['"]\s*\)/g;
  for (const pattern of [staticImport, dynamicImport]) {
    let match;
    while ((match = pattern.exec(content))) {
      if (rel.includes('/tests/') && match[1].startsWith('node:')) {
        continue;
      }
      offenders.push(`${rel}: bare module import ${match[1]} is not allowed`);
    }
  }
}

function assertRequiredMarkers() {
  assertText('manifest.json', [
    ['package-manager-free marker', /"package_manager":\s*"none"/],
    ['native IndexedDB marker', /"indexeddb-native"/],
    ['native WebRTC marker', /"webrtc-native"/],
    ['no feature gates marker', /"feature_gates":\s*"none"/],
    ['no runtime addons marker', /"runtime_addons":\s*"none"/],
  ]);
  assertText('src/storage-indexeddb.mjs', [
    ['native indexedDB usage', /indexedDB\.open/],
    ['collection checkpoint index', /collectionLwtId/],
    ['schema index normalization', /normalizeSchemaIndexes/],
    ['stored document index values', /indexValues/],
    ['query plan index selection', /queryPlanFor\(query = \{\}\)/],
  ]);
  assertText('src/webrtc-native.mjs', [
    ['native RTCPeerConnection usage', /new RTCPeerConnection/],
    ['native data channel usage', /createDataChannel/],
    ['native WebSocket usage', /new WebSocket/],
    ['ctox protocol request envelope', /request\(remotePeerId,\s*method,\s*params/],
    ['ctox protocol response envelope', /result,\s*error:\s*null/],
    ['upstream-compatible signaling join', /type:\s*'join'/],
    ['upstream-compatible signaling sender id', /senderPeerId/],
    ['upstream-compatible signaling receiver id', /receiverPeerId/],
    ['remote request observation', /waitForRequest\(peerId,\s*method/],
    ['dead peer connection removal', /removeConnection\(remotePeerId/],
    ['recoverable peer close code', /ERR_CONNECTION_FAILURE/],
  ]);
  assertText('src/replication-webrtc.mjs', [
    ['replicateWebRTC compatibility export', /export async function replicateWebRTC/],
    ['connection handler compatibility export', /export function getConnectionHandlerSimplePeer/],
    ['masterChangesSince request', /'masterChangesSince'/],
    ['masterWrite request', /'masterWrite'/],
    ['checkpoint status usage', /replicationCheckpointStatus/],
    ['initial replication awaiter', /awaitInitialReplication\(\)/],
    ['native master handshake barrier', /awaitRemoteMasterReady\(peerId\)/],
    ['per-peer push checkpoint tracking', /pushCheckpointsByPeer/],
    ['per-peer pull checkpoint tracking', /pullCheckpointsByPeer/],
    ['closed peer state cleanup', /removePeer\(peerId/],
    ['replaced native peer pruning', /pruneReplacedNativePeers/],
    ['native peer singleton retention', /retainOnlyNativePeer/],
    ['serialized peer-open handshakes', /peerOpenQueue/],
  ]);
  assertText('src/protocol-contract.generated.mjs', [
    ['generated contract marker', /Generated from src\/core\/rxdb\/tests\/fixtures\/webrtc-rxdb-protocol\.json/],
    ['protocol constant', /ctox-rxdb-protocol-v1/],
    ['known schema hash source policy', /business-os-schema-hash-registry-v1/],
    ['custom schema hash source policy', /canonical-json-schema-sha256-v1/],
  ]);
  assertText('src/frame-contract.generated.mjs', [
    ['generated frame contract marker', /Generated from src\/core\/rxdb\/tests\/fixtures\/webrtc-frame-protocol\.json/],
    ['frame protocol constant', /ctox-rxdb-frame-v1/],
    ['chunk size constant', /MAX_CHUNK_CHARS\s*=\s*10240/],
    ['ack window constant', /FRAME_ACK_WINDOW\s*=\s*4/],
  ]);
  assertText('src/schema.mjs', [
    ['webcrypto hash', /crypto\.subtle\.digest/],
    ['generated protocol contract import', /protocol-contract\.generated\.mjs/],
  ]);
  assertText('src/rx-database.mjs', [
    ['createRxDatabase compatibility export', /export async function createRxDatabase/],
    ['dexie compatibility shim without dependency', /getRxStorageDexie/],
    ['addRxPlugin transition shim', /export function addRxPlugin\(_ignored = null\)/],
    ['rxdbCore runtime WebRTC export', /replicateWebRTC/],
    ['rxdbCore runtime connection handler export', /getConnectionHandlerSimplePeer/],
    ['business os query exec surface', /async exec\(\)/],
    ['business os query chaining surface', /where\(field\)/],
    ['business os query skip surface', /skip\(skip\)/],
    ['business os bulk insert surface', /async bulkInsert\(docs = \[\]\)/],
    ['business os bulk upsert surface', /async bulkUpsert\(docs = \[\]\)/],
    ['business os schema indexes surface', /schemaIndexes\(\)/],
    ['business os query plan surface', /queryPlanFor\(query = \{\}\)/],
    ['nested primary key normalization', /setValueAtPath/],
    ['business os logical selector surface', /\$and/],
    ['business os element selector surface', /\$elemMatch/],
    ['document patch surface', /async incrementalPatch/],
    ['document update surface without add-on gate', /async update\(operation\)/],
    ['document atomic update surface without add-on gate', /async atomicUpdate\(modifier\)/],
    ['collection atomic upsert surface without add-on gate', /async atomicUpsert\(doc\)/],
  ]);
  assertText('dist/ctox-rxdb-js.mjs', [
    ['stable dist export', /export\s+\{[\s\S]*createRxDatabase[\s\S]*replicateWebRTC[\s\S]*schemaHash[\s\S]*\}/],
    ['protocol compatibility exports', /CTOX_PROTOCOL_ERROR_CODES[\s\S]*CTOX_REQUIRED_PROTOCOL_CAPABILITIES/],
    ['no npm package surface', /packageManager|ctox-rxdb-js|indexedDB|requiredCapabilities/],
  ]);
}

function assertDynamicSmokeTests() {
  const tests = [
    'tests/schema-hash-registry-smoke.mjs',
    'tests/no-package-manager-import-smoke.mjs',
  ];
  for (const test of tests) {
    const file = path.join(root, test);
    if (!fs.existsSync(file)) {
      offenders.push(`${test}: missing`);
      continue;
    }
    const result = spawnSync(process.execPath, [file], { encoding: 'utf8' });
    if (result.status !== 0) {
      const output = (result.stderr || result.stdout || '').trim();
      offenders.push(`${test}: smoke failed: ${output}`);
    }
  }
}

function assertText(relativePath, rules) {
  const file = path.join(root, relativePath);
  if (!fs.existsSync(file)) {
    offenders.push(`${relativePath}: missing`);
    return;
  }
  const content = fs.readFileSync(file, 'utf8');
  for (const [name, pattern] of rules) {
    if (!pattern.test(content)) {
      offenders.push(`${relativePath}: missing ${name}`);
    }
  }
}

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...walk(file));
    } else {
      out.push(file);
    }
  }
  return out;
}

function relative(file) {
  return path.relative(rxdbRoot, file);
}
