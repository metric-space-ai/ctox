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
  join(repoRoot, 'src/core/business_os/store.rs'),
];

const excludedSegments = new Set(['vendor', 'output', 'installed-modules']);
const forbidden = [
  { name: 'frontend-business-os-http-api', pattern: /\/api\/business-os(?:\/|[`'")])/, frontendOnly: true },
  { name: 'frontend-business-os-http-api-concat', pattern: /\/api\/['"`]\s*\+\s*['"`]business-os/, frontendOnly: true },
  { name: 'frontend-rxdb-http-pull', pattern: /\/rxdb\/pull/ },
  { name: 'frontend-command-http-post', pattern: /\/commands[`'")]/ },
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

for (const file of expandFiles(scannedRoots)) {
  const rel = relative(repoRoot, file);
  const content = readFileSync(file, 'utf8');
  for (const rule of forbidden) {
    if (rule.frontendOnly && !isFrontendFile(file)) continue;
    if (rule.pattern.test(content)) offenders.push(`${rel}: ${rule.name}`);
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

function assertAdvancedStatusInterfaceExists() {
  const appPath = join(appRoot, 'app.js');
  const syncPath = join(appRoot, 'shared/sync.js');
  const appContent = readFileSync(appPath, 'utf8');
  const syncContent = readFileSync(syncPath, 'utf8');
  if (!/CTOX_BUSINESS_OS_STATUS/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: missing advanced Business OS status interface');
  }
  if (!/business-os-advanced-status-v1/.test(appContent)) {
    offenders.push('src/apps/business-os/app.js: missing advanced status version marker');
  }
  for (const requiredCheck of [
    'workspaceNotLoading',
    'dataPlaneWebrtc',
    'moduleCatalogAvailable',
    'requiredCollectionsConnected',
    'requiredCollectionsInitialSyncComplete',
    'noStalledReconnect',
  ]) {
    if (!appContent.includes(requiredCheck)) {
      offenders.push(`src/apps/business-os/app.js: advanced status missing ${requiredCheck}`);
    }
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
  if (!/peerSessions/.test(appContent) || !/remotePeerSession/.test(syncContent) || !/peerSessionSeenAt/.test(syncContent)) {
    offenders.push('src/apps/business-os: advanced status missing peer-session diagnostics');
  }
  if (!/ctox-checkpoint-epoch-v1/.test(syncContent) || !/remoteCheckpoint/.test(syncContent) || !/sanitizeAdvancedStatusRemoteCheckpoint/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing remote checkpoint epoch diagnostics');
  }
  if (!/CtoxCheckpointProtocolError/.test(syncContent) || !/ctox_checkpoint_epoch_missing/.test(syncContent) || !/checkpointErrors/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing typed checkpoint protocol errors');
  }
  if (!/CtoxSchemaProtocolError/.test(syncContent) || !/ctox_schema_hash_mismatch/.test(syncContent) || !/schemaErrors/.test(appContent) || !/noSchemaProtocolErrors/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing typed schema protocol errors');
  }
  if (!/CtoxReplicationIoError/.test(syncContent) || !/ctox_replication_pull_failed/.test(syncContent) || !/ctox_replication_push_failed/.test(syncContent) || !/replicationErrors/.test(appContent) || !/noReplicationIoErrors/.test(appContent)) {
    offenders.push('src/apps/business-os: advanced status missing typed replication I/O errors');
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
  for (const marker of ['readStoredFileFromChunks', 'file-integrity.js?v=20260522-file-chunk-integrity4']) {
    if (!fileViewer.includes(marker)) {
      offenders.push(`src/apps/business-os/desktop-apps/file-viewer/app.js: file chunk integrity missing ${marker}`);
    }
  }
  for (const marker of ['FILE_CONTENT_HASH_SCHEME', 'FILE_CHUNK_HASH_SCHEME', 'chunk_hash', 'base64ToBytes', 'readStoredFileFromChunks']) {
    if (!explorer.includes(marker)) {
      offenders.push(`src/apps/business-os/desktop-apps/explorer/app.js: uploaded file chunk contract missing ${marker}`);
    }
  }
  for (const marker of ['readStoredFileFromChunks', 'file-integrity.js?v=20260522-file-chunk-integrity4', 'contentHashScheme']) {
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
  if (stat.isFile()) {
    if (/\.(js|mjs|html|json|rs)$/.test(path)) files.push(path);
    return;
  }
  if (!stat.isDirectory()) return;
  const name = path.split(/[\\/]/).pop();
  if (excludedSegments.has(name)) return;
  for (const entry of readdirSync(path)) collect(join(path, entry), files);
}

function isFrontendFile(file) {
  const rel = relative(appRoot, file);
  return rel && !rel.startsWith('..') && !rel.split(/[\\/]/).includes('scripts');
}
