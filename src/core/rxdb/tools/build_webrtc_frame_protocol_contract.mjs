#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = dirname(fileURLToPath(import.meta.url));
const rxdbRoot = resolve(toolDir, '..');
const repoRoot = resolve(rxdbRoot, '..', '..');
const fixturePath = resolve(rxdbRoot, 'tests/fixtures/webrtc-frame-protocol.json');
const jsPath = resolve(repoRoot, 'apps/business-os/rxdb/src/frame-contract.generated.mjs');
const rustPath = resolve(rxdbRoot, 'src/plugins/replication_webrtc/frame_contract_generated.rs');
const fixture = JSON.parse(readFileSync(fixturePath, 'utf8'));

const js = `// Generated from src/core/rxdb/tests/fixtures/webrtc-frame-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_frame_protocol_contract.mjs

export const CTOX_FRAME_PROTOCOL = ${json(fixture.protocol)};
export const MAX_INLINE_FRAME_BYTES = ${number(fixture.maxInlineFrameBytes)};
export const MAX_CHUNK_CHARS = ${number(fixture.maxChunkBytes)};
export const MAX_TRANSFER_BYTES = ${number(fixture.maxTransferBytes)};
export const FRAME_ACK_WINDOW = ${number(fixture.ackWindow)};
export const MAX_FRAME_RETRIES = ${number(fixture.maxFrameRetries)};
`;

const rust = `// Generated from src/core/rxdb/tests/fixtures/webrtc-frame-protocol.json.
// Run: node src/core/rxdb/tools/build_webrtc_frame_protocol_contract.mjs

pub(super) const CTOX_FRAME_PROTOCOL: &str = ${json(fixture.protocol)};
pub(super) const MAX_INLINE_FRAME_BYTES: usize = ${number(fixture.maxInlineFrameBytes)};
pub(super) const MAX_CHUNK_BYTES: usize = ${number(fixture.maxChunkBytes)};
pub(super) const MAX_TRANSFER_BYTES: usize = ${number(fixture.maxTransferBytes)};
pub(super) const MAX_FRAME_RETRIES: usize = ${number(fixture.maxFrameRetries)};
pub(super) const FRAME_ACK_WINDOW: usize = ${number(fixture.ackWindow)};
`;

writeFileSync(jsPath, js);
writeFileSync(rustPath, rust);
console.log(`wrote ${jsPath}`);
console.log(`wrote ${rustPath}`);

function json(value) {
  return JSON.stringify(String(value));
}

function number(value) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`invalid frame contract integer: ${value}`);
  }
  return String(value);
}
