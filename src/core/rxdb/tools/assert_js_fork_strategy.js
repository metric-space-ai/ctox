#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const readmePath = path.join(root, 'js-fork/README.md');
const manifestPath = path.join(root, 'js-fork/ctox-rxdb-js.manifest.json');
const dependencyAuditBaselinePath = path.join(root, 'js-fork/dependency-audit-baseline.json');
const bundleContractPath = path.join(root, 'js-fork/bundle-contract.json');
const sourcePackagePath = path.join(root, 'js-fork/source/package.json');
const sourceBundleEntryPath = path.join(root, 'js-fork/source/src/ctox-business-os-browser.ts');
const sourceReplicationPath = path.join(root, 'js-fork/source/src/plugins/replication-webrtc/index.ts');
const sourceSimplePeerPath = path.join(root, 'js-fork/source/src/plugins/replication-webrtc/connection-handler-simple-peer.ts');
const sourceReplicationTypesPath = path.join(root, 'js-fork/source/src/plugins/replication-webrtc/webrtc-types.ts');
const sourcePremiumPath = path.join(root, 'js-fork/source/src/plugins/utils/utils-premium.ts');
const sourceDexieHelperPath = path.join(root, 'js-fork/source/src/plugins/storage-dexie/dexie-helper.ts');
const portingPath = path.join(root, 'PORTING.md');
const offenders = [];

assertText(readmePath, [
  ['hard fork', /hard[- ]fork/i],
  ['business os source of truth', /Business OS is the source of truth/i],
  ['protocol v1', /ctox-rxdb-protocol-v1/],
  ['schema hash', /schema hash/i],
  ['peer generation', /peer generation/i],
  ['typed errors', /typed.*errors/i],
  ['file chunks', /chunk.*generation|generation.*chunk/i],
  ['private package publish policy', /private package/i],
  ['ctox release identity', /bundle SHA-256[\s\S]*lockfile SHA-256[\s\S]*Git tag/],
  ['no bundle hand edit', /Do not hand-edit `rxdb-bundle\.mjs`/],
]);

assertJsonManifest(manifestPath);
assertDependencyAuditBaseline(dependencyAuditBaselinePath);
assertBundleContract(bundleContractPath);
assertSourcePackage(sourcePackagePath);
assertText(sourceBundleEntryPath, [
  ['business bundle create db export', /createRxDatabase/],
  ['business bundle dexie export', /getRxStorageDexie/],
  ['business bundle webrtc export', /replicateWebRTC/],
  ['business bundle simple peer export', /getConnectionHandlerSimplePeer/],
]);
assertText(sourceReplicationPath, [
  ['ctox protocol constant', /CTOX_RXDB_PROTOCOL\s*=\s*['"]ctox-rxdb-protocol-v1['"]/],
  ['ctox protocol request', /method:\s*['"]ctoxProtocol['"]/],
  ['ctox protocol payload', /function\s+ctoxProtocolPayload\s*\(/],
  ['ctox schema hash capability', /ctox-schema-hash-v1/],
  ['ctox protocol schema hash payload', /schemaHash:\s*await\s+collection\.schema\.hash/],
  ['ctox protocol schema hash validation', /remoteCollection\.schemaHash\s*!==\s*expectedSchemaHash/],
  ['ctox peer session capability', /ctox-peer-session-v1/],
  ['ctox peer session payload', /peerSession:\s*\{/],
  ['ctox checkpoint epoch capability', /ctox-checkpoint-epoch-v1/],
  ['ctox checkpoint epoch payload', /ctoxCheckpointPayload/],
  ['ctox peer session replication identifier', /replicationIdentifier:\s*\[collection\.name,\s*options\.topic,\s*peerToken,\s*remotePeerSessionId\]/],
  ['ctox peer session observer hook', /options\.ctox\?\.onPeerProtocol\?\.\(\{/],
  ['ctox idempotent peer removal', /const\s+peerState\s*=\s*peerStates\.get\(peer\)[\s\S]*if\s*\(!peerState\)\s*\{\s*return;\s*\}/],
  ['ctox webrtc pool await initial replication', /async\s+awaitInitialReplication\(\)[\s\S]*awaitPeerReplicationStates/],
  ['ctox webrtc pool await in sync', /async\s+awaitInSync\(\)[\s\S]*awaitPeerReplicationStates/],
  ['ctox protocol validation', /function\s+ensureCtoxProtocolCompatible\s*\(/],
  ['ctox protocol error', /RC_WEBRTC_PROTOCOL/],
]);
assertTextDoesNotContain(sourceReplicationPath, [
  ['node signaling server export', /signaling-server/],
]);
assertText(sourceReplicationTypesPath, [
  ['ctox protocol message type', /\|\s*['"]ctoxProtocol['"]/],
  ['ctox peer protocol observer type', /onPeerProtocol\?:/],
]);
assertText(sourceSimplePeerPath, [
  ['ctox safe signaling websocket send', /readyState\s*!==\s*1[\s\S]*return\s+false/],
  ['ctox no reconnect after handler close', /if\s*\(!closed\)\s*\{\s*createPeerConnection\(remotePeerId\);/],
]);
assertText(sourcePremiumPath, [
  ['hard fork premium gate removed', /CTOX maintains this as an application-specific hard fork/],
  ['premium flag always true', /return\s+PROMISE_RESOLVE_TRUE/],
]);
assertText(sourceDexieHelperPath, [
  ['dexie state map stores promise value', /DEXIE_STATE_DB_BY_NAME\.set\(dexieDbName,\s*value\)/],
  ['dexie refcount stores promise value', /REF_COUNT_PER_DEXIE_DB\.set\(value,\s*0\)/],
]);

assertText(portingPath, [
  ['wave 260', /\|\s*260\s*\|\s*done\s*\|\s*RxDB JS hard-fork control surface/],
  ['wave 262', /\|\s*262\s*\|\s*done\s*\|\s*RxDB JS bundle provenance contract/],
  ['wave 265', /\|\s*265\s*\|\s*done\s*\|\s*RxDB JS\/Rust DataChannel protocol handshake/],
  ['wave 266', /\|\s*266\s*\|\s*done\s*\|\s*RxDB JS hard-fork source bundle/],
  ['hard fork section', /ctox-rxdb-js Hard Fork/],
]);

if (offenders.length) {
  console.error(`ctox-rxdb-js hard-fork strategy guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-rxdb-js hard-fork strategy guard OK');

function assertText(file, rules) {
  if (!fs.existsSync(file)) {
    offenders.push(`${path.relative(root, file)}: missing`);
    return;
  }
  const content = fs.readFileSync(file, 'utf8');
  for (const [name, pattern] of rules) {
    if (!pattern.test(content)) offenders.push(`${path.relative(root, file)}: missing ${name}`);
  }
}

function assertTextDoesNotContain(file, rules) {
  if (!fs.existsSync(file)) {
    offenders.push(`${path.relative(root, file)}: missing`);
    return;
  }
  const content = fs.readFileSync(file, 'utf8');
  for (const [name, pattern] of rules) {
    if (pattern.test(content)) offenders.push(`${path.relative(root, file)}: must not contain ${name}`);
  }
}

function assertJsonManifest(file) {
  if (!fs.existsSync(file)) {
    offenders.push(`${path.relative(root, file)}: missing`);
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${path.relative(root, file)}: invalid JSON: ${error.message}`);
    return;
  }
  const required = {
    name: 'ctox-rxdb-js',
    fork_type: 'hard-fork',
  };
  for (const [key, value] of Object.entries(required)) {
    if (parsed[key] !== value) offenders.push(`${path.relative(root, file)}: ${key} must be ${value}`);
  }
  if (parsed.upstream?.tag !== '16.20.0') offenders.push(`${path.relative(root, file)}: upstream tag must stay pinned to 16.20.0`);
  if (parsed.source_path !== 'source') offenders.push(`${path.relative(root, file)}: source_path must point to source`);
  if (parsed.publish_policy?.npm !== 'private-package-only') {
    offenders.push(`${path.relative(root, file)}: publish_policy.npm must be private-package-only`);
  }
  if (!/provenance/i.test(parsed.publish_policy?.release_identity || '')) {
    offenders.push(`${path.relative(root, file)}: publish_policy.release_identity must mention provenance`);
  }
  if (parsed.rust_peer?.protocol !== 'ctox-rxdb-protocol-v1') offenders.push(`${path.relative(root, file)}: missing ctox-rxdb-protocol-v1`);
  for (const contract of [
    'protocol_version_handshake',
    'schema_hash_exchange',
    'peer_generation_reconnect',
    'typed_replication_errors',
    'file_chunk_generation_integrity',
    'checkpoint_initial_sync_readiness',
  ]) {
    if (!parsed.required_contracts?.includes(contract)) {
      offenders.push(`${path.relative(root, file)}: missing required contract ${contract}`);
    }
  }
}

function assertSourcePackage(file) {
  if (!fs.existsSync(file)) {
    offenders.push(`${path.relative(root, file)}: missing hard-fork source package`);
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${path.relative(root, file)}: invalid JSON: ${error.message}`);
    return;
  }
  if (parsed.name !== 'ctox-rxdb-js') offenders.push(`${path.relative(root, file)}: package name must be ctox-rxdb-js`);
  if (parsed.version !== '16.20.0') offenders.push(`${path.relative(root, file)}: package version must preserve upstream baseline 16.20.0`);
  assertSourcePackageSurface(file, parsed);
  if (parsed.private !== true) offenders.push(`${path.relative(root, file)}: hard-fork source package must be private`);
  if (parsed.publishConfig?.access !== 'restricted') offenders.push(`${path.relative(root, file)}: publishConfig.access must be restricted`);
  if (parsed.repository?.url !== 'https://github.com/metric-space-ai/ctox.git') {
    offenders.push(`${path.relative(root, file)}: repository must point at the CTOX fork repository`);
  }
  if (parsed.repository?.directory !== 'src/core/rxdb/js-fork/source') {
    offenders.push(`${path.relative(root, file)}: repository.directory must point at the hard-fork source path`);
  }
  if (parsed.homepage !== 'https://ctox.dev/') offenders.push(`${path.relative(root, file)}: homepage must point at ctox.dev`);
  if (parsed.ctoxHardFork?.upstream?.version !== '16.20.0') {
    offenders.push(`${path.relative(root, file)}: missing ctoxHardFork upstream provenance`);
  }
  if (parsed.ctoxHardFork?.publishPolicy?.npm !== 'private-package-only') {
    offenders.push(`${path.relative(root, file)}: ctoxHardFork.publishPolicy.npm must be private-package-only`);
  }
  if (!/Business OS first/i.test(parsed.ctoxHardFork?.productRule || '')) {
    offenders.push(`${path.relative(root, file)}: missing Business OS first product rule`);
  }
}

function assertSourcePackageSurface(file, parsed) {
  const expectedExports = [
    '.',
    './plugins/core',
    './plugins/utils',
    './plugins/storage-dexie',
    './plugins/replication-webrtc',
    './plugins/migration-schema',
    './plugins/validate-ajv',
    './plugins/validate-z-schema',
    './package.json',
  ];
  const actualExports = Object.keys(parsed.exports || {}).sort();
  if (actualExports.join('\n') !== expectedExports.slice().sort().join('\n')) {
    offenders.push(`${path.relative(root, file)}: exports must remain narrowed to the CTOX browser bundle surface`);
  }
  const expectedScripts = ['postinstall', 'ctox:bundle', 'ctox:audit', 'ctox:check'];
  const actualScripts = Object.keys(parsed.scripts || {}).sort();
  if (actualScripts.join('\n') !== expectedScripts.slice().sort().join('\n')) {
    offenders.push(`${path.relative(root, file)}: scripts must remain narrowed to CTOX bundle/audit commands`);
  }
  const devDeps = Object.keys(parsed.devDependencies || {});
  if (devDeps.length !== 1 || devDeps[0] !== 'esbuild') {
    offenders.push(`${path.relative(root, file)}: devDependencies must remain limited to esbuild`);
  }
  for (const removed of [
    'firebase',
    'mongodb',
    'nats',
    'graphql',
    'graphql-ws',
    'isomorphic-ws',
    'reconnecting-websocket',
    'crypto-js',
    'jsonschema-key-compression',
    'is-my-json-valid',
    'rxdb-old',
    'webpack',
    'rollup',
    'mocha',
    'karma',
  ]) {
    if (parsed.dependencies?.[removed] || parsed.devDependencies?.[removed]) {
      offenders.push(`${path.relative(root, file)}: removed upstream dependency ${removed} must not be reintroduced`);
    }
  }
}

function assertDependencyAuditBaseline(file) {
  if (!fs.existsSync(file)) {
    offenders.push(`${path.relative(root, file)}: missing dependency audit baseline`);
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${path.relative(root, file)}: invalid JSON: ${error.message}`);
    return;
  }
  if (parsed.policy?.allowed_status !== 'zero-vulnerability-baseline') {
    offenders.push(`${path.relative(root, file)}: policy.allowed_status must be zero-vulnerability-baseline`);
  }
  if (parsed.policy?.fail_on_lockfile_drift !== true) {
    offenders.push(`${path.relative(root, file)}: must fail on lockfile drift`);
  }
  if (parsed.source_lockfile !== 'js-fork/source/package-lock.json') {
    offenders.push(`${path.relative(root, file)}: source_lockfile must point at the fork lockfile`);
  }
  if (parsed.severity_budget?.total !== 0) {
    offenders.push(`${path.relative(root, file)}: severity budget must remain zero`);
  }
  if (parsed.known_direct_vulnerable_packages?.length !== 0) {
    offenders.push(`${path.relative(root, file)}: known direct vulnerable package inventory must remain empty`);
  }
}

function assertBundleContract(file) {
  if (!fs.existsSync(file)) {
    offenders.push(`${path.relative(root, file)}: missing bundle contract`);
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${path.relative(root, file)}: invalid JSON: ${error.message}`);
    return;
  }
  if (parsed.name !== 'ctox-rxdb-js-browser-bundle') offenders.push(`${path.relative(root, file)}: unexpected bundle contract name`);
  if (parsed.fork_package !== 'ctox-rxdb-js') offenders.push(`${path.relative(root, file)}: fork_package must be ctox-rxdb-js`);
  if (parsed.protocol !== 'ctox-rxdb-protocol-v1') offenders.push(`${path.relative(root, file)}: missing protocol contract`);
  if (parsed.publish_policy !== 'private-package-only') offenders.push(`${path.relative(root, file)}: publish_policy must be private-package-only`);
  if (parsed.version_discipline !== 'upstream-version-pinned-with-ctox-provenance') {
    offenders.push(`${path.relative(root, file)}: version_discipline must be upstream-version-pinned-with-ctox-provenance`);
  }
  if (parsed.build_entry !== 'js-fork/source/src/ctox-business-os-browser.ts') {
    offenders.push(`${path.relative(root, file)}: build_entry must point to ctox-business-os-browser.ts`);
  }
  for (const expected of ['createRxDatabase', 'getRxStorageDexie', 'replicateWebRTC', 'getConnectionHandlerSimplePeer']) {
    if (!parsed.expected_exports?.includes(expected)) {
      offenders.push(`${path.relative(root, file)}: missing expected export ${expected}`);
    }
  }
}
