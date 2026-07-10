#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = dirname(fileURLToPath(import.meta.url));
const rxdbRoot = resolve(toolDir, '..');
const repoRoot = resolve(rxdbRoot, '..', '..');
const lifecycleFixturePath = resolve(
  rxdbRoot,
  'tests/fixtures/business-command-lifecycle-v2.json',
);
const protocolFixturePath = resolve(
  rxdbRoot,
  'tests/fixtures/webrtc-rxdb-protocol.json',
);
const jsPath = resolve(
  repoRoot,
  'apps/business-os/shared/command-lifecycle.generated.js',
);
const rustPath = resolve(
  repoRoot,
  'core/business_os/command_lifecycle_generated.rs',
);

const fixture = JSON.parse(readFileSync(lifecycleFixturePath, 'utf8'));
const protocolFixture = JSON.parse(readFileSync(protocolFixturePath, 'utf8'));
const capability = protocolFixture.optionalCapabilities?.commandLifecycle;
if (!capability) {
  throw new Error('webrtc-rxdb-protocol.json must define optionalCapabilities.commandLifecycle');
}

const transitions = Object.entries(fixture.allowedExecutionTransitions || {});
const js = `// Generated from src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json.
// Capability source: src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_business_command_lifecycle_contract.mjs

export const CTOX_COMMAND_CONTRACT_VERSION = ${Number(fixture.contractVersion)};
export const CTOX_COMMAND_LIFECYCLE_CAPABILITY = ${json(capability)};
export const CTOX_COMMAND_REPLICATION_PHASES = Object.freeze(${json(fixture.replicationPhases)});
export const CTOX_COMMAND_EXECUTION_MODES = Object.freeze(${json(fixture.executionModes)});
export const CTOX_COMMAND_EXECUTION_PHASES = Object.freeze(${json(fixture.executionPhases)});
export const CTOX_COMMAND_TERMINAL_STATUSES = Object.freeze(${json(fixture.terminalStatuses)});
export const CTOX_COMMAND_ERROR_CODES = Object.freeze(${json(fixture.errorCodes)});
export const CTOX_COMMAND_AUTHORIZATION = Object.freeze(${json(fixture.authorization)});
export const CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS = Object.freeze(${json(fixture.immutableIntentFields)});
export const CTOX_COMMAND_NATIVE_OWNED_FIELDS = Object.freeze(${json(fixture.nativeOwnedFields)});
export const CTOX_COMMAND_ALLOWED_EXECUTION_TRANSITIONS = Object.freeze(${json(fixture.allowedExecutionTransitions)});
export const CTOX_COMMAND_RESULT_ENVELOPE = Object.freeze(${json(fixture.resultEnvelope)});
`;

const rust = `// Generated from src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json.
// Capability source: src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_business_command_lifecycle_contract.mjs

#![allow(dead_code)]

pub(crate) const CTOX_COMMAND_CONTRACT_VERSION: u32 = ${Number(fixture.contractVersion)};
pub(crate) const CTOX_COMMAND_LIFECYCLE_CAPABILITY: &str = ${rustString(capability)};
${rustSlice('CTOX_COMMAND_REPLICATION_PHASES', fixture.replicationPhases)}
${rustSlice('CTOX_COMMAND_EXECUTION_MODES', fixture.executionModes)}
${rustSlice('CTOX_COMMAND_EXECUTION_PHASES', fixture.executionPhases)}
${rustSlice('CTOX_COMMAND_TERMINAL_STATUSES', fixture.terminalStatuses)}
${rustSlice('CTOX_COMMAND_ERROR_CODES', fixture.errorCodes)}
pub(crate) const CTOX_COMMAND_DEFAULT_AUTHORIZATION_REQUIREMENT: &str = ${rustString(fixture.authorization.defaultRequirement)};
pub(crate) const CTOX_COMMAND_OFFLINE_INTENT_ALLOWED: bool = ${Boolean(fixture.authorization.offlineIntentAllowed)};
${rustSlice('CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS', fixture.immutableIntentFields)}
${rustSlice('CTOX_COMMAND_NATIVE_OWNED_FIELDS', fixture.nativeOwnedFields)}
${rustSlice('CTOX_COMMAND_RESULT_REQUIRED_FIELDS', fixture.resultEnvelope.required)}
${rustSlice('CTOX_COMMAND_RESULT_FIELDS', fixture.resultEnvelope.fields)}
pub(crate) fn execution_transition_allowed(from: &str, to: &str) -> bool {
    match from {
${transitions.map(([from, targets]) => `        ${rustString(from)} => matches!(to, ${targets.map(rustString).join(' | ')}),`).join('\n')}
        _ => false,
    }
}
`;

writeFileSync(jsPath, js);
writeFileSync(rustPath, rust);
execFileSync('rustfmt', [rustPath], { stdio: 'pipe' });
console.log(`wrote ${jsPath}`);
console.log(`wrote ${rustPath}`);

function json(value) {
  return JSON.stringify(value, null, 2)
    .replace(/"([^"\\]+)":/g, '$1:');
}

function rustString(value) {
  return JSON.stringify(String(value));
}

function rustSlice(name, values) {
  return `pub(crate) const ${name}: &[&str] = &[\n${values.map((value) => `    ${rustString(value)},`).join('\n')}\n];`;
}
