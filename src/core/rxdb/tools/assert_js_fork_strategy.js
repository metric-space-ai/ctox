#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const appLocalRoot = path.join(repoRoot, 'src/apps/business-os/rxdb');
const offenders = [];

assertText(path.join(appLocalRoot, 'README.md'), [
  ['CTOX DB public name', /CTOX DB/],
  ['not upstream RxDB', /not upstream RxDB|not upstream npm `rxdb`/],
  ['not drop-in replacement', /not a drop-in replacement/],
  ['API contract', /ctox-db-business-os-v1/],
  ['no package manager', /no install step|package-manager-free|package_manager/],
  ['native IndexedDB', /native `indexedDB` storage|native IndexedDB/i],
  ['native WebRTC', /native `RTCPeerConnection`|native WebRTC/i],
]);
assertManifest(path.join(appLocalRoot, 'manifest.json'));
assertText(path.join(appLocalRoot, 'src/advanced-status-bridge.mjs'), [
  ['public runtime status', /publicName:\s*'CTOX DB'/],
  ['API contract status', /apiContract:\s*'ctox-db-business-os-v1'/],
  ['upstream marker status', /upstreamCompatibility:\s*'not-upstream-rxdb'/],
  ['not upstream compatible status', /upstreamCompatible:\s*false/],
]);
assertText(path.join(appLocalRoot, 'src/replication-webrtc.mjs'), [
  ['WebRTC replication export', /export async function replicateWebRTC/],
  ['native connection handler shim', /export function getConnectionHandlerSimplePeer/],
  ['query demand loading', /createQueryDemandLoader/],
  ['file demand loading', /createFileDemandLoader/],
  ['schema hash payload', /schemaHashValue/],
]);
assertText(path.join(appLocalRoot, 'src/webrtc-native.mjs'), [
  ['native RTCPeerConnection', /new RTCPeerConnection/],
  ['native DataChannel', /createDataChannel/],
  ['native WebSocket signaling', /new WebSocket/],
  ['frame protocol', /ctox-rxdb-frame-v1|FRAME_PROTOCOL/],
  ['request response transport', /request\(remotePeerId,\s*method,\s*params/],
]);
assertText(path.join(appLocalRoot, 'src/storage-indexeddb.mjs'), [
  ['native IndexedDB open', /indexedDB\.open/],
  ['schema indexes', /normalizeSchemaIndexes/],
  ['checkpoint status', /replicationCheckpointStatus/],
]);
assertText(path.join(appLocalRoot, 'dist/ctox-rxdb-js.mjs'), [
  ['stable createRxDatabase export', /\bcreateRxDatabase\b/],
  ['stable replicateWebRTC export', /\breplicateWebRTC\b/],
  ['stable Advanced Status export', /\bbuildBusinessOsAdvancedStatus\b/],
  ['CTOX DB branding', /publicName:\s*"CTOX DB"/],
  ['API contract branding', /apiContract:\s*"ctox-db-business-os-v1"/],
]);

for (const legacy of [
  path.join(rxdbRoot, 'js-fork'),
  path.join(repoRoot, 'src/apps/business-os/vendor/rxdb-bundle.mjs'),
  path.join(repoRoot, 'src/apps/business-os/vendor/rxdb-bundle.provenance.json'),
]) {
  if (fs.existsSync(legacy)) {
    offenders.push(`${relative(legacy)}: remove legacy TS/npm RxDB fork artifact`);
  }
}

if (offenders.length) {
  console.error(`ctox-db strategy guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-db strategy guard OK');

function assertManifest(file) {
  const parsed = readJson(file);
  if (!parsed) return;
  const required = {
    name: 'ctox-rxdb-js',
    public_name: 'CTOX DB',
    package_manager: 'none',
    api_contract: 'ctox-db-business-os-v1',
    compatibility: 'ctox-db-api',
    upstream_compatible: false,
    upstream_compatibility: 'not-upstream-rxdb',
    protocol: 'ctox-rxdb-protocol-v1',
    entry: 'dist/ctox-rxdb-js.mjs',
  };
  for (const [key, value] of Object.entries(required)) {
    if (parsed[key] !== value) offenders.push(`${relative(file)}: ${key} must be ${JSON.stringify(value)}`);
  }
}

function assertText(file, rules) {
  const content = readText(file);
  if (!content) return;
  for (const [name, pattern] of rules) {
    if (!pattern.test(content)) offenders.push(`${relative(file)}: missing ${name}`);
  }
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${relative(file)}: invalid JSON: ${error.message}`);
    return null;
  }
}

function readText(file) {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch (error) {
    offenders.push(`${relative(file)}: ${error.message}`);
    return '';
  }
}

function relative(file) {
  return path.relative(repoRoot, file);
}
