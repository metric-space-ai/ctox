#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = dirname(fileURLToPath(import.meta.url));
const rxdbRoot = resolve(toolDir, '..');
const repoRoot = resolve(rxdbRoot, '..', '..');
const fixturePath = resolve(rxdbRoot, 'tests/fixtures/webrtc-rxdb-protocol.json');
const jsPath = resolve(repoRoot, 'apps/business-os/rxdb/src/protocol-contract.generated.mjs');
const rustPath = resolve(rxdbRoot, 'src/plugins/replication_webrtc/protocol_contract_generated.rs');
const fixture = JSON.parse(readFileSync(fixturePath, 'utf8'));

const js = `// Generated from src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs

export const CTOX_RXDB_PROTOCOL = ${json(fixture.protocol)};
export const CTOX_PROTOCOL_PHASE = ${json(fixture.phase)};
export const CTOX_REQUIRED_PROTOCOL_CAPABILITIES = Object.freeze(${json(fixture.requiredCapabilities)});
export const CTOX_PROTOCOL_ERROR_CODES = Object.freeze(${json(fixture.errorCodes)});
export const CTOX_SCHEMA_HASH_SOURCES = Object.freeze(${json(fixture.schemaHashSources)});
`;

const rust = `// Generated from src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs

pub(super) const CTOX_RXDB_PROTOCOL: &str = ${rustString(fixture.protocol)};
pub(super) const CTOX_REQUIRED_PROTOCOL_CAPABILITIES: &[&str] = &[
${fixture.requiredCapabilities.map((value) => `    ${rustString(value)},`).join('\n')}
];
pub(super) const CTOX_PROTOCOL_ERROR_MISSING: &str = ${rustString(fixture.errorCodes.protocolMissing)};
pub(super) const CTOX_PROTOCOL_ERROR_MISMATCH: &str = ${rustString(fixture.errorCodes.protocolMismatch)};
pub(super) const CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING: &str = ${rustString(fixture.errorCodes.capabilityMissing)};
pub(super) const CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH: &str = ${rustString(fixture.errorCodes.collectionMismatch)};
pub(super) const CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH: &str =
    ${rustString(fixture.errorCodes.schemaVersionMismatch)};
pub(super) const CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH: &str = ${rustString(fixture.errorCodes.schemaHashMismatch)};
pub(super) const CTOX_RXDB_RS_SCHEMA_HASH_SOURCE: &str = ${rustString(fixture.schemaHashSources.rxdbRs)};
`;

writeFileSync(jsPath, js);
writeFileSync(rustPath, rust);
console.log(`wrote ${jsPath}`);
console.log(`wrote ${rustPath}`);

function json(value) {
  return JSON.stringify(value, null, 2)
    .replace(/\n/g, '\n')
    .replace(/"([^"]+)":/g, '$1:');
}

function rustString(value) {
  return JSON.stringify(String(value));
}
