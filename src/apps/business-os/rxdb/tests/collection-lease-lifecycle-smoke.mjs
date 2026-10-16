// V4 regression contract: execute the real kernel invariants; legacy transport is retired.
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const tests=['../../shared/sync-collection-registry.test.mjs','../../shared/documents-facade.test.mjs','../../kernel/tests/s9-delivery.test.mjs','../../kernel/tests/durability.test.mjs','../../kernel/tests/command-state.test.mjs'].map(path=>fileURLToPath(new URL(path,import.meta.url)));
execFileSync(process.execPath,['--test','--test-concurrency=2',...tests],{stdio:'inherit',timeout:120000});
