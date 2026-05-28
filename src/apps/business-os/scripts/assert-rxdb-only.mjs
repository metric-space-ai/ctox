import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');
const repoRoot = resolve(appRoot, '../../..');

const scannedRoots = [
  join(appRoot, 'app.js'),
  join(appRoot, 'index.html'),
  join(appRoot, 'desktop-apps'),
  join(appRoot, 'electron'),
  join(appRoot, 'shared'),
  join(appRoot, 'modules'),
  join(appRoot, 'template-store'),
  join(repoRoot, 'src/core/business_os/server.rs'),
  join(repoRoot, 'src/core/business_os/store.rs'),
];

const excludedSegments = new Set(['vendor', 'output', 'installed-modules']);
const forbidden = [
  { name: 'frontend-business-os-http-api', pattern: /\/api\/business-os(?:\/|[`'")])/, frontendOnly: true },
  { name: 'frontend-business-os-http-api-concat', pattern: /\/api\/['"`]\s*\+\s*['"`]business-os/, frontendOnly: true },
  { name: 'frontend-rxdb-http-pull', pattern: /\/rxdb\/pull/ },
  { name: 'frontend-command-http-post', pattern: /\/commands[`'")]/, frontendOnly: true },
  { name: 'frontend-status-http-poll', pattern: /\/api\/business-os\/status/, frontendOnly: true },
  { name: 'frontend-harness-http-fallback', pattern: /\/api\/business-os\/ctox\/(?:harness-flow|tasks)/, frontendOnly: true },
  { name: 'native-http-command-bridge', pattern: /recordNativeCommand|pullNativeCollection|native-http-pull/ },
  { name: 'sync-config-http-bridge-enabled', pattern: /http_bridge_available:\s*true/ },
  { name: 'native-http-bridge-reason', pattern: /native HTTP bridge/ },
  { name: 'frontend-local-only-sync-mode', pattern: /local-only/, frontendOnly: true },
  { name: 'frontend-fallback-database', pattern: /fallbackDb|FallbackDatabase|FallbackCollection/, frontendOnly: true },
];

const offenders = [];
assertBusinessOsShellBuildKeyIsCurrent();
assertAdvancedStatusInterfaceExists();
assertFileChunkIntegrityContract();
assertActiveNotesModuleDoesNotUseLegacyNotesnookBuild();
assertSyncWarmupDoesNotBlockBoot();
assertConnectionSmokeRequiresAdvancedStatusBootBudget();
assertLoginDoesNotDefaultToAdmin();
assertCtoxDbBrandingContract();
assertBusinessOsServerHttpDataApisAreGated();

for (const file of expandFiles(scannedRoots)) {
  const rel = relative(repoRoot, file);
  const content = readFileSync(file, 'utf8');
  const scannedContent = contentForForbiddenHttpScan(file, content);
  assertNoUpstreamRxdbImports(file, content);
  for (const rule of forbidden) {
    if (rule.frontendOnly && !isFrontendFile(file)) continue;
    if (rule.pattern.test(scannedContent)) offenders.push(`${rel}: ${rule.name}`);
  }
}

if (offenders.length) {
  console.error(`RxDB-only contract failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('RxDB-only contract OK');

function assertBusinessOsShellBuildKeyIsCurrent() {
  const appPath = join(appRoot, 'app.js');
  const indexPath = join(appRoot, 'index.html');
  const appContent = readFileSync(appPath, 'utf8');
  const indexContent = readFileSync(indexPath, 'utf8');
  const appBuild = appContent.match(/const\s+APP_BUILD\s*=\s*['"]([^'"]+)['"]/)?.[1] || '';
  const scriptBuild = indexContent.match(/<script[^>]+src=["']app\.js\?v=([^"']+)["']/)?.[1] || '';
  if (!appBuild) offenders.push('src/apps/business-os/app.js: missing APP_BUILD constant');
  if (!scriptBuild) offenders.push('src/apps/business-os/index.html: missing app.js build key');
  if (appBuild && scriptBuild && appBuild !== scriptBuild) {
    offenders.push(`src/apps/business-os/index.html: app.js build key ${scriptBuild} does not match APP_BUILD ${appBuild}`);
  }
}

function contentForForbiddenHttpScan(file, content) {
  const rel = relative(repoRoot, file).replaceAll('\\', '/');
  let scanned = content;
  const allow = (...patterns) => {
    for (const pattern of patterns) scanned = scanned.replace(pattern, '');
  };

  if (rel === 'src/apps/business-os/shared/react-settings.js') {
    allow(
      /['"]\/api\/business-os\/ctox\/subscription-auth\/start['"]/g,
      /['"]\/api\/business-os\/ctox\/subscription-auth\/callback['"]/g,
    );
  }

  if (rel === 'src/core/business_os/server.rs') {
    allow(
      /"\/api\/business-os\/ctox\/subscription-auth\/start"/g,
      /"\/api\/business-os\/ctox\/subscription-auth\/callback"/g,
    );
  }

  return scanned;
}

function assertBusinessOsServerHttpDataApisAreGated() {
  const serverPath = join(repoRoot, 'src/core/business_os/server.rs');
  const server = readFileSync(serverPath, 'utf8');
  if (!/path\.starts_with\("\/api\/business-os"\)\s*&&\s*!is_subscription_auth_path\(path\)/.test(server)) {
    offenders.push('src/core/business_os/server.rs: /api/business-os data APIs must be hard-gated behind the ChatGPT subscription-auth exception');
  }
  if (!/Business OS HTTP data APIs are disabled; use RxDB\/WebRTC\./.test(server)) {
    offenders.push('src/core/business_os/server.rs: HTTP data API gate must return the RxDB/WebRTC-only contract message');
  }
  const runtimeSettingsRoute = /"\/api\/business-os\/ctox\/runtime-settings"/;
  if (runtimeSettingsRoute.test(server)) {
    offenders.push('src/core/business_os/server.rs: runtime settings must not be exposed as an HTTP route');
  }
}

function assertLoginDoesNotDefaultToAdmin() {
  const appPath = join(appRoot, 'app.js');
  const appContent = readFileSync(appPath, 'utf8');
  if (/loginUser\s*\|\|\s*['"]admin['"]/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: login form must not default username to admin');
  }
  if (/placeholder=["']admin["']/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: login form must not show admin placeholder');
  }
  if (!/const\s+pairedConfig\s*=\s*await\s+readBusinessOsLaunchConfig\(\)/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: loadSession must await pairing config before authenticating');
  }
}

function assertCtoxDbBrandingContract() {
  const sharedDbPath = join(appRoot, 'shared/db.js');
  const contractPath = join(appRoot, 'RXDB_SYNC_CONTRACT.md');
  const appReadmePath = join(appRoot, 'README.md');
  const runtimeReadmePath = join(appRoot, 'rxdb/README.md');
  const runtimeManifestPath = join(appRoot, 'rxdb/manifest.json');
  const advancedStatusBridgePath = join(appRoot, 'rxdb/src/advanced-status-bridge.mjs');
  const rootReadmePath = join(repoRoot, 'README.md');
  const businessOsDocPath = join(repoRoot, 'docs/business-os.md');

  const sharedDb = readFileSync(sharedDbPath, 'utf8');
  for (const marker of [
    "publicName: 'CTOX DB'",
    "compatibility: 'ctox-db-api'",
    'upstreamCompatible: false',
    "upstreamCompatibility: 'not-upstream-rxdb'",
    "apiContract: 'ctox-db-business-os-v1'",
  ]) {
    if (!sharedDb.includes(marker)) {
      offenders.push(`src/apps/business-os/shared/db.js: CTOX DB runtime branding missing ${marker}`);
    }
  }

  const runtimeManifest = JSON.parse(readFileSync(runtimeManifestPath, 'utf8'));
  if (runtimeManifest.public_name !== 'CTOX DB') {
    offenders.push('src/apps/business-os/rxdb/manifest.json: public_name must be CTOX DB');
  }
  if (runtimeManifest.api_contract !== 'ctox-db-business-os-v1') {
    offenders.push('src/apps/business-os/rxdb/manifest.json: api_contract must be ctox-db-business-os-v1');
  }
  if (runtimeManifest.upstream_compatible !== false || runtimeManifest.upstream_compatibility !== 'not-upstream-rxdb') {
    offenders.push('src/apps/business-os/rxdb/manifest.json: must explicitly reject upstream RxDB compatibility');
  }

  const advancedStatusBridge = readFileSync(advancedStatusBridgePath, 'utf8');
  for (const marker of [
    "publicName: 'CTOX DB'",
    "apiContract: 'ctox-db-business-os-v1'",
    "upstreamCompatibility: 'not-upstream-rxdb'",
    'upstreamCompatible: false',
  ]) {
    if (!advancedStatusBridge.includes(marker)) {
      offenders.push(`src/apps/business-os/rxdb/src/advanced-status-bridge.mjs: CTOX DB status branding missing ${marker}`);
    }
  }

  for (const [path, required] of [
    [contractPath, ['CTOX DB', 'upstream RxDB', 'not a drop-in replacement', 'ctox-db-business-os-v1', "must not import `rxdb`"]],
    [appReadmePath, ['CTOX DB', 'not upstream npm `rxdb`', 'ctox-db-business-os-v1', "Do not import `rxdb`"]],
    [runtimeReadmePath, ['CTOX DB', 'not upstream RxDB', 'not a drop-in replacement', 'ctox-db-business-os-v1']],
    [rootReadmePath, ['CTOX DB', 'not upstream npm `rxdb`', 'not a drop-in replacement']],
    [businessOsDocPath, ['CTOX DB', 'not a drop-in replacement for upstream npm `rxdb`']],
  ]) {
    const content = readFileSync(path, 'utf8');
    for (const marker of required) {
      if (!content.includes(marker)) {
        offenders.push(`${relative(repoRoot, path)}: CTOX DB compatibility docs missing ${marker}`);
      }
    }
  }
}

function assertNoUpstreamRxdbImports(file, content) {
  if (!isFrontendFile(file)) return;
  const rel = relative(repoRoot, file);
  const importPatterns = [
    /\bimport\s+(?:[^'"]+\s+from\s+)?['"]rxdb(?:\/plugins\/[^'"]*)?['"]/,
    /\bimport\s*\(\s*['"]rxdb(?:\/plugins\/[^'"]*)?['"]\s*\)/,
    /\brequire\s*\(\s*['"]rxdb(?:\/plugins\/[^'"]*)?['"]\s*\)/,
  ];
  for (const pattern of importPatterns) {
    if (pattern.test(content)) {
      offenders.push(`${rel}: Business OS apps must use CTOX DB shell handles, not upstream rxdb imports`);
      return;
    }
  }
}

function assertAdvancedStatusInterfaceExists() {
  const appPath = join(appRoot, 'app.js');
  const syncPath = join(appRoot, 'shared/sync.js');
  const dbPath = join(appRoot, 'shared/db.js');
  const appContent = readFileSync(appPath, 'utf8');
  const syncContent = readFileSync(syncPath, 'utf8');
  const dbContent = readFileSync(dbPath, 'utf8');
  if (!/CTOX_BUSINESS_OS_STATUS/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: missing advanced Business OS status interface');
  }
  if (!/business-os-advanced-status-v1/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: missing advanced status version marker');
  }
  for (const requiredCheck of [
    'workspaceNotLoading',
    'dataPlaneWebrtc',
    'rxdbRuntimeAppLocal',
    'moduleCatalogAvailable',
    'requiredCollectionsConnected',
    'requiredCollectionsInitialSyncComplete',
    'requiredCollectionsCheckpointEpochAdvertised',
    'noStalledReconnect',
  ]) {
    if (!appContent.includes(requiredCheck)) {
      offenders.push(`src/apps/business-os/app.js: advanced status missing ${requiredCheck}`);
    }
  }
  if (!/rxdbRuntime/.test(appContent) || !/ctox-rxdb-js/.test(dbContent) || !/packageManager:\s*'none'/.test(dbContent)) {
    offenders.push('src/apps/business-os: advanced status missing app-local no-package-manager RxDB runtime evidence');
  }
  for (const criticalCollection of [
    'business_module_catalog',
    'ctox_runtime_settings',
    'business_commands',
    'ctox_queue_tasks',
    'desktop_files',
    'desktop_file_chunks',
  ]) {
    if (!appContent.includes(`'${criticalCollection}'`)) {
      offenders.push(`src/apps/business-os/app.js: advanced status default missing ${criticalCollection}`);
    }
  }
  if (!/function\s+isRequiredCollectionReady/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status missing required collection readiness helper');
  }
  if (!/business_commands[\s\S]{0,180}ctox_queue_tasks[\s\S]{0,220}desktop_files[\s\S]{0,140}desktop_file_chunks[\s\S]{0,220}\]\.includes\(collection\)[\s\S]{0,80}return true/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status does not handle empty command/file collections explicitly');
  }
  if (!/collectionErrors/.test(appContent) || !/serializeAdvancedStatusCollectionError/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status missing serialized collection error diagnostics');
  }
  if (!/fileIntegrity/.test(appContent) || !/reportFileIntegrityError/.test(appContent) || !/serializeAdvancedStatusFileIntegrityError/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status missing file integrity diagnostics');
  }
  if (!/initialSync/.test(appContent) || !/buildAdvancedStatusInitialSync/.test(appContent) || !/missingInitialReplication/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status missing initial-sync readiness diagnostics');
  }
  if (!/lifecycleEvents/.test(appContent) || !/serializeAdvancedStatusLifecycleEvent/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status missing peer lifecycle event diagnostics');
  }
  if (!/initialReplicationState/.test(syncContent) || !/initialReplicationStartedAt/.test(syncContent) || !/watchInitialReplication/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing initial replication diagnostics');
  }
  if (!/CtoxWebRtcPeerLifecycleEvent/.test(syncContent) || !/classifyPeerLifecycleEvent/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing typed WebRTC peer lifecycle diagnostics');
  }
  if (!/__ctoxSyncTestHooks[\s\S]{0,140}classifyPeerLifecycleEvent/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: peer lifecycle classifier is not exposed to executable guards');
  }
  if (!/ctox_data_channel_error/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: data-channel close is not classified as recoverable lifecycle');
  }
  if (!/lastLifecycleEvent:\s*null/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: successful reconnect paths do not clear stale lifecycle status');
  }
  if (!/peerSessions/.test(appContent) || !/remotePeerSession/.test(syncContent) || !/peerSessionSeenAt/.test(syncContent)) {
    offenders.push('src/apps/business-os: advanced status missing peer-session diagnostics');
  }
  if (!/ctox-checkpoint-epoch-v1/.test(syncContent) || !/remoteCheckpoint/.test(syncContent) || !/sanitizeAdvancedStatusRemoteCheckpoint/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing remote checkpoint epoch diagnostics');
  }
  if (!/hasAdvertisedCheckpointEpoch/.test(appContent) || !/missingCheckpointEpoch/.test(appContent) || !/checkpointEpochAdvertised/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status does not gate required collection readiness on checkpoint epoch evidence');
  }
  if (!/CtoxCheckpointProtocolError/.test(syncContent) || !/ctox_checkpoint_epoch_missing/.test(syncContent) || !/checkpointErrors/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing typed checkpoint protocol errors');
  }
  if (!/CtoxSchemaProtocolError/.test(syncContent) || !/ctox_schema_hash_mismatch/.test(syncContent) || !/ctox_rxdb_protocol_mismatch/.test(syncContent) || !/schemaErrors/.test(appContent) || !/noSchemaProtocolErrors/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing typed schema protocol errors');
  }
  if (!/classifySchemaProtocolError\?\.\(item\.collection/.test(appContent) || !/__ctoxSyncTestHooks[\s\S]{0,180}classifySchemaProtocolError/.test(syncContent)) {
    offenders.push('src/apps/business-os: advanced status does not normalize raw RxDB protocol incompatibility errors');
  }
  if (!/sanitizeAdvancedStatusNativePeerRecovery/.test(appContent) || !/ctox_optional_schema_drift/.test(repoBusinessOsContent()) || !/repair-optional-drift/.test(repoBusinessOsContent())) {
    offenders.push('src/apps/business-os: advanced status missing optional native schema drift recovery metadata');
  }
  if (!/health:\s*\{[\s\S]{0,120}errorTotal/.test(appContent) || !/sanitizeAdvancedStatusTypedError/.test(appContent) || !/serializeAdvancedStatusServiceErrors/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status missing unified typed native peer/service health errors');
  }
  if (!/ctox-native-rxdb-peer-status-v1/.test(repoBusinessOsContent()) || !/CtoxNativePeerCollectionDegraded/.test(repoBusinessOsContent()) || !/ctox_native_peer_not_running/.test(repoBusinessOsContent())) {
    offenders.push('src/core/business_os/rxdb_peer.rs: native peer status missing typed health errors');
  }
  if (!/CtoxReplicationIoError/.test(syncContent) || !/ctox_replication_pull_failed/.test(syncContent) || !/ctox_replication_push_failed/.test(syncContent) || !/replicationErrors/.test(appContent) || !/noReplicationIoErrors/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing typed replication I/O errors');
  }
  if (!/__ctoxSyncTestHooks/.test(syncContent) || !/explicitRowCount/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing executable replication error classifier hooks');
  }
  if (!/(classifyReplicationIoError,\s*createSyncRuntime|classifyReplicationIoError\?\.\()/.test(appContent) || !/normalizedError\s*=[\s\S]{0,220}classifyReplicationIoError\?\.\(item\.collection/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: advanced status does not normalize raw Rust replication envelopes');
  }
  if (!/peerGeneration/.test(syncContent) || !/generationChangedAt/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing peer-generation diagnostics');
  }
  if (!/ctoxError/.test(syncContent) || !/CtoxSignalingControlPlaneError/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing typed signaling control-plane error diagnostics');
  }
  if (!/registerSignalingErrorHandler/.test(syncContent) || !/parseSignalingControlPlaneError/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing signaling error observer');
  }
  if (!/ctox-rxdb-protocol-v1/.test(syncContent) || !/signalingUrlWithBrowserMetadata/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing browser protocol/capability signaling metadata');
  }
  if (!/protocol/.test(syncContent) || !/ctox-rxdb-browser-v1/.test(syncContent) || !/ctox-file-chunks-v1/.test(syncContent)) {
    offenders.push('src/apps/business-os/shared/sync.js: missing protocol capability diagnostics');
  }
  for (const marker of [
    'bootTimings',
    'shellVisibleMs',
    'firstWebRtcConnectedMs',
    'firstAdvancedStatusHealthyMs',
    'serializeBootTimings',
    'markBootTiming',
  ]) {
    if (!appContent.includes(marker)) {
      offenders.push(`src/apps/business-os/app.js: advanced status missing boot timing marker ${marker}`);
    }
  }
}

function repoBusinessOsContent() {
  return readFileSync(join(repoRoot, 'src/core/business_os/rxdb_peer.rs'), 'utf8');
}

function assertFileChunkIntegrityContract() {
  const desktopSchemaPath = join(appRoot, 'modules/desktop/schema.js');
  const sharedIntegrityPath = join(appRoot, 'shared/file-integrity.js');
  const universalImporterPath = join(appRoot, 'shared/universal-importer.js');
  const fileViewerPath = join(appRoot, 'desktop-apps/file-viewer/app.js');
  const explorerPath = join(appRoot, 'desktop-apps/explorer/app.js');
  const rustPeerPath = join(repoRoot, 'src/core/business_os/rxdb_peer.rs');
  const desktopSchema = readFileSync(desktopSchemaPath, 'utf8');
  const sharedIntegrity = readFileSync(sharedIntegrityPath, 'utf8');
  const universalImporter = readFileSync(universalImporterPath, 'utf8');
  const fileViewer = readFileSync(fileViewerPath, 'utf8');
  const explorer = readFileSync(explorerPath, 'utf8');
  const rustPeer = readFileSync(rustPeerPath, 'utf8');
  for (const field of [
    'content_generation_id',
    'content_hash_scheme',
    'generation_id',
    'content_hash',
    'chunk_hash',
    'chunk_hash_scheme',
  ]) {
    if (!desktopSchema.includes(field)) {
      offenders.push(`src/apps/business-os/modules/desktop/schema.js: desktop file chunk contract missing ${field}`);
    }
  }
  for (const marker of [
    'FILE_CONTENT_HASH_SCHEME',
    'FILE_CHUNK_HASH_SCHEME',
    'FILE_CHUNK_ERROR_CODES',
    'CtoxFileChunkIntegrityError',
    'ctox_file_chunk_missing',
    'ctox_file_chunk_generation_mismatch',
    'ctox_file_chunk_integrity_mismatch',
    'file-chunk-reconstruct',
    'readStoredFileFromChunks',
    'validateChunkMetadata',
    'size_bytes',
    'validateGenerationContract',
    'validateChunkHashes',
    'validateContentHash',
    'isDeletedChunk',
    'return chunks.filter((chunk) => chunk.generation_id === contentGenerationId)',
  ]) {
    if (!sharedIntegrity.includes(marker)) {
      offenders.push(`src/apps/business-os/shared/file-integrity.js: file chunk integrity missing ${marker}`);
    }
  }
  for (const marker of ['readStoredFileFromChunks', 'file-integrity.js?v=20260522-file-chunk-integrity5']) {
    if (!fileViewer.includes(marker)) {
      offenders.push(`src/apps/business-os/desktop-apps/file-viewer/app.js: file chunk integrity missing ${marker}`);
    }
  }
  for (const marker of ['FILE_CONTENT_HASH_SCHEME', 'FILE_CHUNK_HASH_SCHEME', 'chunk_hash', 'base64ToBytes', 'readStoredFileFromChunks']) {
    if (!explorer.includes(marker)) {
      offenders.push(`src/apps/business-os/desktop-apps/explorer/app.js: uploaded file chunk contract missing ${marker}`);
    }
  }
  for (const marker of ['readStoredFileFromChunks', 'file-integrity.js?v=20260522-file-chunk-integrity5', 'contentHashScheme']) {
    if (!universalImporter.includes(marker)) {
      offenders.push(`src/apps/business-os/shared/universal-importer.js: imported virtual file integrity missing ${marker}`);
    }
  }
  for (const marker of ['DESKTOP_FILE_CONTENT_HASH_SCHEME', 'DESKTOP_FILE_CHUNK_HASH_SCHEME', 'chunk_hash']) {
    if (!rustPeer.includes(marker)) {
      offenders.push(`src/core/business_os/rxdb_peer.rs: native desktop file chunk contract missing ${marker}`);
    }
  }
}

function assertActiveNotesModuleDoesNotUseLegacyNotesnookBuild() {
  const notesModulePath = join(appRoot, 'modules/notes/module.json');
  const notesIndexPath = join(appRoot, 'modules/notes/index.html');
  const notesScriptPath = join(appRoot, 'modules/notes/index.js');
  const moduleJson = JSON.parse(readFileSync(notesModulePath, 'utf8'));
  const indexContent = readFileSync(notesIndexPath, 'utf8');
  const scriptContent = readFileSync(notesScriptPath, 'utf8');
  if (moduleJson.entry !== 'modules/notes/index.html') {
    offenders.push(`src/apps/business-os/modules/notes/module.json: active Notes entry must stay on CTOX RxDB-backed index.html, got ${JSON.stringify(moduleJson.entry)}`);
  }
  for (const forbiddenPath of ['build/', 'notesnook-src/']) {
    if (indexContent.includes(forbiddenPath) || scriptContent.includes(forbiddenPath)) {
      offenders.push(`src/apps/business-os/modules/notes: active Notes module references inactive legacy Notesnook path ${forbiddenPath}`);
    }
  }
}

function assertSyncWarmupDoesNotBlockBoot() {
  const appPath = join(appRoot, 'app.js');
  const appContent = readFileSync(appPath, 'utf8');
  const bootstrapBody = appContent.match(/async function bootstrap\(\) \{([\s\S]*?)\n\}\n\nasync function resetBusinessDataPlaneForBuildIfNeeded/)?.[1] || '';
  if (!bootstrapBody) {
    offenders.push('src/apps/business-os/app.js: could not inspect bootstrap boot path');
    return;
  }
  if (/await\s+startCriticalSyncCollections\s*\(/.test(bootstrapBody)) {
    offenders.push('src/apps/business-os/app.js: critical RxDB sync warmup blocks Business OS boot');
  }
  const loadModulesAt = bootstrapBody.indexOf('modules = await loadModules');
  const openModuleAt = bootstrapBody.indexOf('await openModule(');
  const warmupAt = bootstrapBody.indexOf('scheduleCriticalSyncWarmup();');
  if (warmupAt === -1) {
    offenders.push('src/apps/business-os/app.js: missing post-boot critical sync warmup scheduling');
  } else if (loadModulesAt !== -1 && warmupAt < loadModulesAt) {
    offenders.push('src/apps/business-os/app.js: critical sync warmup starts before module manifests load');
  } else if (openModuleAt !== -1 && warmupAt < openModuleAt) {
    offenders.push('src/apps/business-os/app.js: critical sync warmup starts before the visible workspace opens');
  }

  const catalogBody = appContent.match(/async function loadModuleCatalog\([^)]*\) \{([\s\S]*?)\n\}\n\nasync function readModuleCatalogProjection/)?.[1] || '';
  if (!catalogBody) {
    offenders.push('src/apps/business-os/app.js: could not inspect module catalog loading path');
    return;
  }
  const cachedReadAt = catalogBody.indexOf('const cachedCatalog = await readModuleCatalogProjection(coll);');
  const awaitedSyncStartAt = catalogBody.indexOf("await state.sync?.startCollection?.('business_module_catalog')");
  if (cachedReadAt === -1) {
    offenders.push('src/apps/business-os/app.js: module catalog boot path does not read cached RxDB projection first');
  } else if (awaitedSyncStartAt !== -1 && awaitedSyncStartAt < cachedReadAt) {
    offenders.push('src/apps/business-os/app.js: module catalog boot path waits for WebRTC before reading local RxDB projection');
  }
  const shellSeedAt = catalogBody.indexOf('loadPackagedModuleCatalog()');
  const awaitSyncStartAt = catalogBody.indexOf('await syncStart;');
  if (shellSeedAt === -1 || !/async function loadPackagedModuleCatalog\(/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: missing packaged shell module catalog seed for cold Business OS startup');
  } else if (awaitSyncStartAt !== -1 && awaitSyncStartAt < shellSeedAt) {
    offenders.push('src/apps/business-os/app.js: cold module catalog boot waits for WebRTC before trying the packaged shell seed');
  }
  if (!/loadModules\(\{\s*timeoutMs:\s*20000,\s*allowShellSeed:\s*false\s*\}\)/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: background module refresh must wait for the RxDB projection instead of reusing the shell seed');
  }
}

function assertConnectionSmokeRequiresAdvancedStatusBootBudget() {
  const smokePath = join(repoRoot, 'src/core/rxdb/tools/business_os_connection_modes_smoke.js');
  const smokeContent = readFileSync(smokePath, 'utf8');
  for (const marker of [
    'requiredAdvancedStatusVersion',
    'business-os-advanced-status-v1',
    'advancedStatusBootTimings',
    'statusShellVisibleMs',
    'advanced status shellVisibleMs exceeded budget',
  ]) {
    if (!smokeContent.includes(marker)) {
      offenders.push(`src/core/rxdb/tools/business_os_connection_modes_smoke.js: startup smoke missing advanced-status boot budget marker ${marker}`);
    }
  }
}

function expandFiles(paths) {
  const files = [];
  for (const path of paths) {
    collect(path, files);
  }
  return files;
}

function collect(path, files) {
  const stat = statSync(path, { throwIfNoEntry: false });
  if (!stat) return;
  if (isInactiveLegacyNotesnookPath(path)) return;
  if (stat.isFile()) {
    if (/\.(js|mjs|html|json|rs)$/.test(path)) files.push(path);
    return;
  }
  if (!stat.isDirectory()) return;
  const name = path.split(/[\\/]/).pop();
  if (excludedSegments.has(name)) return;
  for (const entry of readdirSync(path)) collect(join(path, entry), files);
}

function isInactiveLegacyNotesnookPath(path) {
  const rel = relative(appRoot, path);
  return rel.startsWith(`modules/notes/build`) || rel.startsWith(`modules/notes/notesnook-src`);
}

function isFrontendFile(file) {
  const rel = relative(appRoot, file);
  return rel && !rel.startsWith('..') && !rel.split(/[\\/]/).includes('scripts');
}
